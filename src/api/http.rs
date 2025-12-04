/// HTTP client utilities and helpers
use super::error::GitHubError;
use crate::api::models::RateLimitInfo;
use reqwest::{Response, StatusCode, header};
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::warn;

pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// Handles common response processing including rate limit tracking
pub(super) struct ResponseHandler {
    rate_limit: Arc<StdMutex<Option<RateLimitInfo>>>,
}

impl ResponseHandler {
    pub fn new(rate_limit: Arc<StdMutex<Option<RateLimitInfo>>>) -> Self {
        Self { rate_limit }
    }

    pub fn get_rate_limit(&self) -> Option<RateLimitInfo> {
        self.rate_limit
            .lock()
            .ok()
            .and_then(|guard| (*guard).clone())
    }

    pub fn apply_cache_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        // HTTP caching has been disabled because GitHub's workflow endpoints are too dynamic.
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

    pub async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, GitHubError> {
        let status = response.status();
        let headers = response.headers().clone();
        self.update_rate_limit(&headers);

        match status {
            StatusCode::OK | StatusCode::CREATED => {
                let body = response.bytes().await?;
                self.deserialize_json(body.as_ref())
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                warn!("Authentication failed with status: {}", status);
                Err(GitHubError::AuthenticationFailed)
            }
            StatusCode::NOT_FOUND => {
                warn!("Resource not found");
                Err(GitHubError::NotFound)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                warn!("Rate limit exceeded");
                Err(GitHubError::RateLimitExceeded)
            }
            other => {
                let text = response.text().await.unwrap_or_default();
                warn!("API error {}: {}", other, text);
                Err(GitHubError::ApiError(format!("Status {}: {}", other, text)))
            }
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
