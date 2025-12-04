/// HTTP client utilities and helpers
use super::error::GitHubError;
use crate::api::models::RateLimitInfo;
use reqwest::{Response, StatusCode, header};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, warn};

pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// Handles common response processing including rate limit tracking
pub(super) struct ResponseHandler {
    rate_limit: Arc<StdMutex<Option<RateLimitInfo>>>,
    cache: Arc<StdMutex<HashMap<String, CachedResponse>>>,
}

#[derive(Clone)]
struct CachedResponse {
    etag: String,
    body: Arc<Vec<u8>>,
}

impl ResponseHandler {
    pub fn new(rate_limit: Arc<StdMutex<Option<RateLimitInfo>>>) -> Self {
        Self {
            rate_limit,
            cache: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    pub fn get_rate_limit(&self) -> Option<RateLimitInfo> {
        self.rate_limit
            .lock()
            .ok()
            .and_then(|guard| (*guard).clone())
    }

    pub fn apply_cache_headers(
        &self,
        request: reqwest::RequestBuilder,
        cache_key: Option<&str>,
    ) -> reqwest::RequestBuilder {
        if let Some(key) = cache_key
            && let Some(etag) = self.cached_etag(key)
        {
            debug!(cache_key = key, etag = %etag, "Applying conditional GET with cached ETag");
            return request.header(header::IF_NONE_MATCH, etag);
        }

        request
    }

    pub fn update_rate_limit(&self, headers: &header::HeaderMap) {
        let limit = headers.get("x-ratelimit-limit");
        let remaining = headers.get("x-ratelimit-remaining");
        let reset = headers.get("x-ratelimit-reset");

        if let (Some(limit), Some(remaining), Some(reset)) = (limit, remaining, reset)
            && let (Ok(limit), Ok(remaining), Ok(reset)) = (
                limit.to_str().unwrap_or_default().parse::<i64>(),
                remaining.to_str().unwrap_or_default().parse::<i64>(),
                reset.to_str().unwrap_or_default().parse::<i64>(),
            )
        {
            let info = RateLimitInfo {
                limit,
                remaining,
                reset,
            };

            if let Ok(mut guard) = self.rate_limit.lock() {
                *guard = Some(info);
            }
        }
    }

    pub async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: Response,
        cache_key: Option<&str>,
    ) -> Result<T, GitHubError> {
        let status = response.status();
        let headers = response.headers().clone();
        self.update_rate_limit(&headers);

        match status {
            StatusCode::OK | StatusCode::CREATED => {
                let body = response.bytes().await?;
                if let Some(key) = cache_key {
                    self.store_cache_entry(key, &headers, &body);
                }
                self.deserialize_json(body.as_ref())
            }
            StatusCode::NOT_MODIFIED => {
                if let Some(key) = cache_key
                    && let Some(cached) = self.cached_body(key)
                {
                    debug!(
                        cache_key = key,
                        "Received 304 Not Modified; returning cached body"
                    );
                    return self.deserialize_json(cached.as_slice());
                }

                warn!("Received 304 Not Modified but no cached response available");
                Err(GitHubError::ApiError(
                    "Not modified but no cached response available".to_string(),
                ))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                warn!("Authentication failed with status: {}", status);
                Err(GitHubError::AuthenticationFailed)
            }
            StatusCode::NOT_FOUND => {
                debug!("Resource not found");
                Err(GitHubError::NotFound)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                warn!("Rate limit exceeded");
                Err(GitHubError::RateLimitExceeded)
            }
            _ => {
                let text = response.text().await.unwrap_or_default();
                warn!("API error {}: {}", status, text);
                Err(GitHubError::ApiError(format!(
                    "Status {}: {}",
                    status, text
                )))
            }
        }
    }

    pub(super) fn store_cache_entry(
        &self,
        cache_key: &str,
        headers: &header::HeaderMap,
        body: &[u8],
    ) {
        if let Some(etag) = headers
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .filter(|etag| !etag.is_empty())
        {
            let entry = CachedResponse {
                etag: etag.to_string(),
                body: Arc::new(body.to_vec()),
            };

            if let Ok(mut guard) = self.cache.lock() {
                guard.insert(cache_key.to_string(), entry);
            }
        }
    }

    pub(super) fn cached_json<T: DeserializeOwned>(
        &self,
        cache_key: &str,
    ) -> Result<Option<T>, GitHubError> {
        if let Some(body) = self.cached_body(cache_key) {
            return self.deserialize_json(body.as_slice()).map(Some);
        }

        Ok(None)
    }

    pub(super) fn cached_body(&self, cache_key: &str) -> Option<Arc<Vec<u8>>> {
        self.cache
            .lock()
            .ok()
            .and_then(|guard| guard.get(cache_key).cloned())
            .map(|entry| entry.body)
    }

    fn cached_etag(&self, cache_key: &str) -> Option<String> {
        self.cache
            .lock()
            .ok()
            .and_then(|guard| guard.get(cache_key).map(|entry| entry.etag.clone()))
    }

    /// Remove a cached response (and its ETag) so the next request bypasses
    /// conditional GET handling.
    pub fn clear_cache_entry(&self, cache_key: &str) {
        if let Ok(mut guard) = self.cache.lock() {
            guard.remove(cache_key);
        }
    }

    fn deserialize_json<T: DeserializeOwned>(&self, body: &[u8]) -> Result<T, GitHubError> {
        serde_json::from_slice(body).map_err(|error| {
            GitHubError::ApiError(format!("Failed to parse response body: {}", error))
        })
    }
}

/// Helper to add authorization header to requests
pub(super) fn add_auth_header(
    request: reqwest::RequestBuilder,
    token: &Option<String>,
) -> reqwest::RequestBuilder {
    if let Some(t) = token {
        request.header(header::AUTHORIZATION, format!("Bearer {}", t))
    } else {
        request
    }
}
