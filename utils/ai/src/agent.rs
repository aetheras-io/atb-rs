use crate::openai::{
    self, CustomClient, GPT4_O,
    responses::{
        FunctionTool, FunctionToolCall, InputFunctionCallOutput, InputItem, InputMessage,
        MessageRole, OutputItem, RequestPayloadBuilder, ToolChoice, ToolChoiceOption,
        streaming::ResponseStreamEvent,
    },
};

use std::collections::HashMap;
use std::sync::Arc;

use atb_types::Uuid;
use eventsource_stream::{Event as SseEvent, Eventsource};
use futures::{
    future::{self, BoxFuture},
    stream::{StreamExt, TryStreamExt},
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub const AGENT_DONE_EVENT: &str = "atb.ai.done";
pub const AGENT_ERROR_EVENT: &str = "atb.ai.error";

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("json parse error {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("UTF-8 conversion error {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("Reqwest error {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("SSE Stream Error: {0}")]
    EventSource(String),

    #[error("Other Error: {0}")]
    Other(String),
}

type ToolHandlerFn = dyn Fn(&FunctionToolCall) -> BoxFuture<'static, anyhow::Result<serde_json::Value>>
    + Send
    + Sync;

/// A no-op tool handler that returns a default success JSON asynchronously.
pub fn null_tool_handler(
    _: &openai::responses::FunctionToolCall,
) -> BoxFuture<'static, anyhow::Result<serde_json::Value>> {
    Box::pin(future::ready(Ok(serde_json::json!({ "success": true }))))
}

#[derive(Debug, Clone, Default)]
pub struct AgentContext {
    pub(crate) conversation: Vec<InputItem>,
    pub(crate) tool_results: Vec<serde_json::Value>,
    /// Index marking the end of the initial seeded context.
    /// All items at indices >= baseline_len are considered runtime-generated.
    pub(crate) baseline_len: usize,
}

impl AgentContext {
    /// Returns an iterator over borrowed message items from the conversation.
    pub fn messages_iter(&self) -> impl Iterator<Item = &InputMessage> {
        self.conversation.iter().filter_map(|item| match item {
            InputItem::Message(msg) => Some(msg),
            _ => None,
        })
    }

    /// Returns the full conversation history, including messages, tool calls, and outputs.
    pub fn history(&self) -> &[InputItem] {
        &self.conversation
    }

    /// Read-only view of tool results.
    pub fn tool_results(&self) -> &[serde_json::Value] {
        &self.tool_results
    }

    /// Read-only view of baseline length.
    pub fn baseline_len(&self) -> usize {
        self.baseline_len
    }

    /// Clears runtime history, restoring the conversation back to the baseline
    /// that was set at seed or via `mark_baseline`. Also clears tool_results.
    pub fn clear_history(&mut self) {
        self.conversation.truncate(self.baseline_len);
        self.tool_results.clear();
    }
}

#[derive(Clone)]
pub struct Agent {
    client: CustomClient,
    run_id: Uuid,
    system_prompt: String,
    user_data: Option<String>,
    model: String,
    max_turns: usize,
    tools: Vec<FunctionTool>,
    tool_handlers: HashMap<String, Arc<ToolHandlerFn>>,
}

impl Agent {
    pub fn new(
        run_id: Uuid,
        api_key: &str,
        system_prompt: &str,
        user_data: Option<String>,
    ) -> Self {
        let client = CustomClient::new(api_key);
        Self {
            client,
            run_id,
            system_prompt: system_prompt.to_owned(),
            user_data,
            model: GPT4_O.to_owned(),
            max_turns: 10,
            tools: vec![],
            tool_handlers: HashMap::new(),
        }
    }

    /// Create a fresh AgentContext seeded with the system prompt and optional user data.
    pub fn seed_context(&self) -> AgentContext {
        let mut conversation_start =
            vec![(MessageRole::Developer, self.system_prompt.to_owned()).into()];
        if let Some(ref data) = self.user_data {
            conversation_start.push((MessageRole::Developer, data.clone()).into());
        }
        let baseline_len = conversation_start.len();
        AgentContext {
            conversation: conversation_start,
            tool_results: vec![],
            baseline_len,
        }
    }

    /// Returns a reference to the immutable run_id for this agent.
    pub fn run_id(&self) -> &Uuid {
        &self.run_id
    }

    /// Sets the maximum number of turns the agent will execute
    /// during a streaming interaction before stopping. Default is 10.
    pub fn set_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns.max(1);
        self
    }

    pub fn with_tool(mut self, tool: FunctionTool, handler: Arc<ToolHandlerFn>) -> Self {
        self.tool_handlers.insert(tool.name.clone(), handler);
        self.tools.push(tool);
        self
    }

    /// Set the model identifier used for responses (default: `gpt-4o`).
    pub fn with_model<I: Into<String>>(mut self, model: I) -> Self {
        self.model = model.into();
        self
    }

    pub async fn respond_stream(
        &mut self,
        mut ctx: AgentContext,
        user_input: &str,
    ) -> Result<AgentRun, AgentError> {
        ctx.conversation
            .push((MessageRole::User, user_input.to_owned()).into());

        let (tx, rx) = mpsc::unbounded_channel();

        let agent = self.clone();
        let handle = tokio::spawn(async move {
            let mut turns: usize = 0;
            loop {
                if turns >= agent.max_turns {
                    tracing::warn!(
                        "Max turn count reached ({}). Stopping agent loop.",
                        agent.max_turns
                    );
                    break;
                }
                turns += 1;

                match agent.run_once_stream(&mut ctx, &tx).await {
                    Ok(mut tools) => {
                        if !tools.is_empty() {
                            for call in tools.drain(..) {
                                let handler = agent.tool_handlers.get(&call.name);
                                if let Some(handler) = handler {
                                    match handler(&call).await {
                                        Ok(json_out) => {
                                            let output = InputFunctionCallOutput {
                                                call_id: call.call_id.clone(),
                                                output: json_out.to_string(),
                                                id: None,
                                                status: None,
                                            };
                                            ctx.conversation.push(InputItem::FunctionCall(call));
                                            ctx.conversation
                                                .push(InputItem::FunctionCallOutput(output));
                                            ctx.tool_results.push(json_out);
                                        }
                                        Err(err) => {
                                            let _ = tx.send(Ok(SseEvent {
                                                event: AGENT_ERROR_EVENT.to_owned(),
                                                data: err.to_string(),
                                                ..Default::default()
                                            }));
                                            return ctx;
                                        }
                                    }
                                }
                            }
                            continue;
                        } else {
                            break;
                        }
                    }

                    Err(e) => {
                        tracing::trace!("run_once_stream error: {e:?}");
                        let _ = tx.send(Ok(SseEvent {
                            event: AGENT_ERROR_EVENT.to_owned(),
                            data: e.to_string(),
                            ..Default::default()
                        }));
                        return ctx;
                    }
                }
            }

            let _ = tx.send(Ok(SseEvent {
                event: AGENT_DONE_EVENT.to_owned(),
                data: serde_json::json!({
                    "run_id": agent.run_id.to_string(),
                    "turns": turns,
                })
                .to_string(),
                ..Default::default()
            }));

            ctx
        });

        Ok(AgentRun {
            stream: tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
            handle,
        })
    }

    /// Run the agent to completion without streaming events.
    ///
    /// - Appends the `user_input` to the conversation
    /// - Repeatedly queries the model and executes any returned tool calls
    /// - Returns the final `AgentContext` once the model no longer requests tools
    pub async fn respond(
        &self,
        mut ctx: AgentContext,
        user_input: &str,
    ) -> Result<AgentContext, AgentError> {
        ctx.conversation
            .push((MessageRole::User, user_input.to_owned()).into());

        let mut turns: usize = 0;
        loop {
            if turns >= self.max_turns {
                tracing::warn!(
                    "Max turn count reached ({}). Stopping agent loop.",
                    self.max_turns
                );
                break;
            }
            turns += 1;

            match self.run_once(&mut ctx).await {
                Ok(mut tools) => {
                    if tools.is_empty() {
                        break;
                    }

                    for call in tools.drain(..) {
                        if let Some(handler) = self.tool_handlers.get(&call.name) {
                            match handler(&call).await {
                                Ok(json_out) => {
                                    let output = InputFunctionCallOutput {
                                        call_id: call.call_id.clone(),
                                        output: json_out.to_string(),
                                        id: None,
                                        status: None,
                                    };
                                    ctx.conversation.push(InputItem::FunctionCall(call));
                                    ctx.conversation.push(InputItem::FunctionCallOutput(output));
                                    ctx.tool_results.push(json_out);
                                }
                                Err(err) => {
                                    return Err(AgentError::Other(err.to_string()));
                                }
                            }
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(ctx)
    }

    async fn run_once_stream(
        &self,
        ctx: &mut AgentContext,
        tx: &mpsc::UnboundedSender<Result<SseEvent, AgentError>>,
    ) -> Result<Vec<FunctionToolCall>, AgentError> {
        let conversation = ctx.history().to_vec();
        let req = RequestPayloadBuilder::default()
            .model(self.model.clone())
            .input(conversation.into())
            .stream(true)
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto))
            .build()
            .expect("builder builds. qed");

        let resp = self
            .client
            .build_request(reqwest::Method::POST, "responses")
            .json(&req)
            .send()
            .await?
            .error_for_status()?;

        let mut stream = resp.bytes_stream().eventsource().map(|res| {
            res.map_err(|e| match e {
                eventsource_stream::EventStreamError::Transport(reqwest_err) => {
                    AgentError::Reqwest(reqwest_err)
                }
                e => AgentError::EventSource(e.to_string()),
            })
        });

        let mut tool_calls = vec![];
        while let Some(evt) = stream.try_next().await? {
            match serde_json::from_str::<ResponseStreamEvent>(&evt.data).unwrap() {
                ResponseStreamEvent::OutputTextDelta(_) => {
                    let _ = tx.send(Ok(evt));
                }
                ResponseStreamEvent::OutputTextDone(output) => {
                    use openai::responses::MessageRole;
                    let _ = tx.send(Ok(evt));
                    ctx.conversation
                        .push((MessageRole::Assistant, output.text).into());
                }
                ResponseStreamEvent::OutputItemAdded(added) => {
                    if let OutputItem::FunctionCall(_) = added.item {
                        let _ = tx.send(Ok(evt));
                    }
                }
                ResponseStreamEvent::OutputItemDone(done) => {
                    if let OutputItem::FunctionCall(call) = done.item {
                        let _ = tx.send(Ok(evt));
                        tool_calls.push(call);
                    }
                }
                ResponseStreamEvent::FileSearchCallSearching(_) => {
                    let _ = tx.send(Ok(evt));
                }
                ResponseStreamEvent::FileSearchCallCompleted(_) => {
                    let _ = tx.send(Ok(evt));
                }
                ResponseStreamEvent::Error(err_event) => {
                    return Err(AgentError::Other(err_event.error.message));
                }
                ResponseStreamEvent::Completed(_) => {
                    if tool_calls.is_empty() {
                        let _ = tx.send(Ok(evt));
                    }
                    return Ok(tool_calls);
                }
                evt => tracing::trace!("{evt:?}"),
            }
        }

        Ok(tool_calls)
    }

    /// Single non-streaming turn: sends conversation, updates context with any assistant messages,
    /// and returns any function tool calls to execute. Returns empty when there are no tools.
    async fn run_once(&self, ctx: &mut AgentContext) -> Result<Vec<FunctionToolCall>, AgentError> {
        let conversation = ctx.history().to_vec();
        let req = RequestPayloadBuilder::default()
            .model(self.model.clone())
            .input(conversation.into())
            .stream(false)
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto))
            .build()
            .expect("builder builds. qed");

        let resp = self
            .client
            .build_request(reqwest::Method::POST, "responses")
            .json(&req)
            .send()
            .await?
            .error_for_status()?;

        let body: openai::responses::Response = resp.json().await?;

        // Collect any tool calls to execute next turn
        let mut tool_calls = Vec::new();

        for item in body.output.into_iter() {
            match item {
                OutputItem::Message(msg) => {
                    // Aggregate any text parts into a single assistant message
                    let mut text = String::new();
                    for part in msg.content {
                        if let openai::responses::OutputContent::OutputText(t) = part {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(&t.text);
                        }
                    }
                    if !text.is_empty() {
                        ctx.conversation.push((MessageRole::Assistant, text).into());
                    }
                }
                OutputItem::FunctionCall(call) => {
                    tool_calls.push(call);
                }
                OutputItem::FileSearchCall(_) => {
                    // Currently ignored for non-streaming mode; nothing to add to context
                }
            }
        }

        Ok(tool_calls)
    }
}

/// Handle returned by `respond_stream` containing the event stream,
///
/// - Owns the background agent task via `handle` which yields the final `AgentContext` when the
///   model run completes.
/// - Exposes a `stream` of intermediate `SseEvent`s you can forward to clients or inspect for
///   logging/metrics.
pub struct AgentRun {
    pub(crate) stream:
        tokio_stream::wrappers::UnboundedReceiverStream<Result<SseEvent, AgentError>>,
    pub(crate) handle: JoinHandle<AgentContext>,
}

impl AgentRun {
    /// Consume this `AgentRun` and return its raw parts for manual composition.
    ///
    /// This lets callers build custom pipelines without exposing struct fields:
    /// ```rust,no_run
    /// use axum::response::sse::{Event, Sse};
    /// use futures::StreamExt;
    /// use anyhow::Result;
    /// async fn handler(mut agent: Agent, ctx: AgentContext, user_input: String) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>> {
    ///     let run = agent.respond_stream(ctx, &user_input).await?;
    ///     let (stream, handle) = run.into_parts();
    ///     let stream = async_stream::stream! {
    ///         while let Some(res) = stream.next().await {
    ///             match res {
    ///                 Ok(ev) => {
    ///                     // Optionally record metrics/logging here before sending
    ///                     yield Ok(Event::default().event(ev.event).data(ev.data));
    ///                 }
    ///                 Err(e) => {
    ///                     // Convert internal error into an SSE error event
    ///                     yield Ok(Event::default().event("error").data(e.to_string()));
    ///                 }
    ///             }
    ///         }
    ///         // Stream ended; now await the final context and use it
    ///         match run.handle.await {
    ///             Ok(final_ctx) => {
    ///                 // e.g., persist final_ctx or update caches
    ///                 let _ = final_ctx.history().len();
    ///             }
    ///             Err(join_err) => {
    ///                 // Join error from background task
    ///                 let _ = join_err.to_string();
    ///             }
    ///         }
    ///     };
    ///     Ok(Sse::new(stream))
    /// }
    /// ```
    ///
    /// ```rust,no_run
    /// use axum::response::sse::{Event, Sse};
    /// use futures::StreamExt;
    /// # async fn route(run: AgentRun) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    ///     let (stream, handle) = run.into_parts();
    ///     let stream = async_stream::stream! {
    ///         futures::pin_mut!(stream);
    ///         while let Some(res) = stream.next().await {
    ///             match res {
    ///                 Ok(ev) => yield Ok(Event::default().event(ev.event).data(ev.data)),
    ///                 Err(e) => yield Ok(Event::default().event("error").data(e.to_string())),
    ///             }
    ///         }
    ///         let _ = handle.await; // await and optionally use the final AgentContext
    ///     };
    ///     Sse::new(stream)
    /// }
    /// ```
    pub fn into_parts(
        self,
    ) -> (
        tokio_stream::wrappers::UnboundedReceiverStream<Result<SseEvent, AgentError>>,
        JoinHandle<AgentContext>,
    ) {
        (self.stream, self.handle)
    }

    /// Consume this run and produce a stream that forwards events and runs a finalizer.
    ///
    /// This method:
    /// - Forwards every `Result<SseEvent, AgentError>` from the internal `stream`.
    /// - After the stream ends, awaits the `handle` for the final `AgentContext`.
    /// - Invokes the provided async `on_done` callback with the final context.
    ///
    /// Common use-case: finalize into an HTTP SSE stream while persisting the context at the end.
    ///
    /// Axum example returning `Sse<...>`:
    ///
    /// ```rust,no_run
    /// use axum::response::sse::{Event, Sse};
    /// use futures::StreamExt;
    /// # use anyhow::Result;
    /// # async fn save_ctx(_pool: &(), _ctx: &AgentContext) -> Result<(), ()> { Ok(()) }
    /// # async fn route(mut agent: Agent, ctx: AgentContext, user_input: String, pool: ()) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>> {
    ///     let run = agent.respond_stream(ctx, &user_input).await?;
    ///     let stream = run
    ///         .finalize_with(move |final_ctx| {
    ///             let pool = pool; // capture things by move
    ///             async move {
    ///                 let _ = save_ctx(&pool, &final_ctx).await;
    ///             }
    ///         })
    ///         .map(|res| match res {
    ///             Ok(ev) => Ok(Event::default().event(ev.event).data(ev.data)),
    ///             Err(e) => Ok(Event::default().event("error").data(e.to_string())),
    ///         });
    ///     Ok(Sse::new(stream))
    /// }
    /// ```
    pub fn finalize_with<F, Fut>(
        self,
        on_done: F,
    ) -> impl futures::Stream<Item = Result<SseEvent, AgentError>>
    where
        F: FnOnce(AgentContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        async_stream::stream! {
            let (mut stream, handle) = self.into_parts();

            while let Some(evt) = stream.next().await {
                yield evt;
            }

            match handle.await {
                Ok(final_ctx) => on_done(final_ctx).await,
                Err(e) => tracing::error!("agent join error: {e}"),
            }
        }
    }
}
