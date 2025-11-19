use super::client::{AnyClient, LLMClient};
use super::codec;
use super::provider::{LLMRequest, LLMStream};
use super::types::LLMError;
use super::types::LLMResponse;
use async_trait::async_trait;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Configuration for the LLM response cache.
#[derive(Debug, Clone)]
pub struct LLMCacheConfig {
    /// Whether caching is enabled.
    pub enabled: bool,
    /// Maximum number of entries in the cache.
    pub max_entries: usize,
    /// Time-to-live for cache entries.
    pub ttl: Duration,
}

impl Default for LLMCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 100,                  // Default to 100 cached responses
            ttl: Duration::from_secs(60 * 60), // 1 hour
        }
    }
}

/// A cached LLM response entry.
#[derive(Debug, Clone)]
struct CacheEntry {
    response: LLMResponse,
    timestamp: Instant,
}

/// A caching layer for LLM responses.
pub struct CachedLLMClient {
    inner: AnyClient,
    cache: Arc<DashMap<String, CacheEntry>>,
    config: LLMCacheConfig,
    // Mutex to protect cache eviction logic if needed, or for more complex cache types
    // For DashMap, individual operations are concurrent, but global eviction might need a lock.
    _eviction_lock: Mutex<()>, // Placeholder for potential future eviction logic
}

impl CachedLLMClient {
    pub fn new(inner: AnyClient, config: LLMCacheConfig) -> Self {
        Self {
            inner,
            cache: Arc::new(DashMap::new()),
            config,
            _eviction_lock: Mutex::new(()),
        }
    }

    /// Generates a cache key from an LLMRequest.
    fn generate_cache_key(request: &LLMRequest) -> String {
        let serialized = serde_json::to_string(request).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Performs cache eviction if the cache size exceeds the configured maximum.
    /// This is a basic LRU-like eviction by removing the oldest entries.
    async fn evict_cache(&self) {
        if self.cache.len() > self.config.max_entries {
            let mut entries: Vec<_> = self
                .cache
                .iter()
                .map(|e| (e.key().clone(), e.value().timestamp))
                .collect();
            entries.sort_by_key(|(_, ts)| *ts); // Sort by timestamp (oldest first)

            let num_to_evict = self.cache.len() - self.config.max_entries;
            for i in 0..num_to_evict {
                if let Some((key, _)) = entries.get(i) {
                    self.cache.remove(key);
                }
            }
        }
    }
}

#[async_trait]
impl LLMClient for CachedLLMClient {
    async fn generate(&mut self, request: &str) -> Result<LLMResponse, LLMError> {
        if !self.config.enabled {
            return self.inner.generate(request).await;
        }

        let trimmed = request.trim_start();
        if !trimmed.starts_with('{') {
            // Plain-text prompt; bypass caching to preserve legacy flows.
            return self.inner.generate(request).await;
        }

        let llm_request: LLMRequest = codec::deserialize_request(request)?;

        let key = Self::generate_cache_key(&llm_request);

        // Check cache
        if let Some(entry) = self.cache.get(&key) {
            if entry.timestamp.elapsed() < self.config.ttl {
                // Cache hit, return cached response
                // TODO: Update usage stats for cached tokens
                return Ok(entry.response.clone());
            } else {
                // Entry expired, remove it
                self.cache.remove(&key);
            }
        }

        // Cache miss or expired, call inner client
        let response = self.inner.generate(request).await?;

        // Store in cache
        self.cache.insert(
            key,
            CacheEntry {
                response: response.clone(),
                timestamp: Instant::now(),
            },
        );

        // Perform eviction if needed (can be done in a background task for larger caches)
        self.evict_cache().await;

        Ok(response)
    }

    fn backend_kind(&self) -> super::types::BackendKind {
        self.inner.backend_kind()
    }

    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    // For stream, we currently don't cache. Just delegate to the inner client.
    // Caching streamed responses is more complex and will be considered in future iterations.
    async fn stream(&self, request: LLMRequest) -> Result<LLMStream, LLMError> {
        // If caching is enabled and a response exists, we could potentially return a stream
        // from the cached response, but for simplicity, we'll just delegate for now.
        self.inner.stream(request).await
    }
}
