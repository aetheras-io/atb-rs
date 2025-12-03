use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

//#NOTE newest and cheapest
pub const TEXT_EMBEDDING_3_SMALL: &str = "text-embedding-3-small";
pub const GPT4_O: &str = "gpt-4o";
pub const GPT4_O_MINI: &str = "gpt-4o-mini";
//#NOTE GPT 5 stuff, reasoning models
pub const GPT5: &str = "gpt-5";
pub const GPT5_MINI: &str = "gpt-5-mini";
pub const API_URL_V1: &str = "https://api.openai.com/v1";

#[derive(Serialize)]
pub struct BatchRequestEntry {
    pub method: String,
    pub url: String,
    pub custom_id: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub delta: Delta,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
}

#[derive(Clone)]
pub struct CustomClient {
    api_endpoint: String,
    api_key: String,
    organization: Option<String>,
    timeout: Option<u64>,
}

impl CustomClient {
    pub fn new<I: Into<String>>(key: I) -> Self {
        Self {
            api_endpoint: API_URL_V1.to_owned(),
            api_key: key.into(),
            organization: None,
            timeout: None,
        }
    }

    pub fn build_request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.api_endpoint, path);
        let client = Client::builder();

        let client = if let Some(timeout) = self.timeout {
            client.timeout(std::time::Duration::from_secs(timeout))
        } else {
            client
        };

        let client = client.build().unwrap();

        let mut request = client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "assistants=v2");

        if let Some(organization) = &self.organization {
            request = request.header("openai-organization", organization);
        }

        // if let Some(headers) = &self.headers {
        //     for (key, value) in headers {
        //         request = request.header(key, value);
        //     }
        // }

        request
    }

    // async fn post<T: serde::de::DeserializeOwned>(
    //     &self,
    //     path: &str,
    //     body: &impl serde::ser::Serialize,
    // ) -> Result<T, APIError> {
    //     let request = self.build_request(Method::POST, path).await;
    //     let request = request.json(body);
    //     let response = request.send().await?;
    //     self.handle_response(response).await
    // }
}

pub mod responses {
    use derive_builder::Builder;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::HashMap;

    // Type alias for Metadata for clarity
    pub type Metadata = HashMap<String, String>;

    // --- Base Create Params ---

    /// Base parameters for creating a response.
    #[derive(Clone, Debug, Serialize, Deserialize, Builder, PartialEq)]
    #[builder(setter(strip_option), default)] // Apply to all Option fields
    pub struct RequestPayload {
        /// Text, image, or file inputs to the model.
        pub input: InputType,

        /// Model ID used to generate the response.
        #[builder(setter(into))]
        pub model: String, // Assuming Shared.ResponsesModel is effectively a string or enum convertible to string

        /// Specify additional output data to include.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub include: Option<Vec<Includable>>,

        /// Inserts a system (or developer) message.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[builder(setter(into))]
        pub instructions: Option<String>,

        /// An upper bound for the number of tokens that can be generated.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub max_output_tokens: Option<u32>,

        /// Set of 16 key-value pairs that can be attached to an object.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub metadata: Option<Metadata>,

        /// Whether to allow the model to run tool calls in parallel.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub parallel_tool_calls: Option<bool>,

        /// The unique ID of the previous response to the model.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[builder(setter(into))]
        pub previous_response_id: Option<String>,

        /// Configuration options for reasoning models.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub reasoning: Option<ReasoningConfig>,

        /// Specifies the latency tier to use.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub service_tier: Option<ServiceTier>,

        /// Whether to store the generated model response.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub store: Option<bool>,

        /// If set to true, stream the response. (Included in base as per TS)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stream: Option<bool>,

        /// Sampling temperature.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub temperature: Option<f64>,

        /// Configuration options for a text response.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub text: Option<TextConfig>,

        /// How the model should select which tool to use.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tool_choice: Option<ToolChoice>,

        /// An array of tools the model may call.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tools: Option<Vec<Tool>>,

        /// Nucleus sampling parameter.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub top_p: Option<f64>,

        /// The truncation strategy to use.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub truncation: Option<TruncationStrategy>,

        /// A unique identifier representing your end-user.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[builder(setter(into))]
        pub user: Option<String>,
    }

    // Default implementation needed for Builder
    impl Default for RequestPayload {
        fn default() -> Self {
            // Note: Provide sensible defaults ONLY if the builder defaults aren't enough
            // For required fields like `input` and `model`, the builder enforces setting them.
            // This default is mainly for the derive_builder macro requirements.
            Self {
                input: InputType::Items(vec![]), // Default to empty items, builder will require it
                model: String::new(),            // Default to empty string, builder will require it
                include: None,
                instructions: None,
                max_output_tokens: None,
                metadata: None,
                parallel_tool_calls: None,
                previous_response_id: None,
                reasoning: None,
                service_tier: None,
                store: None,
                stream: None, // Default is None (usually implies false)
                temperature: None,
                text: None,
                tool_choice: None,
                tools: None,
                top_p: None,
                truncation: None,
                user: None,
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ReasoningConfig {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub effort: Option<ReasoningEffortConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub summary: Option<ReasoningSummaryConfig>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ReasoningEffortConfig {
        Minimal,
        Low,
        Medium,
        High,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ReasoningSummaryConfig {
        Auto,
        Concise,
        Detailed,
    }

    #[derive(Deserialize, Clone, Debug)]
    pub struct Error {
        pub code: String,
        pub message: String,
    }

    #[derive(Deserialize, Clone, Debug)]
    pub struct Response {
        pub id: String,
        pub object: String, // "response"
        pub created_at: i64,
        pub status: String, // "in_progress", "completed", etc.
        #[serde(default)]
        pub error: Option<Error>,
        #[serde(default)]
        pub incomplete_details: Option<Value>,
        #[serde(default)]
        pub instructions: Option<String>,
        #[serde(default)]
        pub max_output_tokens: Option<u32>,
        pub model: String,
        pub output: Vec<OutputItem>,
        #[serde(default)]
        pub parallel_tool_calls: Option<bool>,
        #[serde(default)]
        pub previous_response_id: Option<String>,
        #[serde(default)]
        pub reasoning: Option<ReasoningConfig>, // Use Reasoning from parent
        #[serde(default)]
        pub service_tier: Option<String>, // Present in initial event, might differ in final? Keep optional.
        #[serde(default)]
        pub store: Option<bool>,
        #[serde(default)]
        pub temperature: Option<f64>, // Use f64 to match RequestPayload
        #[serde(default)]
        pub text: Option<TextConfig>, // Use TextConfig from parent
        #[serde(default)]
        pub tool_choice: Option<ToolChoice>, // Use ToolChoice from parent
        #[serde(default)]
        pub tools: Option<Vec<Tool>>, // Use Tool from parent
        #[serde(default)]
        pub top_p: Option<f64>, // Use f64 to match RequestPayload
        #[serde(default)]
        pub truncation: Option<TruncationStrategy>, // Use enum from parent
        #[serde(default)]
        pub usage: Option<Usage>, // Use Usage from parent (only in completed)
        #[serde(default)]
        pub user: Option<String>,
        #[serde(default)]
        pub metadata: Option<Metadata>, // Use Metadata from parent
        #[serde(default)]
        pub input: Option<Vec<InputItem>>,
    }

    // --- Enums for String Unions ---

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum Includable {
        #[serde(rename = "file_search_call.results")]
        FileSearchCallResults,
        #[serde(rename = "message.input_image.image_url")]
        MessageInputImageImageUrl,
        #[serde(rename = "computer_call_output.output.image_url")]
        ComputerCallOutputOutputImageUrl,
        #[serde(rename = "reasoning.encrypted_content")]
        ReasoningEncryptedContent,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ServiceTier {
        Auto,
        Default,
        Flex,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ToolChoiceOption {
        None,
        Auto,
        Required,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum BuiltInToolType {
        FileSearch,
        #[serde(rename = "web_search_preview")]
        WebSearchPreview,
        #[serde(rename = "computer_use_preview")]
        ComputerUsePreview,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum TruncationStrategy {
        Auto,
        Disabled,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum EasyMessageRole {
        User,
        Assistant,
        System,
        Developer,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum MessageRole {
        User,
        System,
        Developer,
        Assistant,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ItemStatus {
        InProgress,
        Completed,
        Incomplete,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum FileSearchStatus {
        InProgress,
        Searching,
        Completed,
        Incomplete,
        Failed,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ComputerActionType {
        Click,
        DoubleClick,
        Drag,
        Keypress,
        Move,
        Screenshot,
        Scroll,
        Type,
        Wait,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum MouseButton {
        Left,
        Right,
        Wheel,
        Back,
        Forward,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum WebSearchStatus {
        InProgress,
        Searching,
        Completed,
        Failed,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ReasoningSummaryType {
        SummaryText,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum InputContentType {
        InputText,
        InputImage,
        InputFile,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ImageDetail {
        Low,
        High,
        Auto,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum ComputerEnvironment {
        Windows,
        Mac,
        Linux,
        Ubuntu,
        Browser,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum FileSearchRanker {
        Auto,
        #[serde(rename = "default-2024-11-15")]
        Default20241115,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum WebSearchToolType {
        #[serde(rename = "web_search_preview")]
        WebSearchPreview,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    pub enum SearchContextSize {
        Low,
        Medium,
        High,
    }

    #[derive(Deserialize, Clone, Debug)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum OutputItem {
        FunctionCall(FunctionToolCall),
        Message(OutputMessage),
        FileSearchCall(FileSearchToolCall),
        Reasoning(ReasoningItem),
    }

    // --- Input Types ---

    /// An image input to the model. Learn about
    /// [image inputs](https://platform.openai.com/docs/guides/vision).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputImage {
        /// The detail level of the image to be sent to the model. One of `high`, `low`, or
        /// `auto`. Defaults to `auto`.
        pub detail: ImageDetail,

        /// The ID of the file to be sent to the model.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_id: Option<String>,

        /// The URL of the image to be sent to the model. A fully qualified URL or base64
        /// encoded image in a data URL.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub image_url: Option<String>,
    }

    /// A text input to the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputText {
        /// The text input to the model.
        pub text: String,
    }

    /// A file input to the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputFile {
        /// The content of the file to be sent to the model. Base64 encoded.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_data: Option<String>,

        /// The ID of the file to be sent to the model.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_id: Option<String>,

        /// The name of the file to be sent to the model.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub filename: Option<String>,
    }

    /// Multi-modal input contents.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum InputContent {
        InputText(InputText),
        InputImage(InputImage),
        InputFile(InputFile),
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(untagged)]
    pub enum InputMessageContent {
        Text(String),
        List(InputContent),
    }

    /// A message input to the model (detailed version).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputMessage {
        /// A list of one or many input items to the model, containing different content types.
        pub content: InputMessageContent,

        /// The role of the message input. One of `user`, `system`, or `developer`.
        pub role: MessageRole,

        /// The status of item. Populated when items are returned via API.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<ItemStatus>,
    }

    impl From<(MessageRole, String)> for InputMessage {
        fn from((role, message): (MessageRole, String)) -> Self {
            Self {
                content: InputMessageContent::Text(message),
                role,
                status: None,
            }
        }
    }

    /// A text output from the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct OutputText {
        /// The annotations of the text output.
        pub annotations: Vec<TextAnnotation>,

        /// The text output from the model.
        pub text: String,
    }

    /// A refusal from the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct OutpuRefusal {
        /// The refusal explanationfrom the model.
        pub refusal: String,
    }

    /// An annotation within the text output.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum TextAnnotation {
        /// A citation to a file.
        FileCitation(FileCitationAnnotation),
        /// A citation for a web resource used to generate a model response.
        UrlCitation(UrlCitationAnnotation),
        /// A path to a file.
        FilePath(FilePathAnnotation),
    }

    /// A citation to a file.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FileCitationAnnotation {
        /// The ID of the file.
        pub file_id: String,
        /// The index of the file in the list of files.
        pub index: u32,
    }

    /// A citation for a web resource used to generate a model response.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct UrlCitationAnnotation {
        /// The index of the last character of the URL citation in the message.
        pub end_index: u32,
        /// The index of the first character of the URL citation in the message.
        pub start_index: u32,
        /// The title of the web resource.
        pub title: String,
        /// The URL of the web resource.
        pub url: String,
    }

    /// A path to a file.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FilePathAnnotation {
        /// The ID of the file.
        pub file_id: String,
        /// The index of the file in the list of files.
        pub index: u32,
    }

    /// Content item in an output message.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum OutputContent {
        OutputText(OutputText),
        Refusal(OutpuRefusal),
        FunctionCall(FunctionToolCall),
    }

    /// An output message from the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct OutputMessage {
        /// The unique ID of the output message.
        pub id: String,
        /// The content of the output message.
        pub content: Vec<OutputContent>,
        /// The role of the output message. Always `assistant`.
        pub role: MessageRole, // Always Assistant
        /// The status of the message input. One of `in_progress`, `completed`, or `incomplete`.
        pub status: ItemStatus,
    }

    /// The results of a file search tool call. See the
    /// [file search guide](https://platform.openai.com/docs/guides/tools-file-search)
    /// for more information.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FileSearchToolCall {
        /// The unique ID of the file search tool call.
        pub id: String,
        /// The queries used to search for files.
        pub queries: Vec<String>,
        /// The status of the file search tool call.
        pub status: FileSearchStatus,
        /// The results of the file search tool call.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub results: Option<Vec<FileSearchResult>>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FileSearchResult {
        /// Set of 16 key-value pairs that can be attached to an object.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub attributes: Option<HashMap<String, Value>>, // Using Value for string | number | boolean
        /// The unique ID of the file.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_id: Option<String>,
        /// The name of the file.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub filename: Option<String>,
        /// The relevance score of the file - a value between 0 and 1.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub score: Option<f64>,
        /// The text that was retrieved from the file.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
    }

    /// A tool call to a computer use tool.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerToolCall {
        /// The unique ID of the computer call.
        pub id: String,
        /// A click action.
        pub action: ComputerAction,
        /// An identifier used when responding to the tool call with output.
        pub call_id: String,
        /// The pending safety checks for the computer call.
        pub pending_safety_checks: Vec<PendingSafetyCheck>,
        /// The status of the item.
        pub status: ItemStatus,
    }

    /// A pending safety check for the computer call.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct PendingSafetyCheck {
        /// The ID of the pending safety check.
        pub id: String,
        /// The type of the pending safety check.
        pub code: String,
        /// Details about the pending safety check.
        pub message: String,
    }

    /// A computer screenshot image used with the computer use tool.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerToolCallOutputScreenshot {
        /// The identifier of an uploaded file that contains the screenshot.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_id: Option<String>,
        /// The URL of the screenshot image.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub image_url: Option<String>,
    }

    /// Represents actions performed by the computer tool.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ComputerAction {
        Click(ComputerClickAction),
        DoubleClick(ComputerDoubleClickAction),
        Drag(ComputerDragAction),
        Keypress(ComputerKeypressAction),
        Move(ComputerMoveAction),
        Screenshot(ComputerScreenshotAction),
        Scroll(ComputerScrollAction),
        Type(ComputerTypeAction),
        Wait(ComputerWaitAction),
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerClickAction {
        /// Indicates which mouse button was pressed during the click.
        pub button: MouseButton,
        /// The x-coordinate where the click occurred.
        pub x: f64, // Using f64 for coordinates
        /// The y-coordinate where the click occurred.
        pub y: f64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerDoubleClickAction {
        /// The x-coordinate where the double click occurred.
        pub x: f64,
        /// The y-coordinate where the double click occurred.
        pub y: f64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerDragAction {
        /// An array of coordinates representing the path of the drag action.
        pub path: Vec<Coordinate>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct Coordinate {
        /// The x-coordinate.
        pub x: f64,
        /// The y-coordinate.
        pub y: f64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerKeypressAction {
        /// The combination of keys the model is requesting to be pressed.
        pub keys: Vec<String>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerMoveAction {
        /// The x-coordinate to move to.
        pub x: f64,
        /// The y-coordinate to move to.
        pub y: f64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerScreenshotAction {} // No fields for screenshot action itself

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerScrollAction {
        /// The horizontal scroll distance.
        pub scroll_x: f64,
        /// The vertical scroll distance.
        pub scroll_y: f64,
        /// The x-coordinate where the scroll occurred.
        pub x: f64,
        /// The y-coordinate where the scroll occurred.
        pub y: f64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerTypeAction {
        /// The text to type.
        pub text: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerWaitAction {} // No fields for wait action itself

    /// The output of a computer tool call (used as input).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputComputerCallOutput {
        /// The ID of the computer tool call that produced the output.
        pub call_id: String,
        /// A computer screenshot image used with the computer use tool.
        pub output: ComputerToolCallOutputScreenshot,
        /// The ID of the computer tool call output.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
        /// The safety checks reported by the API that have been acknowledged by the developer.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub acknowledged_safety_checks: Option<Vec<AcknowledgedSafetyCheck>>,
        /// The status of the message input.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<ItemStatus>,
    }

    /// Acknowledged safety check details.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct AcknowledgedSafetyCheck {
        /// The ID of the pending safety check.
        pub id: String,
        /// The type of the pending safety check.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub code: Option<String>,
        /// Details about the pending safety check.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
    }

    /// The results of a web search tool call.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FunctionWebSearch {
        /// The unique ID of the web search tool call.
        pub id: String,
        /// The status of the web search tool call.
        pub status: WebSearchStatus,
    }

    /// A tool call to run a function.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FunctionToolCall {
        /// A JSON string of the arguments to pass to the function.
        pub arguments: String,
        /// The unique ID of the function tool call generated by the model.
        pub call_id: String,
        /// The name of the function to run.
        pub name: String,
        /// The unique ID of the function tool call. (Optional in input, present in output)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
        /// The status of the item. (Optional in input, present in output)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<ItemStatus>,
    }

    /// The output of a function tool call (used as input).
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputFunctionCallOutput {
        /// The unique ID of the function tool call generated by the model.
        pub call_id: String,
        /// A JSON string of the output of the function tool call.
        pub output: String,
        /// The unique ID of the function tool call output. Populated when this item is returned via API.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
        /// The status of the item. Populated when items are returned via API.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<ItemStatus>,
    }

    /// A description of the chain of thought used by a reasoning model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ReasoningItem {
        /// The unique identifier of the reasoning content.
        pub id: String,
        /// Reasoning text contents.
        pub summary: Vec<ReasoningSummary>,
        /// The encrypted content of the reasoning item.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub encrypted_content: Option<String>,
        /// The status of the item.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status: Option<ItemStatus>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ReasoningSummary {
        /// A short summary of the reasoning used by the model.
        pub text: String,
    }

    /// An internal identifier for an item to reference.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct InputItemReference {
        /// The ID of the item to reference.
        pub id: String,
    }

    /// Represents an item in the `input` array for the API request.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum InputItem {
        #[serde(rename = "message")]
        Message(InputMessage),
        //#TODO if these other two variants really are needed, its better to just have another
        //indirection with another enum, otherwise deserialization will be brittle
        //not sure this is needed at the moment anyway, OutputMessage is designed to feed back an
        //ai's response as an input item for convenience but we will be managing our own
        //conversation history so an "easy input message" conversion is better
        // #[serde(rename = "message")]
        // OutputMessage(OutputMessage),
        FileSearchCall(FileSearchToolCall),
        ComputerCall(ComputerToolCall),
        ComputerCallOutput(InputComputerCallOutput),
        WebSearchCall(FunctionWebSearch),
        FunctionCall(FunctionToolCall),
        FunctionCallOutput(InputFunctionCallOutput),
        Reasoning(ReasoningItem),
        ItemReference(InputItemReference),
    }

    impl From<(MessageRole, String)> for InputItem {
        fn from(message: (MessageRole, String)) -> Self {
            InputItem::Message(message.into())
        }
    }

    /// Represents the `input` parameter, which can be a simple string or a list of items.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(untagged)]
    pub enum InputType {
        Text(String),
        Items(Vec<InputItem>),
    }

    impl From<String> for InputType {
        fn from(input: String) -> Self {
            Self::Text(input)
        }
    }

    impl From<Vec<InputMessage>> for InputType {
        fn from(input: Vec<InputMessage>) -> Self {
            Self::Items(input.into_iter().map(InputItem::Message).collect())
        }
    }

    impl From<Vec<InputItem>> for InputType {
        fn from(input: Vec<InputItem>) -> Self {
            Self::Items(input)
        }
    }

    // --- Tool Definitions ---
    /// Ranking options for search.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct FileSearchRankingOptions {
        /// The ranker to use for the file search.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub ranker: Option<FileSearchRanker>,
        /// The score threshold for the file search.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub score_threshold: Option<f64>,
    }

    /// A tool that searches for relevant content from uploaded files.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FileSearchTool {
        /// The IDs of the vector stores to search.
        pub vector_store_ids: Vec<String>,
        /// A filter to apply.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub filters: Option<Value>, // Representing ComparisonFilter | CompoundFilter | null
        /// The maximum number of results to return.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub max_num_results: Option<u32>,
        /// Ranking options for search.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub ranking_options: Option<FileSearchRankingOptions>,
    }

    /// Defines a function in your own code the model can choose to call.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct FunctionTool {
        /// The name of the function to call.
        pub name: String,
        /// A JSON schema object describing the parameters of the function.
        pub parameters: Option<Value>,
        /// Whether to enforce strict parameter validation. Default `true`.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub strict: Option<bool>,
        /// A description of the function.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
    }

    /// The user's location for web search.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct WebSearchUserLocation {
        /// Free text input for the city of the user.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub city: Option<String>,
        /// The two-letter ISO country code of the user.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub country: Option<String>,
        /// Free text input for the region of the user.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub region: Option<String>,
        /// The IANA timezone of the user.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub timezone: Option<String>,
    }

    /// This tool searches the web for relevant results to use in a response.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct WebSearchTool {
        /// High level guidance for context window space usage.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub search_context_size: Option<SearchContextSize>,
        /// The user's location.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub user_location: Option<WebSearchUserLocation>,
    }

    /// A tool that controls a virtual computer.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ComputerTool {
        /// The height of the computer display.
        pub display_height: u32,
        /// The width of the computer display.
        pub display_width: u32,
        /// The type of computer environment to control.
        pub environment: ComputerEnvironment,
    }

    /// A tool that can be used to generate a response.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum Tool {
        FileSearch(FileSearchTool),
        Function(FunctionTool),
        #[serde(rename = "web_search_preview")] // Need rename here if type dictates variant
        WebSearchPreview(WebSearchTool),
        #[serde(rename = "computer_use_preview")] // Need rename here if type dictates variant
        ComputerPreview(ComputerTool), // Renamed variant for Rust convention
    }

    impl From<FunctionTool> for Tool {
        fn from(f: FunctionTool) -> Self {
            Self::Function(f)
        }
    }

    // --- Tool Choice Types ---

    /// Use this option to force the model to call a specific function.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ToolChoiceFunction {
        /// Tool choice function
        #[serde(rename = "type")]
        kind: ToolChoiceFunctionType,

        /// The name of the function to call.
        pub name: String,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub enum ToolChoiceFunctionType {
        #[serde(rename = "function")] // Ensures it serializes/deserializes as "function"
        Function,
    }

    /// Indicates that the model should use a built-in tool.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct ToolChoiceType {
        /// The type of hosted tool the model should use.
        #[serde(rename = "type")]
        pub kind: BuiltInToolType,
    }

    /// Controls which (if any) tool is called by the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(untagged)] // Using untagged because structure differs significantly
    pub enum ToolChoice {
        Option(ToolChoiceOption),     // e.g., "none", "auto", "required"
        Type(ToolChoiceType),         // e.g., { "type": "file_search" }
        Function(ToolChoiceFunction), // e.g., { "type": "function", "name": "my_func" }
    }

    // --- Text Configuration ---

    /// JSON Schema response format.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct JsonSchemaFormat {
        /// The name of the response format.
        pub name: String,
        /// The schema for the response format.
        pub schema: Value,
        /// A description of what the response format is for.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        /// Whether to enable strict schema adherence.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub strict: Option<bool>,
    }

    /// An object specifying the format that the model must output.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum FormatTextConfigUnion {
        Text,                         // Matches { "type": "text" }
        JsonObject,                   // Matches { "type": "json_object" }
        JsonSchema(JsonSchemaFormat), // Matches { "type": "json_schema", ... }
    }

    /// Configuration options for a text response from the model.
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct TextConfig {
        /// An object specifying the format that the model must output.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub format: Option<FormatTextConfigUnion>,
    }

    /// Placeholder for Shared.Reasoning if definition is unknown
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct _Reasoning {}

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct Usage {
        /// The number of input tokens.
        pub input_tokens: u32,

        /// A detailed breakdown of the input tokens.
        pub input_tokens_details: InputTokensDetails,

        /// The number of output tokens.
        pub output_tokens: u32,

        /// A detailed breakdown of the output tokens.
        pub output_tokens_details: OutputTokensDetails,

        /// The total number of tokens used.
        pub total_tokens: u32,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct InputTokensDetails {
        /// The number of tokens that were retrieved from the cache.
        pub cached_tokens: u32,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
    pub struct OutputTokensDetails {
        /// The number of reasoning tokens.
        pub reasoning_tokens: u32,
    }

    pub mod streaming {
        use super::*;
        use serde::Deserialize;

        // --- Event Data Payloads ---

        #[derive(Deserialize, Clone, Debug)]
        pub struct Created {
            pub response: Response,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct InProgress {
            pub response: Response,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct OutputItemAdded {
            pub output_index: usize,
            pub item: OutputItem,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct ContentPartAdded {
            pub item_id: String, // The ID of the OutputMessage this belongs to (e.g., "msg_...")
            pub output_index: usize,
            pub content_index: usize,
            pub part: OutputContent,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct OutputTextDelta {
            pub item_id: String,
            pub output_index: usize,
            pub content_index: usize,
            pub delta: String, // The chunk of text
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct FunctionCallArgumentsDelta {
            pub item_id: String,
            pub output_index: usize,
            /// JSON string delta of function arguments
            pub delta: String,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct FunctionCallArgumentsDone {
            pub item_id: String,
            pub output_index: usize,
            /// full function arguments as JSON string
            pub arguments: String,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct FileSearchCallOutput {
            pub output_index: usize,
            pub item_id: String,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct OutputTextDone {
            pub item_id: String,
            pub output_index: usize,
            pub content_index: usize,
            pub text: String, // The complete text for this output_text part
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct ContentPartDone {
            pub item_id: String,
            pub output_index: usize,
            pub content_index: usize,
            // The final state of the content part (e.g., OutputText with full text, FunctionCall with all details)
            pub part: OutputContent,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct OutputItemDone {
            pub output_index: usize,
            // The final state of the OutputMessage item
            pub item: OutputItem,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct Completed {
            // The final, complete Response object
            pub response: Response,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct OutputTextAnnotationAdded {
            pub item_id: String,
            pub output_index: usize,
            pub content_index: usize,
            pub annotation_index: usize,
            pub annotation: FileCitation,
        }

        #[derive(Deserialize, Clone, Debug)]
        pub struct FileCitation {
            #[serde(rename = "type")]
            pub kind: String, // typically "file_citation"
            pub index: usize,
            pub file_id: String,
            pub filename: String,
        }

        // --- Top-Level Stream Event Enum ---

        #[derive(Deserialize, Clone, Debug)]
        pub struct ErrorEvent {
            #[serde(default)]
            pub sequence_number: Option<u64>,
            pub error: super::Error,
        }

        /// Represents any event that can be received from the Response streaming API.
        #[derive(Deserialize, Clone, Debug)]
        #[serde(tag = "type")]
        pub enum ResponseStreamEvent {
            #[serde(rename = "response.created")]
            Created(Created),
            #[serde(rename = "response.in_progress")]
            InProgress(InProgress),
            #[serde(rename = "response.output_item.added")]
            OutputItemAdded(OutputItemAdded),
            #[serde(rename = "response.content_part.added")]
            ContentPartAdded(ContentPartAdded),
            #[serde(rename = "response.output_text.delta")]
            OutputTextDelta(OutputTextDelta),
            #[serde(rename = "response.output_text.annotation.added")]
            OutputTextAnnotationAdded(OutputTextAnnotationAdded),
            #[serde(rename = "response.function_call_arguments.delta")]
            FunctionCallArgumentsDelta(FunctionCallArgumentsDelta),
            #[serde(rename = "response.function_call_arguments.done")]
            FunctionCallArgumentsDone(FunctionCallArgumentsDone),
            #[serde(rename = "response.file_search_call.in_progress")]
            FileSearchCallInProgress(FileSearchCallOutput),
            #[serde(rename = "response.file_search_call.searching")]
            FileSearchCallSearching(FileSearchCallOutput),
            #[serde(rename = "response.file_search_call.completed")]
            FileSearchCallCompleted(FileSearchCallOutput),
            #[serde(rename = "response.output_text.done")]
            OutputTextDone(OutputTextDone),
            #[serde(rename = "response.content_part.done")]
            ContentPartDone(ContentPartDone),
            #[serde(rename = "response.output_item.done")]
            OutputItemDone(OutputItemDone),
            #[serde(rename = "response.completed")]
            Completed(Completed),
            #[serde(rename = "response.failed")]
            Failed(Completed),
            #[serde(rename = "error")]
            Error(ErrorEvent),
        }
    }
}

pub mod embeddings {
    use serde::{Deserialize, Serialize};

    /// Request body for OpenAI embeddings API
    #[derive(Debug, Serialize)]
    pub struct EmbeddingsRequest {
        /// Input text to embed, as a string or array of strings/tokens
        pub input: EmbeddingsInput,
        /// ID of the model to use
        pub model: String,
        /// Optional: number of dimensions for output embeddings (text-embedding-3+)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub dimensions: Option<u32>,
        /// Optional: encoding format ("float" or "base64")
        #[serde(skip_serializing_if = "Option::is_none")]
        pub encoding_format: Option<String>,
        /// Optional: unique identifier for end-user
        #[serde(skip_serializing_if = "Option::is_none")]
        pub user: Option<String>,
    }

    /// The "input" field can be a string, array of strings, or array of arrays of tokens
    #[derive(Debug, Serialize)]
    #[serde(untagged)]
    pub enum EmbeddingsInput {
        String(String),
        Strings(Vec<String>),
        Tokens(Vec<u32>),
        TokensList(Vec<Vec<u32>>),
    }

    /// Response from OpenAI embeddings API
    #[derive(Debug, Deserialize)]
    pub struct EmbeddingsResponse {
        pub object: String, // "list"
        pub data: Vec<EmbeddingObject>,
        pub model: String,
        pub usage: EmbeddingsUsage,
    }

    /// An embedding object in the response
    #[derive(Debug, Deserialize)]
    pub struct EmbeddingObject {
        pub object: String, // "embedding"
        pub embedding: Vec<f32>,
        pub index: usize,
    }

    /// Usage statistics for the embeddings request
    #[derive(Debug, Deserialize)]
    pub struct EmbeddingsUsage {
        pub prompt_tokens: u32,
        pub total_tokens: u32,
    }
}
