use crate::openai::{
    self, CustomClient, GPT4_O,
    responses::{
        FunctionTool, FunctionToolCall, Includable, InputFunctionCallOutput, InputItem,
        InputMessage, MessageRole, OutputItem, ReasoningConfig, ReasoningEffortConfig,
        RequestPayloadBuilder, RequestPayloadBuilderError, ToolChoice, ToolChoiceOption,
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

    #[error("Builder Error: {0}")]
    RequestPayload(#[from] RequestPayloadBuilderError),
}

type ToolHandlerFn = dyn Fn(&FunctionToolCall) -> BoxFuture<'static, anyhow::Result<serde_json::Value>>
    + Send
    + Sync;

/// User hook to tweak the request builder while still allowing the agent to reassert invariants.
type RequestCustomizerFn =
    dyn Fn(&mut RequestPayloadBuilder) -> &mut RequestPayloadBuilder + Send + Sync;

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
    request_customizer: Arc<RequestCustomizerFn>,
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
            request_customizer: Arc::new(|b| b),
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

    /// Provide a hook to customize the request builder; invariants are re-applied after the hook runs.
    pub fn with_request_customizer<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut RequestPayloadBuilder) -> &mut RequestPayloadBuilder + Send + Sync + 'static,
    {
        self.request_customizer = Arc::new(f);
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
        let mut builder = RequestPayloadBuilder::default();
        builder
            .store(false)
            .include(vec![Includable::ReasoningEncryptedContent])
            .model(self.model.clone())
            .input(conversation.into())
            .stream(true)
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto))
            .reasoning(ReasoningConfig {
                effort: Some(ReasoningEffortConfig::Minimal),
                summary: None,
            });

        // Apply caller hook, then re-assert agent-owned invariants.
        let req = (self.request_customizer)(&mut builder)
            .stream(true)
            .model(self.model.clone())
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .include(vec![Includable::ReasoningEncryptedContent])
            .build()?;

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
        let mut builder = RequestPayloadBuilder::default();
        builder
            .store(false)
            .include(vec![Includable::ReasoningEncryptedContent])
            .model(self.model.clone())
            .input(conversation.into())
            .stream(false)
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto));

        let req = (self.request_customizer)(&mut builder)
            .stream(false)
            .model(self.model.clone())
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .include(vec![Includable::ReasoningEncryptedContent])
            .build()?;

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
                OutputItem::Reasoning(_) => {
                    // Do nothing for now
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::responses::RequestPayload;

    #[test]
    fn customizer_cannot_override_agent_invariants() {
        let run_id = Uuid::from_u128(0);

        let tool = FunctionTool {
            name: "demo_tool".to_string(),
            parameters: None,
            strict: None,
            description: Some("demo".to_string()),
        };

        let agent = Agent::new(run_id, "test-key", "sys", None)
            .with_model("agent-model")
            .with_tool(tool, Arc::new(null_tool_handler))
            .with_request_customizer(|b| {
                // Caller tries to override invariants the agent owns.
                b.stream(false)
                    .model("caller-model")
                    .tools(vec![])
                    .include(vec![])
            });

        let mut builder = RequestPayloadBuilder::default();
        builder
            .store(false)
            .include(vec![Includable::ReasoningEncryptedContent])
            .model(agent.model.clone())
            .stream(true)
            .tools(agent.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto))
            .reasoning(ReasoningConfig {
                effort: Some(ReasoningEffortConfig::Minimal),
                summary: None,
            });

        let mut base = builder;
        let req: RequestPayload = (agent.request_customizer)(&mut base)
            // Agent re-applies invariants after the hook.
            .stream(true)
            .model(agent.model.clone())
            .tools(agent.tools.iter().cloned().map(Into::into).collect())
            .include(vec![Includable::ReasoningEncryptedContent])
            .build()
            .expect("builder builds");

        assert_eq!(req.stream, Some(true));
        assert_eq!(req.model, agent.model);
        assert_eq!(req.tools.as_ref().map(|t| t.len()), Some(agent.tools.len()));
        assert_eq!(
            req.include,
            Some(vec![Includable::ReasoningEncryptedContent])
        );
    }
}
