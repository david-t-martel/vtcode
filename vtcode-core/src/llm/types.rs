use crate::config::types::{ReasoningEffortLevel, VerbosityLevel};
use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;

/// Backend kind for LLM providers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendKind {
    Gemini,
    OpenAI,
    Anthropic,
    DeepSeek,
    OpenRouter,
    Ollama,
    XAI,
    ZAI,
    Moonshot,
}

/// Universal LLM request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMRequest {
    pub messages: Vec<Message>,
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<Tool>>,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    pub tool_choice: Option<ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub parallel_tool_config: Option<ParallelToolConfig>,
    pub reasoning_effort: Option<ReasoningEffortLevel>,
    pub output_format: Option<String>,
    pub verbosity: Option<VerbosityLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_details: Option<Vec<Value>>,
    pub origin_tool: Option<String>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(content),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn user_with_parts(parts: Vec<ContentPart>) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Parts(parts),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn system(content: String) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::Text(content),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn assistant_with_tools(content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn assistant_with_parts(parts: Vec<ContentPart>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Parts(parts),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn tool_response(tool_call_id: String, content: String) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::Text(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            reasoning: None,
            reasoning_details: None,
            origin_tool: None,
        }
    }

    pub fn tool_response_with_origin(
        tool_call_id: String,
        content: String,
        origin_tool: String,
    ) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::Text(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            reasoning: None,
            reasoning_details: None,
            origin_tool: Some(origin_tool),
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            MessageContent::Text(text) => Some(text),
            MessageContent::Parts(_) => None,
        }
    }

    pub fn as_tool_calls(&self) -> Option<&Vec<ToolCall>> {
        self.tool_calls.as_ref()
    }

    pub fn is_tool_response(&self) -> bool {
        matches!(self.role, MessageRole::Tool)
    }

    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls
            .as_ref()
            .map(|calls| !calls.is_empty())
            .unwrap_or(false)
    }

    pub fn with_reasoning(mut self, reasoning: Option<String>) -> Self {
        self.reasoning = reasoning;
        self
    }

    pub fn validate_for_provider(&self, _provider_key: &str) -> Result<(), String> {
        // Basic validation, can be expanded per provider needs
        if self.content.is_empty() && self.tool_calls.is_none() {
            return Err("Message content and tool calls cannot both be empty".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn as_openai_str(&self) -> &str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }

    pub fn as_generic_str(&self) -> &str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }

    pub fn as_gemini_str(&self) -> &str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "model",
            _ => "user", // Gemini only supports user and model roles in history
        }
    }

    pub fn as_anthropic_str(&self) -> &str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            _ => "user",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn text(text: String) -> Self {
        MessageContent::Text(text)
    }

    pub fn parts(parts: Vec<ContentPart>) -> Self {
        MessageContent::Parts(parts)
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(text) => Some(text),
            MessageContent::Parts(_) => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            MessageContent::Text(text) => text.is_empty(),
            MessageContent::Parts(parts) => parts.is_empty(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentPart {
    Text { text: String },
    Image { url: String },
}

impl ContentPart {
    pub fn text(text: String) -> Self {
        ContentPart::Text { text }
    }

    /// Backwards-compatible helper for constructing image parts from raw data and MIME type.
    ///
    /// The data and MIME type are combined into a data URL of the form
    /// `data:<mime_type>;base64,<data>` and stored in the `url` field.
    pub fn image(data: String, mime_type: String) -> Self {
        let url = format!("data:{};base64,{}", mime_type, data);
        ContentPart::Image { url }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tool {
    pub function: Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl Function {
    pub fn as_ref(&self) -> &Self {
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolChoice {
    Auto,
    Any,
    Tool(String),
    None,
}

impl ToolChoice {
    pub fn auto() -> Self {
        ToolChoice::Auto
    }

    pub fn any() -> Self {
        ToolChoice::Any
    }

    pub fn tool(name: impl Into<String>) -> Self {
        ToolChoice::Tool(name.into())
    }

    // Backwards-compatible alias used in some providers/tests
    pub fn function(name: String) -> Self {
        ToolChoice::Tool(name)
    }

    pub fn none() -> Self {
        ToolChoice::None
    }

    pub fn to_provider_format(&self, provider_key: &str) -> Value {
        match provider_key {
            "openai" | "anthropic" | "deepseek" | "openrouter" | "moonshot" | "ollama" | "xai"
            | "zai" => match self {
                ToolChoice::Auto => Value::String("auto".to_string()),
                ToolChoice::Any => Value::String("auto".to_string()), // OpenAI doesn't have 'any', 'auto' is closest
                ToolChoice::Tool(name) => {
                    serde_json::json!({ "type": "function", "function": { "name": name } })
                }
                ToolChoice::None => Value::String("none".to_string()),
            },
            "gemini" => match self {
                ToolChoice::Auto => {
                    serde_json::json!({ "function_calling_config": { "mode": "auto" } })
                }
                ToolChoice::Any => {
                    serde_json::json!({ "function_calling_config": { "mode": "auto" } })
                }
                ToolChoice::Tool(name) => {
                    serde_json::json!({ "function_calling_config": { "mode": "any", "allowed_function_names": [name] } })
                }
                ToolChoice::None => {
                    serde_json::json!({ "function_calling_config": { "mode": "none" } })
                }
            },
            _ => Value::String("auto".to_string()), // Default to auto for unknown providers
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParallelToolConfig {
    pub disable_parallel_tool_use: bool,
    pub max_parallel_tools: Option<u32>,
    pub encourage_parallel: bool,
}

/// Unified LLM response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: FinishReason,
    pub reasoning_details: Option<Vec<Value>>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub cached_prompt_tokens: Option<usize>,
    pub cache_creation_tokens: Option<usize>,
    pub cache_read_tokens: Option<usize>,
}

/// LLM error types
#[derive(Debug, thiserror::Error)]
pub enum LLMError {
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Authentication error: {0}")]
    Authentication(String),
    #[error("Provider error: {0}")]
    Provider(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
    pub call_type: String,
    pub text: Option<String>,
}

impl ToolCall {
    pub fn function(id: String, name: String, arguments: String) -> Self {
        Self {
            id,
            function: FunctionCall { name, arguments },
            call_type: "function".to_string(),
            text: None,
        }
    }

    pub fn parsed_arguments(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str(&self.function.arguments)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl ToolDefinition {
    pub fn function(name: String, description: String, parameters: Value) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: Some(FunctionDefinition {
                name,
                description,
                parameters,
            }),
            shell: None,
            grammar: None,
            strict: None,
        }
    }

    pub fn apply_patch(description: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Structured patch content (e.g. unified diff) to apply to the workspace.",
                }
            },
            "required": ["input"],
            "additionalProperties": false
        });
        Self::function("apply_patch".to_string(), description, parameters)
    }

    pub fn shell(config: Option<Value>) -> Self {
        Self {
            tool_type: "shell".to_string(),
            function: None,
            shell: config,
            grammar: None,
            strict: None,
        }
    }

    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
    }

    /// Convenience accessor used by ACP tooling to derive a stable function identifier.
    pub fn function_name(&self) -> &str {
        self.function
            .as_ref()
            .map(|f| f.name.as_str())
            .unwrap_or("")
    }
}

impl From<&Tool> for ToolDefinition {
    fn from(tool: &Tool) -> Self {
        ToolDefinition::function(
            tool.function.name.clone(),
            tool.function.description.clone(),
            tool.function.parameters.clone(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    FunctionCall,
    Error(String),
}

pub type LLMStream = Pin<Box<dyn Stream<Item = Result<LLMStreamEvent, LLMError>> + Send>>;

#[derive(Debug, Clone)]
pub enum LLMStreamEvent {
    Token { delta: String },
    Reasoning { delta: String },
    Completed { response: LLMResponse },
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supports_streaming(&self) -> bool;
    fn supports_reasoning_effort(&self, model: &str) -> bool;
    fn supports_parallel_tool_config(&self, model: &str) -> bool;
    fn supports_tools(&self, model: &str) -> bool;
    fn supported_models(&self) -> Vec<String>;
    fn validate_request(&self, request: &LLMRequest) -> Result<(), LLMError>;
    fn supports_reasoning(&self, model: &str) -> bool;
    fn supports_structured_output(&self, model: &str) -> bool;

    async fn generate(&self, request: LLMRequest) -> Result<LLMResponse, LLMError>;
    async fn stream(&self, request: LLMRequest) -> Result<LLMStream, LLMError>;
}
