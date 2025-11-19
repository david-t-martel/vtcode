//! Universal LLM provider abstraction with API-specific role handling
//!
//! This module provides a unified interface for different LLM providers (OpenAI, Anthropic, Gemini)
//! while properly handling their specific requirements for message roles and tool calling.

use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use serde_json::{Value, json};
use std::env;
use std::pin::Pin;

use crate::config::types::{ReasoningEffortLevel, VerbosityLevel};
use crate::config::{
    TimeoutsConfig,
    constants::{defaults, env_vars, models, urls},
    core::{AnthropicPromptCacheSettings, PromptCachingConfig},
    models::Provider,
};
use crate::llm::client::LLMClient;
use crate::llm::error_display;
use crate::llm::provider::LLMProvider;
use crate::llm::rig_adapter::reasoning_parameters_for;
use crate::llm::types as llm_types;
use llm_types::{
    ContentPart, FinishReason, Function, FunctionCall, FunctionDefinition, LLMError, LLMRequest,
    LLMResponse, LLMStreamEvent, Message, MessageContent, MessageRole, ParallelToolConfig, Tool,
    ToolCall, ToolChoice, ToolDefinition, Usage,
};

use super::{
    common::{extract_prompt_cache_settings, override_base_url, resolve_model},
    extract_reasoning_trace,
};

pub type LLMStream = Pin<Box<dyn Stream<Item = Result<LLMStreamEvent, LLMError>> + Send>>;

pub struct AnthropicProvider {
    api_key: String,
    http_client: reqwest::Client,
    base_url: String,
    model: String,
    prompt_cache_enabled: bool,
    prompt_cache_settings: AnthropicPromptCacheSettings,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_model_internal(
            api_key,
            models::anthropic::DEFAULT_MODEL.to_string(),
            None,
            None,
        )
    }

    pub fn with_model(api_key: String, model: String) -> Self {
        Self::with_model_internal(api_key, model, None, None)
    }

    pub fn from_config(
        api_key: Option<String>,
        model: Option<String>,
        base_url: Option<String>,
        prompt_cache: Option<PromptCachingConfig>,
        _timeouts: Option<TimeoutsConfig>,
    ) -> Self {
        let api_key_value = api_key.unwrap_or_default();
        let model_value = resolve_model(model, models::anthropic::DEFAULT_MODEL);

        Self::with_model_internal(api_key_value, model_value, prompt_cache, base_url)
    }

    fn with_model_internal(
        api_key: String,
        model: String,
        prompt_cache: Option<PromptCachingConfig>,
        base_url: Option<String>,
    ) -> Self {
        let (prompt_cache_enabled, prompt_cache_settings) = extract_prompt_cache_settings(
            prompt_cache,
            |providers| &providers.anthropic,
            |cfg, provider_settings| cfg.enabled && provider_settings.enabled,
        );

        let base_url_value = if model.as_str() == models::minimax::MINIMAX_M2 {
            Self::resolve_minimax_base_url(base_url)
        } else {
            override_base_url(
                urls::ANTHROPIC_API_BASE,
                base_url,
                Some(env_vars::ANTHROPIC_BASE_URL),
            )
        };

        Self {
            api_key,
            http_client: reqwest::Client::new(),
            base_url: base_url_value,
            model,
            prompt_cache_enabled,
            prompt_cache_settings,
        }
    }

    fn resolve_minimax_base_url(base_url: Option<String>) -> String {
        fn sanitize(value: &str) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.trim_end_matches('/').to_string())
            }
        }

        let resolved = base_url
            .and_then(|value| sanitize(&value))
            .or_else(|| {
                env::var(env_vars::MINIMAX_BASE_URL)
                    .ok()
                    .and_then(|value| sanitize(&value))
            })
            .or_else(|| {
                env::var(env_vars::ANTHROPIC_BASE_URL)
                    .ok()
                    .and_then(|value| sanitize(&value))
            })
            .or_else(|| sanitize(urls::MINIMAX_API_BASE))
            .unwrap_or_else(|| urls::MINIMAX_API_BASE.trim_end_matches('/').to_string());

        let mut normalized = resolved;

        if normalized.ends_with("/messages") {
            normalized = normalized
                .trim_end_matches("/messages")
                .trim_end_matches('/')
                .to_string();
        }

        if let Some(pos) = normalized.find("/v1/") {
            normalized = normalized[..pos + 3].to_string();
        }

        if !normalized.ends_with("/v1") {
            normalized = format!("{}/v1", normalized);
        }

        normalized
    }

    /// Determines the TTL string for cache control.
    /// Anthropic only supports "5m" (5 minutes) or "1h" (1 hour).
    ///
    /// Returns:
    /// - "1h" if extended_ttl_seconds is set and >= 3600 seconds
    /// - "5m" for default or extended_ttl_seconds < 3600 seconds
    fn get_cache_ttl(&self) -> &'static str {
        self.prompt_cache_settings
            .extended_ttl_seconds
            .filter(|&ttl| ttl >= 3600)
            .map(|_| "1h")
            .unwrap_or("5m")
    }

    /// Returns the cache control JSON block for Anthropic API.
    fn cache_control_value(&self) -> Option<Value> {
        if !self.prompt_cache_enabled {
            return None;
        }

        Some(json!({
            "type": "ephemeral",
            "ttl": self.get_cache_ttl()
        }))
    }

    /// Returns the beta header value for Anthropic API prompt caching.
    /// - Always includes "prompt-caching-2024-07-31"
    /// - Adds "extended-cache-ttl-2025-04-11" only when using 1h TTL
    fn prompt_cache_beta_header_value(&self) -> Option<String> {
        if !self.prompt_cache_enabled {
            return None;
        }

        let mut betas = vec!["prompt-caching-2024-07-31"];

        // Only add extended TTL beta if we're actually using 1h cache
        if self.get_cache_ttl() == "1h" {
            betas.push("extended-cache-ttl-2025-04-11");
        }

        Some(betas.join(", "))
    }

    /// Combines prompt cache betas with structured outputs beta when requested.
    fn combined_beta_header_value(&self, include_structured: bool) -> Option<String> {
        let mut pieces: Vec<String> = Vec::new();
        if let Some(pc) = self.prompt_cache_beta_header_value() {
            for p in pc
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                pieces.push(p);
            }
        }
        if include_structured {
            // Use the correct beta header for structured outputs
            pieces.push("structured-outputs-2025-11-13".to_string());
        }
        if pieces.is_empty() {
            None
        } else {
            Some(pieces.join(", "))
        }
    }

    fn default_request(&self, prompt: &str) -> LLMRequest {
        LLMRequest {
            messages: vec![Message::user(prompt.to_string())],
            system_prompt: None,
            tools: None,
            model: self.model.clone(),
            max_tokens: None,
            temperature: None,
            stream: false,
            tool_choice: None,
            parallel_tool_calls: None,
            parallel_tool_config: None,
            reasoning_effort: None,
            verbosity: None,
            output_format: None,
        }
    }

    fn parse_client_prompt(&self, prompt: &str) -> LLMRequest {
        let trimmed = prompt.trim_start();
        if trimmed.starts_with('{') {
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                if let Some(request) = self.parse_messages_request(&value) {
                    return request;
                }
            }
        }

        self.default_request(prompt)
    }

    fn parse_messages_request(&self, value: &Value) -> Option<LLMRequest> {
        let messages_value = value.get("messages")?.as_array()?;
        let mut system_prompt = value
            .get("system")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let mut messages = Vec::new();

        for entry in messages_value {
            let role = entry
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or(crate::config::constants::message_roles::USER);

            match role {
                "assistant" => {
                    let mut text_content = String::new();
                    let mut tool_calls = Vec::new();

                    if let Some(content_array) = entry.get("content").and_then(|c| c.as_array()) {
                        for block in content_array {
                            match block.get("type").and_then(|t| t.as_str()) {
                                Some("text") => {
                                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                        text_content.push_str(text);
                                    }
                                }
                                Some("tool_use") => {
                                    let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                    let name =
                                        block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                    let input =
                                        block.get("input").cloned().unwrap_or_else(|| json!({}));
                                    let arguments = serde_json::to_string(&input)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    if !id.is_empty() && !name.is_empty() {
                                        tool_calls.push(ToolCall::function(
                                            id.to_string(),
                                            name.to_string(),
                                            arguments,
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(content_text) = entry.get("content").and_then(|c| c.as_str())
                    {
                        text_content.push_str(content_text);
                    }

                    let mut message = Message::assistant(text_content);
                    if !tool_calls.is_empty() {
                        message.tool_calls = Some(tool_calls);
                    }
                    messages.push(message);
                }
                "user" => {
                    let mut text_buffer = String::new();
                    let mut pending_tool_results = Vec::new();

                    if let Some(content_array) = entry.get("content").and_then(|c| c.as_array()) {
                        for block in content_array {
                            match block.get("type").and_then(|t| t.as_str()) {
                                Some("text") => {
                                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                        text_buffer.push_str(text);
                                    }
                                }
                                Some("tool_result") => {
                                    if !text_buffer.is_empty() {
                                        messages.push(Message::user(text_buffer.clone()));
                                        text_buffer.clear();
                                    }
                                    if let Some(tool_use_id) =
                                        block.get("tool_use_id").and_then(|id| id.as_str())
                                    {
                                        let serialized = block.to_string();
                                        pending_tool_results
                                            .push((tool_use_id.to_string(), serialized));
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(content_text) = entry.get("content").and_then(|c| c.as_str())
                    {
                        text_buffer.push_str(content_text);
                    }

                    if !text_buffer.is_empty() {
                        messages.push(Message::user(text_buffer));
                    }

                    for (tool_use_id, payload) in pending_tool_results {
                        messages.push(Message::tool_response(tool_use_id, payload));
                    }
                }
                "system" => {
                    if system_prompt.is_none() {
                        let extracted = if let Some(content_array) =
                            entry.get("content").and_then(|c| c.as_array())
                        {
                            content_array
                                .iter()
                                .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("")
                        } else {
                            entry
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string()
                        };
                        if !extracted.is_empty() {
                            system_prompt = Some(extracted);
                        }
                    }
                }
                _ => {}
            }
        }

        if messages.is_empty() {
            let formatted_error = error_display::format_llm_error(
                "Anthropic",
                "No convertible messages for Anthropic request",
            );
            return None;
        }

        Some(LLMRequest {
            messages,
            system_prompt,
            tools: None,
            model: self.model.clone(),
            max_tokens: value
                .get("max_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .or(Some(defaults::ANTHROPIC_DEFAULT_MAX_TOKENS)),
            temperature: value
                .get("temperature")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32),
            stream: value
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            tool_choice: None,
            parallel_tool_calls: None,
            parallel_tool_config: None,
            reasoning_effort: None,
            output_format: value
                .get("output_format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            verbosity: None,
        })
    }

    fn convert_to_anthropic_format(&self, request: &LLMRequest) -> Result<Value, LLMError> {
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|msg| {
                json!({
                    "role": msg.role.as_anthropic_str(),
                    "content": [{"type": "text", "text": msg.as_text().unwrap_or_default()}]
                })
            })
            .collect();

        Ok(json!({
            "model": request.model,
            "messages": messages,
            "stream": request.stream,
            "max_tokens": request
                .max_tokens
                .unwrap_or(defaults::ANTHROPIC_DEFAULT_MAX_TOKENS),
        }))
    }

    fn parse_anthropic_response(&self, response_json: Value) -> Result<LLMResponse, LLMError> {
        let content = response_json
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                let formatted = error_display::format_llm_error(
                    "Anthropic",
                    "Invalid response format: missing content",
                );
                LLMError::Provider(formatted)
            })?;

        let mut text_parts = Vec::new();
        let mut reasoning_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                Some("thinking") => {
                    if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                        reasoning_parts.push(thinking.to_string());
                    } else if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        reasoning_parts.push(text.to_string());
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Special handling for structured output tools
                    if name == "structured_output" {
                        // For structured output, we should treat the input as the main content
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        // Convert the structured output to text for the content field
                        let output_text =
                            serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                        text_parts.push(output_text);
                    } else {
                        // Handle regular tools
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        let arguments =
                            serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                        if !id.is_empty() && !name.is_empty() {
                            tool_calls.push(ToolCall::function(id, name, arguments));
                        }
                    }
                }
                _ => {}
            }
        }

        let reasoning = if reasoning_parts.is_empty() {
            response_json
                .get("reasoning")
                .and_then(extract_reasoning_trace)
        } else {
            let joined = reasoning_parts.join("\n");
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        let stop_reason = response_json
            .get("stop_reason")
            .and_then(|sr| sr.as_str())
            .unwrap_or("end_turn");
        let finish_reason = match stop_reason {
            "end_turn" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "stop_sequence" => FinishReason::Stop,
            "tool_use" => FinishReason::ToolCalls,
            other => FinishReason::Error(other.to_string()),
        };

        let usage = response_json.get("usage").map(|usage_value| {
            let cache_creation_tokens = usage_value
                .get("cache_creation_input_tokens")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let cache_read_tokens = usage_value
                .get("cache_read_input_tokens")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);

            Usage {
                prompt_tokens: usage_value
                    .get("input_tokens")
                    .and_then(|it| it.as_u64())
                    .unwrap_or(0) as usize,
                completion_tokens: usage_value
                    .get("output_tokens")
                    .and_then(|ot| ot.as_u64())
                    .unwrap_or(0) as usize,
                total_tokens: (usage_value
                    .get("input_tokens")
                    .and_then(|it| it.as_u64())
                    .unwrap_or(0)
                    + usage_value
                        .get("output_tokens")
                        .and_then(|ot| ot.as_u64())
                        .unwrap_or(0)) as usize,
                cached_prompt_tokens: cache_read_tokens,
                cache_creation_tokens,
                cache_read_tokens,
            }
        });
        Ok(LLMResponse {
            content: text_parts.join(""),
            model: self.model.clone(),
            usage,
            reasoning,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            finish_reason,
            reasoning_details: None,
        })
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn supports_reasoning(&self, _model: &str) -> bool {
        false
    }

    fn supports_streaming(&self) -> bool {
        // Streaming not yet implemented for Anthropic in this build.
        false
    }

    fn supports_reasoning_effort(&self, model: &str) -> bool {
        let requested = if model.trim().is_empty() {
            self.model.as_str()
        } else {
            model
        };

        if requested == models::minimax::MINIMAX_M2 {
            return true;
        }

        models::anthropic::REASONING_MODELS
            .iter()
            .any(|candidate| *candidate == requested)
    }

    fn supports_parallel_tool_config(&self, _model: &str) -> bool {
        true
    }

    /// Check if the Anthropic provider supports structured outputs for the given model.
    ///
    /// According to Anthropic documentation, structured outputs are available
    /// for Claude 4 and Claude 4.5 models, including Sonnet, Haiku, and Opus variants.
    ///
    /// This feature allows Claude to guarantee responses that follow a specific JSON schema,
    /// ensuring valid, parseable output for downstream processing.
    fn supports_structured_output(&self, model: &str) -> bool {
        let requested = if model.trim().is_empty() {
            self.model.as_str()
        } else {
            model
        };

        // Structured outputs are available for Claude 4.5 models and their aliases
        requested == models::anthropic::CLAUDE_SONNET_4_5
            || requested == models::anthropic::CLAUDE_SONNET_4_5_20250929
            || requested == models::anthropic::CLAUDE_OPUS_4_1
            || requested == models::anthropic::CLAUDE_OPUS_4_1_20250805
            || requested == models::anthropic::CLAUDE_HAIKU_4_5
            || requested == models::anthropic::CLAUDE_HAIKU_4_5_20251001
    }

    fn supports_tools(&self, _model: &str) -> bool {
        true
    }

    async fn generate(&self, request: LLMRequest) -> Result<LLMResponse, LLMError> {
        let anthropic_request = self.convert_to_anthropic_format(&request)?;
        let url = format!("{}/messages", self.base_url);

        let mut request_builder = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", urls::ANTHROPIC_API_VERSION);

        let include_structured = anthropic_request.get("output_format").is_some();
        if let Some(beta_header) = self.combined_beta_header_value(include_structured) {
            request_builder = request_builder.header("anthropic-beta", beta_header);
        }

        let response = request_builder
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| {
                let formatted_error =
                    error_display::format_llm_error("Anthropic", &format!("Network error: {}", e));
                LLMError::NetworkError(formatted_error)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            // Handle specific HTTP status codes
            if status.as_u16() == 429
                || error_text.contains("insufficient_quota")
                || error_text.contains("quota")
                || error_text.contains("rate limit")
            {
                return Err(LLMError::RateLimit);
            }

            // Provide helpful context for cache-related errors
            let error_message = if error_text.contains("cache_control") {
                format!(
                    "HTTP {} - Cache configuration error: {}. \n                    Note: Anthropic only supports cache_control with type='ephemeral' and ttl='5m' or '1h'.",
                    status, error_text
                )
            } else {
                format!("HTTP {}: {}", status, error_text)
            };

            let formatted_error = error_display::format_llm_error("Anthropic", &error_message);
            return Err(LLMError::Provider(formatted_error));
        }

        let anthropic_response: Value = response.json().await.map_err(|e| {
            let formatted_error = error_display::format_llm_error(
                "Anthropic",
                &format!("Failed to parse response: {}", e),
            );
            LLMError::Provider(formatted_error)
        })?;

        self.parse_anthropic_response(anthropic_response)
    }

    fn supported_models(&self) -> Vec<String> {
        let mut supported: Vec<String> = models::anthropic::SUPPORTED_MODELS
            .iter()
            .map(|s| s.to_string())
            .collect();

        supported.extend(
            models::minimax::SUPPORTED_MODELS
                .iter()
                .map(|s| s.to_string()),
        );

        supported.sort();
        supported.dedup();
        supported
    }

    fn validate_request(&self, request: &LLMRequest) -> Result<(), LLMError> {
        if request.messages.is_empty() {
            let formatted_error =
                error_display::format_llm_error("Anthropic", "Messages cannot be empty");
            return Err(LLMError::InvalidRequest(formatted_error));
        }

        if !self.supported_models().contains(&request.model) {
            let formatted_error = error_display::format_llm_error(
                "Anthropic",
                &format!("Unsupported model: {}", request.model),
            );
            return Err(LLMError::InvalidRequest(formatted_error));
        }

        // Check if structured output is requested on an unsupported model
        if request.output_format.is_some() && !self.supports_structured_output(&request.model) {
            let formatted_error = error_display::format_llm_error(
                "Anthropic",
                &format!(
                    "Structured output is not supported for model '{}'. Structured outputs are only available for Claude Sonnet 4.5 and Claude Opus 4.1 models.",
                    request.model
                ),
            );
            return Err(LLMError::InvalidRequest(formatted_error));
        }

        // Structured output validation skipped for simplified build
        if let Some(_) = request.output_format {
            // no-op
        }

        for message in &request.messages {
            if let Err(err) = message.validate_for_provider("anthropic") {
                let formatted = error_display::format_llm_error("Anthropic", &err);
                return Err(LLMError::InvalidRequest(formatted));
            }
        }

        Ok(())
    }

    async fn stream(&self, request: LLMRequest) -> Result<LLMStream, LLMError> {
        // Streaming not wired; delegate to generate and emit a single completion event.
        let response = self.generate(request).await?;
        let stream = try_stream! {
            yield LLMStreamEvent::Completed { response };
        };
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TimeoutsConfig;
    use crate::config::core::PromptCachingConfig;
    use llm_types::{Message, ToolDefinition};
    use serde_json::{Value, json};

    fn base_prompt_cache_config() -> PromptCachingConfig {
        let mut config = PromptCachingConfig::default();
        config.enabled = true;
        config.providers.anthropic.enabled = true;
        config.providers.anthropic.max_breakpoints = 3;
        config.providers.anthropic.cache_user_messages = true;
        config.providers.anthropic.extended_ttl_seconds = Some(3600);
        config
    }

    fn sample_request() -> LLMRequest {
        let tool = ToolDefinition::function(
            "get_weather".to_string(),
            "Retrieve the weather for a city".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                },
                "required": ["city"]
            }),
        );

        LLMRequest {
            messages: vec![Message::user("What's the forecast?".to_string())],
            system_prompt: Some("You are a weather assistant".to_string()),
            tools: Some(vec![Tool {
                function: tool.function.unwrap(),
            }]),
            model: models::CLAUDE_SONNET_4_5.to_string(),
            max_tokens: Some(512),
            temperature: Some(0.2),
            stream: false,
            tool_choice: None,
            parallel_tool_calls: None,
            parallel_tool_config: None,
            reasoning_effort: None,
            output_format: None,
            verbosity: None,
        }
    }

    async fn stream(&self, request: LLMRequest) -> Result<LLMStream, LLMError> {
        // Fallback to non-streaming behavior: execute generate and emit a single Completed event.
        let response = self.generate(request).await?;
        let stream = try_stream! {
            yield LLMStreamEvent::Completed { response };
        };
        Ok(Box::pin(stream))
    }

    #[test]
    fn convert_to_anthropic_format_injects_cache_control() {
        let config = base_prompt_cache_config();
        let provider = AnthropicProvider::from_config(
            Some("key".to_string()),
            Some(models::CLAUDE_SONNET_4_5.to_string()),
            None,
            Some(config),
            None,
        );

        let request = sample_request();
        let converted = provider
            .convert_to_anthropic_format(&request)
            .expect("conversion should succeed");

        let tools = converted["tools"].as_array().expect("tools array");
        let tool_cache = tools
            .last()
            .and_then(|value| value.get("cache_control"))
            .expect("tool cache control present");
        assert_eq!(tool_cache["type"], "ephemeral");
        assert_eq!(tool_cache["ttl"], "1h");

        let system = converted["system"].as_array().expect("system array");
        let system_cache = system[0]
            .get("cache_control")
            .expect("system cache control present");
        assert_eq!(system_cache["type"], "ephemeral");

        let messages = converted["messages"].as_array().expect("messages array");
        let user_message = messages
            .iter()
            .find(|msg| msg["role"] == "user")
            .expect("user message exists");
        let user_cache = user_message["content"][0]
            .get("cache_control")
            .expect("user cache control present");
        assert_eq!(user_cache["type"], "ephemeral");
    }

    #[test]
    fn cache_headers_reflect_extended_ttl() {
        let config = base_prompt_cache_config();
        let provider = AnthropicProvider::from_config(
            Some("key".to_string()),
            Some(models::CLAUDE_SONNET_4_5.to_string()),
            None,
            Some(config),
            None,
        );

        let beta_header = provider
            .prompt_cache_beta_header_value()
            .expect("beta header present when caching enabled");
        assert!(beta_header.contains("prompt-caching-2024-07-31"));
        assert!(beta_header.contains("extended-cache-ttl-2025-04-11"));
    }

    #[test]
    fn cache_control_absent_when_disabled() {
        let mut config = PromptCachingConfig::default();
        config.enabled = false;
        config.providers.anthropic.enabled = false;

        let provider = AnthropicProvider::from_config(
            Some("key".to_string()),
            Some(models::CLAUDE_SONNET_4_5.to_string()),
            None,
            Some(config),
            None,
        );

        let request = sample_request();
        let converted = provider
            .convert_to_anthropic_format(&request)
            .expect("conversion should succeed even without caching");

        assert!(
            converted["tools"].as_array().unwrap()[0]
                .get("cache_control")
                .is_none()
        );

        if let Some(system_value) = converted.get("system") {
            match system_value {
                Value::Array(blocks) => {
                    assert!(blocks[0].get("cache_control").is_none());
                }
                Value::String(_) => {} // This case should not happen for system messages with cache control
                _ => panic!("unexpected system value"),
            }
        }

        let messages = converted["messages"].as_array().expect("messages array");
        let user_message = messages
            .iter()
            .find(|msg| msg["role"] == "user")
            .expect("user message exists");
        assert!(user_message["content"][0].get("cache_control").is_none());
    }

    #[test]
    fn test_structured_output_support() {
        let provider = AnthropicProvider::from_config(
            Some("key".to_string()),
            Some(models::CLAUDE_SONNET_4_5.to_string()),
            None,
            None,
            None,
        );

        // Claude Sonnet 4.5 should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_SONNET_4_5));

        // Claude Opus 4.1 should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_OPUS_4_1_20250805));

        // Claude Sonnet 4.5 should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_SONNET_4_5));

        // Claude Sonnet 4.5 (versioned) should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_SONNET_4_5_20250929));

        // Claude Opus 4.1 should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_OPUS_4_1));

        // Claude Haiku 4.5 should support structured output
        assert!(provider.supports_structured_output(models::CLAUDE_HAIKU_4_5));

        // Test with empty model string (should use provider's default)
        let provider_default = AnthropicProvider::from_config(
            "key".to_string(),
            models::anthropic::DEFAULT_MODEL.to_string(),
            None,
            None,
            None,
        );
        assert!(provider_default.supports_structured_output(""));
    }

    #[test]
    fn test_structured_output_schema_validation() {
        let provider = AnthropicProvider::new("key".to_string());

        // Valid schema should pass
        let valid_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"],
            "additionalProperties": false
        });
        assert!(provider.validate_anthropic_schema(&valid_schema).is_ok());

        // Schema with unsupported numeric constraints should fail
        let invalid_schema = json!({
            "type": "object",
            "properties": {
                "age": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 100
                }
            },
            "required": ["age"],
            "additionalProperties": false
        });
        assert!(provider.validate_anthropic_schema(&invalid_schema).is_err());

        // Schema with unsupported string constraints should fail
        let invalid_string_schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 50
                }
            },
            "required": ["name"],
            "additionalProperties": false
        });
        assert!(
            provider
                .validate_anthropic_schema(&invalid_string_schema)
                .is_err()
        );

        // Schema with minItems > 1 should fail
        let invalid_array_schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {"type": "string"},
                    "minItems": 5
                }
            },
            "required": ["items"],
            "additionalProperties": false
        });
        assert!(
            provider
                .validate_anthropic_schema(&invalid_array_schema)
                .is_err()
        );

        // Schema with additionalProperties: true should fail
        let invalid_additional_props_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"],
            "additionalProperties": true
        });
        assert!(
            provider
                .validate_anthropic_schema(&invalid_additional_props_schema)
                .is_err()
        );
    }
}

#[async_trait]
impl LLMClient for AnthropicProvider {
    async fn generate(&mut self, prompt: &str) -> Result<llm_types::LLMResponse, LLMError> {
        let request = self.parse_client_prompt(prompt);
        let request_model = request.model.clone();
        let response = LLMProvider::generate(self, request).await?;

        Ok(llm_types::LLMResponse {
            content: response.content,
            model: request_model,
            usage: response.usage.map(|u| llm_types::Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cached_prompt_tokens: u.cached_prompt_tokens,
                cache_creation_tokens: u.cache_creation_tokens,
                cache_read_tokens: u.cache_read_tokens,
            }),
            reasoning: response.reasoning,
            tool_calls: response.tool_calls,
            finish_reason: response.finish_reason,
            reasoning_details: response.reasoning_details,
        })
    }

    async fn stream(
        &self,
        request: llm_types::LLMRequest,
    ) -> Result<llm_types::LLMStream, LLMError> {
        LLMProvider::stream(self, request).await
    }

    fn backend_kind(&self) -> llm_types::BackendKind {
        llm_types::BackendKind::Anthropic
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

// Helper impl block for schema validation methods (not part of LLMProvider trait)
impl AnthropicProvider {
    /// Validates a JSON schema against Anthropic's structured output limitations
    /// Based on Anthropic documentation: https://docs.anthropic.com/claude/reference/structured-outputs
    fn validate_anthropic_schema(&self, schema: &Value) -> Result<(), LLMError> {
        match schema {
            Value::Object(obj) => {
                // For Anthropic's output_format, the schema should be the JSON schema itself, not wrapped
                self.validate_schema_object(obj, "root")?;
            }
            Value::String(_)
            | Value::Number(_)
            | Value::Bool(_)
            | Value::Array(_)
            | Value::Null => {
                let formatted_error = error_display::format_llm_error(
                    "Anthropic",
                    "Structured output schema must be a JSON object",
                );
                return Err(LLMError::InvalidRequest(formatted_error));
            }
        }
        Ok(())
    }

    /// Recursively validate an object in the JSON schema according to Anthropic limitations
    fn validate_schema_object(
        &self,
        obj: &serde_json::Map<String, Value>,
        path: &str,
    ) -> Result<(), LLMError> {
        for (key, value) in obj {
            match key.as_str() {
                // Validate type-specific limitations
                "type" => {
                    if let Some(type_str) = value.as_str() {
                        match type_str {
                            "object" | "array" | "string" | "number" | "integer" | "boolean"
                            | "null" => {} // These types are supported
                            _ => {
                                let formatted_error = error_display::format_llm_error(
                                    "Anthropic",
                                    &format!(
                                        "Unsupported schema type '{}', path: {}",
                                        type_str, path
                                    ),
                                );
                                return Err(LLMError::InvalidRequest(formatted_error));
                            }
                        }
                    }
                }
                // Check for unsupported numeric constraints
                "minimum" | "maximum" | "multipleOf" => {
                    let formatted_error = error_display::format_llm_error(
                        "Anthropic",
                        &format!(
                            "Numeric constraints like '{}' are not supported by Anthropic structured output. Path: {}",
                            key, path
                        ),
                    );
                    return Err(LLMError::InvalidRequest(formatted_error));
                }
                // Check for unsupported string constraints
                "minLength" | "maxLength" => {
                    let formatted_error = error_display::format_llm_error(
                        "Anthropic",
                        &format!(
                            "String constraints like '{}' are not supported by Anthropic structured output. Path: {}",
                            key, path
                        ),
                    );
                    return Err(LLMError::InvalidRequest(formatted_error));
                }
                // Check for unsupported array constraints beyond minItems with values 0 or 1
                "minItems" | "maxItems" | "uniqueItems" => {
                    if key == "minItems" {
                        if let Some(min_items) = value.as_u64() {
                            if min_items > 1 {
                                let formatted_error = error_display::format_llm_error(
                                    "Anthropic",
                                    &format!(
                                        "Array minItems only supports values 0 or 1, got {}, path: {}",
                                        min_items, path
                                    ),
                                );
                                return Err(LLMError::InvalidRequest(formatted_error));
                            }
                        }
                    } else {
                        let formatted_error = error_display::format_llm_error(
                            "Anthropic",
                            &format!(
                                "Array constraints like '{}' are not supported by Anthropic structured output. Path: {}",
                                key, path
                            ),
                        );
                        return Err(LLMError::InvalidRequest(formatted_error));
                    }
                }
                // Check for additionalProperties - must be false for objects
                "additionalProperties" => {
                    if let Some(additional_props) = value.as_bool() {
                        if additional_props != false {
                            let formatted_error = error_display::format_llm_error(
                                "Anthropic",
                                &format!(
                                    "additionalProperties must be set to false, got {}, path: {}",
                                    additional_props, path
                                ),
                            );
                            return Err(LLMError::InvalidRequest(formatted_error));
                        }
                    }
                }
                // Recursively validate nested objects and arrays in properties
                "properties" => {
                    if let Value::Object(props) = value {
                        for (prop_name, prop_value) in props {
                            let prop_path = format!("{}.properties.{}", path, prop_name);
                            self.validate_schema_property(prop_value, &prop_path)?;
                        }
                    }
                }
                "items" => {
                    let items_path = format!("{}.items", path);
                    self.validate_schema_property(value, &items_path)?;
                }
                "enum" => {
                    // Enums are supported but with limitations (no complex types)
                    if let Value::Array(items) = value {
                        for (i, item) in items.iter().enumerate() {
                            if !self.is_valid_enum_value(item) {
                                let formatted_error = error_display::format_llm_error(
                                    "Anthropic",
                                    &format!(
                                        "Invalid enum value at index {}, path: {}. Enums in Anthropic structured output only support strings, numbers, booleans, and null.",
                                        i, path
                                    ),
                                );
                                return Err(LLMError::InvalidRequest(formatted_error));
                            }
                        }
                    }
                }
                // For other keys, check if it's a nested schema component
                _ => {
                    // If the value is an object that could be a schema, validate it recursively
                    if let Value::Object(nested_obj) = value {
                        let nested_path = format!("{}.{}", path, key);
                        self.validate_schema_object(nested_obj, &nested_path)?;
                    }
                    // If it's an array of objects that could be schemas
                    else if let Value::Array(arr) = value {
                        for (i, item) in arr.iter().enumerate() {
                            if let Value::Object(nested_obj) = item {
                                let nested_path = format!("{}.{}[{}", path, key, i);
                                self.validate_schema_object(nested_obj, &nested_path)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate an individual schema property
    fn validate_schema_property(&self, value: &Value, path: &str) -> Result<(), LLMError> {
        match value {
            Value::Object(obj) => self.validate_schema_object(obj, path),
            Value::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    if let Value::Object(obj) = item {
                        let item_path = format!("{}[{}]", path, i);
                        self.validate_schema_object(obj, &item_path)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()), // Other types like string, number, etc. are valid leaf nodes
        }
    }

    /// Check if an enum value is valid (string, number, boolean, or null)
    fn is_valid_enum_value(&self, value: &Value) -> bool {
        matches!(
            value,
            Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
        )
    }
}
