use crate::llm::provider::LLMRequest;
use crate::llm::types::LLMError;

/// Serialize an [`LLMRequest`] into a JSON payload using a preallocated buffer to
/// minimize allocations before handing it to the cache or provider layer.
pub fn serialize_request(request: &LLMRequest) -> Result<String, LLMError> {
    let mut buffer = Vec::with_capacity(1024);
    serde_json::to_writer(&mut buffer, request).map_err(|err| {
        LLMError::InvalidRequest(format!("Failed to serialize LLMRequest: {err}"))
    })?;

    // SAFETY: `serde_json::to_writer` always emits valid UTF-8.
    Ok(unsafe { String::from_utf8_unchecked(buffer) })
}

/// Deserialize a JSON payload into an [`LLMRequest`].
pub fn deserialize_request(payload: &str) -> Result<LLMRequest, LLMError> {
    serde_json::from_str(payload)
        .map_err(|err| LLMError::InvalidRequest(format!("Failed to parse LLMRequest JSON: {err}")))
}
