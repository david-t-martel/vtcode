//! Universal LLM provider abstraction with API-specific role handling
//!
//! This module provides a unified interface for different LLM providers (OpenAI, Anthropic, Gemini)
//! while properly handling their specific requirements for message roles and tool calling.

use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;

use crate::config::types::{ReasoningEffortLevel, VerbosityLevel};
// Re-export core LLM types so downstream modules can import from `llm::provider`.
pub use crate::llm::types::{
    FinishReason, Function, FunctionCall, FunctionDefinition, LLMError, LLMRequest, LLMResponse,
    LLMStreamEvent, Message, MessageContent, MessageRole, ParallelToolConfig, Tool, ToolCall,
    ToolChoice, ToolDefinition, Usage,
};

pub type LLMStream = Pin<Box<dyn Stream<Item = Result<LLMStreamEvent, LLMError>> + Send>>;

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
