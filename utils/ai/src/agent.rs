use crate::openai::{
    self, CustomClient, GPT4_O,
    responses::{
        FunctionTool, FunctionToolCall, InputFunctionCallOutput, InputItem, InputMessage,
        MessageRole, OutputItem, RequestPayloadBuilder, ToolChoice, ToolChoiceOption,
        streaming::ResponseStreamEvent,
    },
};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use atb_types::Uuid;
use eventsource_stream::{Event as SseEvent, Eventsource};
use futures::{
    future::{self, BoxFuture},
    stream::{StreamExt, TryStreamExt},
};
use tokio::sync::mpsc;

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
    pub conversation: Vec<InputItem>,
    pub tool_results: Vec<serde_json::Value>,
    /// Index marking the end of the initial seeded context.
    /// All items at indices >= baseline_len are considered runtime-generated.
    pub baseline_len: usize,
}

impl AgentContext {
    /// Returns cloned message items (InputMessage) from the conversation.
    pub fn messages(&self) -> Vec<InputMessage> {
        self.conversation
            .iter()
            .filter_map(|item| {
                if let InputItem::Message(msg) = item {
                    Some(msg.clone())
                } else {
                    None
                }
            })
            .collect()
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
            max_turns: 10,
            tools: vec![],
            tool_handlers: HashMap::new(),
        }
    }

    /// Create a fresh AgentContext seeded with the system prompt and optional user data.
    pub fn seed_context(&self) -> Arc<Mutex<AgentContext>> {
        let mut conversation_start =
            vec![(MessageRole::Developer, self.system_prompt.to_owned()).into()];
        if let Some(ref data) = self.user_data {
            conversation_start.push((MessageRole::Developer, data.clone()).into());
        }
        let baseline_len = conversation_start.len();
        Arc::new(Mutex::new(AgentContext {
            conversation: conversation_start,
            tool_results: vec![],
            baseline_len,
        }))
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

    // Conversation helpers removed; callers should operate on AgentContext directly.

    /// Usage Example
    ///
    /// ```rust
    /// let ctx = agent.seed_context();
    /// let mut stream = agent
    ///     .respond_stream(ctx.clone(), &chat.message)
    ///     .await
    ///     .unwrap();
    ///
    /// Ok(Sse::new(async_stream::stream! {
    ///     while let Some(Ok(evt)) = stream.next().await {
    ///         if evt.event == AGENT_DONE_EVENT {
    ///             // doesn't need to be cached anymore; it's considered done
    ///             // cache.invalidate(&user_id);
    ///         }
    ///         yield Ok(Event::default().event(evt.event).data(evt.data))
    ///     }
    /// }))
    /// ```
    pub async fn respond_stream(
        &mut self,
        ctx: Arc<Mutex<AgentContext>>,
        user_input: &str,
    ) -> Result<impl futures::Stream<Item = Result<SseEvent, AgentError>>, AgentError> {
        ctx.lock()
            .map_err(|e| AgentError::Other(e.to_string()))?
            .conversation
            .push((MessageRole::User, user_input.to_owned()).into());

        let (tx, rx) = mpsc::unbounded_channel();

        let agent = self.clone();
        let ctx_for_task = ctx.clone();
        tokio::spawn(async move {
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
                match agent.run_once(&ctx_for_task, &tx).await {
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
                                            let mut ctx_guard = ctx_for_task.lock().unwrap();
                                            ctx_guard
                                                .conversation
                                                .push(InputItem::FunctionCall(call));
                                            ctx_guard
                                                .conversation
                                                .push(InputItem::FunctionCallOutput(output));
                                            ctx_guard.tool_results.push(json_out);
                                        }
                                        Err(err) => {
                                            let _ = tx.send(Ok(SseEvent {
                                                event: AGENT_ERROR_EVENT.to_owned(),
                                                data: err.to_string(),
                                                ..Default::default()
                                            }));
                                            // Send error event and end the task
                                            return;
                                        }
                                    }
                                }
                            }
                            //run_once again to allow the agent to respond to the tool calls
                            continue;
                        } else {
                            break;
                        }
                    }

                    Err(e) => {
                        tracing::trace!("run_once error: {e:?}");
                        let _ = tx.send(Ok(SseEvent {
                            event: AGENT_ERROR_EVENT.to_owned(),
                            data: e.to_string(),
                            ..Default::default()
                        }));
                        // Send error event and end the task
                        return;
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
        });

        Ok(Box::pin(
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        ))
    }

    async fn run_once(
        &self,
        ctx: &Arc<Mutex<AgentContext>>,
        tx: &mpsc::UnboundedSender<Result<SseEvent, AgentError>>,
    ) -> Result<Vec<FunctionToolCall>, AgentError> {
        let conversation = {
            let guard = ctx.lock().unwrap();
            guard.conversation.clone()
        };
        // tracing::info!("AI CONVERSATION: {conversation:?}");
        let req = RequestPayloadBuilder::default()
            .model(GPT4_O)
            .input(conversation.into())
            .stream(true)
            .tools(self.tools.iter().cloned().map(Into::into).collect())
            .tool_choice(ToolChoice::Option(ToolChoiceOption::Auto))
            .build()
            .expect("builder builds. qed");

        // tracing::trace!("req: {}", serde_json::to_string_pretty(&req).unwrap());
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
            // tracing::info!("LLM event {}", evt.event);
            // tracing::info!("LLM data {}", evt.data);
            match serde_json::from_str::<ResponseStreamEvent>(&evt.data).unwrap() {
                ResponseStreamEvent::OutputTextDelta(_) => {
                    let _ = tx.send(Ok(evt));
                }
                ResponseStreamEvent::OutputTextDone(output) => {
                    use openai::responses::MessageRole;
                    let _ = tx.send(Ok(evt));
                    ctx.lock()
                        .map_err(|e| AgentError::Other(e.to_string()))?
                        .conversation
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
                    // Propagate model errors (e.g. context_length_exceeded) as AgentError
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

        // if the stream ends unexpectedly, treat it as error?
        Ok(tool_calls)
    }
}
