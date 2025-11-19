//! # LLM Integration Layer
//!
//! This module provides a unified, modular interface for integrating multiple LLM providers
//! with VTCode, supporting Gemini, OpenAI, Anthropic, xAI, and DeepSeek.
//!
//! ## Architecture Overview
//!
//! The LLM layer is designed with several key principles:
//!
//! - **Unified Interface**: Single `AnyClient` trait for all providers
//! - **Provider Agnostic**: Easy switching between providers
//! - **Configuration Driven**: TOML-based provider configuration
//! - **Error Handling**: Comprehensive error types and recovery
//! - **Async Support**: Full async/await support for all operations
//!
//! ## Supported Providers
//!
//! | Provider | Status | Models |
//! |----------|--------|---------|
//! | Gemini | ✓ | gemini-2.5-pro, gemini-2.5-flash-preview-05-20 |
//! | OpenAI | ✓ | gpt-5, gpt-5-mini, gpt-5-nano |
//! | Anthropic | ✓ | claude-4.1-opus, claude-4-sonnet |
//! | xAI | ✓ | grok-2-latest, grok-2-mini |
//! | DeepSeek | ✓ | deepseek-chat, deepseek-reasoner |
//! | Z.AI | ✓ | glm-4.6 |
//! | Ollama | ✓ | gpt-oss:20b (local) |
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use std::str::FromStr;
//! use vtcode_core::config::models::ModelId;
//! use vtcode_core::llm::cache::LLMCacheConfig;
//! use vtcode_core::llm::provider::LLMRequest;
//! use vtcode_core::llm::types::{Message, ToolChoice};
//! use vtcode_core::llm::make_client;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let model = ModelId::from_str("gemini-2.5-flash")?;
//!     let mut client = make_client(
//!         std::env::var("GEMINI_API_KEY")?,
//!         model.clone(),
//!         LLMCacheConfig::default(),
//!     );
//!
//!     let request = LLMRequest {
//!         messages: vec![Message::user("Hello, how can you help me with coding?".to_string())],
//!         system_prompt: None,
//!         tools: None,
//!         model: model.as_str().to_string(),
//!         max_tokens: None,
//!         temperature: None,
//!         stream: false,
//!         tool_choice: Some(ToolChoice::none()),
//!         parallel_tool_calls: None,
//!         parallel_tool_config: None,
//!         reasoning_effort: None,
//!         output_format: None,
//!         verbosity: None,
//!     };
//!
//!     let response = client.generate_request(&request).await?;
//!     println!("Response: {}", response.content);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Provider Configuration
//!
//! ```rust,no_run
//! use vtcode_core::utils::dot_config::{ProviderConfigs, ProviderConfig};
//!
//! let config = ProviderConfigs {
//!     gemini: Some(ProviderConfig {
//!         api_key: "your-api-key".to_string(),
//!         model: "gemini-2.5-flash".to_string(),
//!         temperature: Some(0.7),
//!         max_tokens: Some(4096),
//!         ..Default::default()
//!     }),
//!     openai: Some(ProviderConfig {
//!         api_key: "your-openai-key".to_string(),
//!         model: "gpt-5".to_string(),
//!         temperature: Some(0.3),
//!         max_tokens: Some(8192),
//!         ..Default::default()
//!     }),
//!     ..Default::default()
//! };
//! ```
//!
//! ## Advanced Features
//!
//! ### Streaming Responses
//! ```text
//! use futures::StreamExt;
//! let mut stream = provider.stream(request).await?;
//! while let Some(chunk) = stream.next().await {
//!     // handle chunk
//! }
//! ```
//!
//! ### Function Calling
//! ```text
//! use vtcode_core::llm::types::FunctionDefinition;
//! let tools = vec![FunctionDefinition::new(... )];
//! let request = LLMRequest { tools: Some(tools.into_iter().map(ToolDefinition::from).collect()), .. };
//! ```
//!
//! ## Error Handling
//!
//! The LLM layer provides comprehensive error handling:
//!
//! ```rust,no_run
//! use vtcode_core::llm::LLMError;
//!
//! match client.chat(&messages, None).await {
//!     Ok(response) => println!("Success: {}", response.content),
//!     Err(LLMError::Authentication) => eprintln!("Authentication failed"),
//!     Err(LLMError::RateLimit) => eprintln!("Rate limit exceeded"),
//!     Err(LLMError::Network(e)) => eprintln!("Network error: {}", e),
//!     Err(LLMError::Provider(e)) => eprintln!("Provider error: {}", e),
//!     Err(e) => eprintln!("Other error: {}", e),
//! }
//! ```
//!
//! ## Performance Considerations
//!
//! - **Connection Pooling**: Efficient connection reuse
//! - **Request Batching**: Where supported by providers
//! - **Caching**: Built-in prompt caching for repeated requests
//! - **Timeout Handling**: Configurable timeouts and retries
//! - **Rate Limiting**: Automatic rate limit handling
//!
//! # LLM abstraction layer with modular architecture
//!
//! This module provides a unified interface for different LLM providers
//! with provider-specific implementations.

pub mod capabilities;
pub mod client;
pub mod error_display;
pub mod factory;
pub mod provider;
pub mod providers;
pub mod rig_adapter;

pub mod cache;
pub mod codec;
pub mod token_metrics;
pub mod types;

#[cfg(test)]
mod error_display_test;

// Re-export main types for backward compatibility
pub use capabilities::ProviderCapabilities;
pub use client::{AnyClient, make_client};
pub use factory::{create_provider_with_config, get_factory};
pub use provider::{LLMStream, LLMStreamEvent};
pub use providers::{
    AnthropicProvider, GeminiProvider, OllamaProvider, OpenAIProvider, XAIProvider, ZAIProvider,
};

pub use token_metrics::{TokenCounter, TokenMetrics, TokenTypeMetrics};
pub use types::{BackendKind, LLMError, LLMResponse};
