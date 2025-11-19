//! Ask command implementation - single prompt without tools

use crate::config::models::ModelId;
use crate::config::types::AgentConfig;
use crate::llm::cache::LLMCacheConfig;
use crate::llm::make_client;
use crate::llm::provider::{LLMRequest, Message, ToolChoice};
use crate::prompts::{generate_lightweight_instruction, read_system_prompt_from_md};
use anyhow::Result;

/// Handle the ask command - single prompt without tools
pub async fn handle_ask_command(config: AgentConfig, prompt: Vec<String>) -> Result<()> {
    let model_id = config
        .model
        .parse::<ModelId>()
        .map_err(|_| anyhow::anyhow!("Invalid model: {}", config.model))?;
    let mut client = make_client(config.api_key.clone(), model_id, LLMCacheConfig::default());
    let prompt_text = prompt.join(" ");

    if config.verbose {
        println!("Sending prompt to {}: {}", config.model, prompt_text);
    }

    let lightweight_instruction = generate_lightweight_instruction();

    let system_prompt = if let Some(text) = lightweight_instruction
        .parts
        .first()
        .and_then(|part| part.as_text())
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
    {
        text.to_string()
    } else {
        read_system_prompt_from_md()
            .await
            .unwrap_or_else(|_| "You are a helpful coding assistant.".to_string())
    };

    let request = LLMRequest {
        messages: vec![Message::user(prompt_text)],
        system_prompt: Some(system_prompt),
        tools: None,
        model: config.model.clone(),
        max_tokens: None,
        temperature: None,
        stream: false,
        tool_choice: Some(ToolChoice::none()),
        parallel_tool_calls: None,
        parallel_tool_config: None,
        reasoning_effort: Some(config.reasoning_effort),
        output_format: None,
        verbosity: None,
    };

    let response = client.generate_request(&request).await?;

    // Print the response content directly
    println!("{}", response.content);

    Ok(())
}
