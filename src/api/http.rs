/// HTTP client utilities and helpers
use super::error::GitHubError;
use crate::api::models::RateLimitInfo;
use reqwest::{Response, StatusCode, header};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tracing::warn;

pub const GITHUB_API_BASE: &str = "https://api.github.com";
const ETAG_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct CachedResponse {
    etag: String,
    body: Vec<u8>,
    stored_at: Instant,
}

/// Handles common response processing including rate limit tracking
pub(super) struct ResponseHandler {
    rate_limit: Arc<StdMutex<Option<RateLimitInfo>>>,
    cache: Arc<StdMutex<HashMap<String, CachedResponse>>>,
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
        let Some(cache_key) = cache_key else {
            return request;
        };

        if let Some(entry) = self.active_cache_entry(cache_key) {
            request.header(header::IF_NONE_MATCH, entry.etag)
        } else {
            request
        }
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
        cache_key: Option<&str>,
    ) -> Result<T, GitHubError> {
        let status = response.status();
        let headers = response.headers().clone();
        self.update_rate_limit(&headers);

        match status {
            StatusCode::OK | StatusCode::CREATED => {
                let body = response.bytes().await?;
                self.maybe_store_cached_response(cache_key, &headers, body.as_ref());
                self.deserialize_json(body.as_ref())
            }
            StatusCode::NOT_MODIFIED => {
                let Some(cache_key) = cache_key else {
                    return Err(GitHubError::ApiError(
                        "Received 304 Not Modified for a non-cacheable request".to_string(),
                    ));
                };

                let Some(entry) = self.cached_entry(cache_key) else {
                    return Err(GitHubError::ApiError(
                        "Received 304 Not Modified without a cached response body".to_string(),
                    ));
                };

                self.deserialize_json(entry.body.as_ref())
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

    fn maybe_store_cached_response(
        &self,
        cache_key: Option<&str>,
        headers: &header::HeaderMap,
        body: &[u8],
    ) {
        let Some(cache_key) = cache_key else {
            return;
        };

        let etag = headers
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        let Ok(mut cache) = self.cache.lock() else {
            return;
        };

        if let Some(etag) = etag {
            cache.insert(
                cache_key.to_string(),
                CachedResponse {
                    etag,
                    body: body.to_vec(),
                    stored_at: Instant::now(),
                },
            );
        } else {
            cache.remove(cache_key);
        }
    }

    fn active_cache_entry(&self, cache_key: &str) -> Option<CachedResponse> {
        let Ok(mut cache) = self.cache.lock() else {
            return None;
        };

        let entry = cache.get(cache_key)?.clone();
        if entry.stored_at.elapsed() <= ETAG_CACHE_TTL {
            Some(entry)
        } else {
            cache.remove(cache_key);
            None
        }
    }

    fn cached_entry(&self, cache_key: &str) -> Option<CachedResponse> {
        let Ok(cache) = self.cache.lock() else {
            return None;
        };

        cache.get(cache_key).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use std::time::{Duration, Instant};
    use wiremock::matchers::{header as match_header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn handler() -> ResponseHandler {
        ResponseHandler::new(Arc::new(StdMutex::new(None)))
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct TestPayload {
        value: i32,
    }

    #[test]
    fn apply_cache_headers_uses_matching_etag() {
        let handler = handler();
        handler.cache.lock().unwrap().insert(
            "workflows:demo/repo".into(),
            CachedResponse {
                etag: "\"etag-123\"".into(),
                body: br#"{"value":1}"#.to_vec(),
                stored_at: Instant::now(),
            },
        );

        let request = handler
            .apply_cache_headers(
                Client::new().get("https://example.com/test"),
                Some("workflows:demo/repo"),
            )
            .build()
            .expect("build request");

        assert_eq!(
            request
                .headers()
                .get(header::IF_NONE_MATCH)
                .expect("if-none-match header"),
            "\"etag-123\""
        );
    }

    #[test]
    fn apply_cache_headers_skips_expired_entries() {
        let handler = handler();
        handler.cache.lock().unwrap().insert(
            "branches:demo/repo".into(),
            CachedResponse {
                etag: "\"expired\"".into(),
                body: br#"[]"#.to_vec(),
                stored_at: Instant::now() - Duration::from_secs(61),
            },
        );

        let request = handler
            .apply_cache_headers(
                Client::new().get("https://example.com/test"),
                Some("branches:demo/repo"),
            )
            .build()
            .expect("build request");

        assert!(
            request.headers().get(header::IF_NONE_MATCH).is_none(),
            "expired cache entries must not produce conditional headers"
        );
    }

    #[test]
    fn apply_cache_headers_skips_non_cacheable_requests() {
        let request = handler()
            .apply_cache_headers(Client::new().get("https://example.com/test"), None)
            .build()
            .expect("build request");

        assert!(
            request.headers().get(header::IF_NONE_MATCH).is_none(),
            "requests without cache keys must stay unconditional"
        );
    }

    #[tokio::test]
    async fn handle_response_reuses_cached_body_for_not_modified() {
        let server = MockServer::start().await;
        let key = "workflows:demo/repo";

        Mock::given(method("GET"))
            .and(path("/cached"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"etag-304\"")
                    .set_body_json(serde_json::json!({ "value": 42 })),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/cached"))
            .and(match_header("if-none-match", "\"etag-304\""))
            .respond_with(ResponseTemplate::new(304))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let handler = handler();
        let client = Client::new();

        let response = handler
            .apply_cache_headers(client.get(format!("{}/cached", server.uri())), Some(key))
            .send()
            .await
            .expect("send initial request");
        let initial: TestPayload = handler
            .handle_response(response, Some(key))
            .await
            .expect("cache initial body");
        assert_eq!(initial, TestPayload { value: 42 });

        let response = handler
            .apply_cache_headers(client.get(format!("{}/cached", server.uri())), Some(key))
            .send()
            .await
            .expect("send conditional request");
        let cached: TestPayload = handler
            .handle_response(response, Some(key))
            .await
            .expect("reuse cached body");
        assert_eq!(cached, TestPayload { value: 42 });
    }

    #[tokio::test]
    async fn handle_response_reuses_cached_body_after_ttl_expiry_if_server_returns_304() {
        let server = MockServer::start().await;
        let key = "workflows:demo/repo";

        Mock::given(method("GET"))
            .and(path("/ttl-race"))
            .and(match_header("if-none-match", "\"etag-race\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&server)
            .await;

        let handler = handler();
        handler.cache.lock().unwrap().insert(
            key.into(),
            CachedResponse {
                etag: "\"etag-race\"".into(),
                body: br#"{"value":7}"#.to_vec(),
                stored_at: Instant::now(),
            },
        );

        let client = Client::new();
        let request = handler
            .apply_cache_headers(client.get(format!("{}/ttl-race", server.uri())), Some(key))
            .build()
            .expect("build request");
        assert_eq!(
            request
                .headers()
                .get(header::IF_NONE_MATCH)
                .expect("if-none-match header"),
            "\"etag-race\""
        );

        handler.cache.lock().unwrap().insert(
            key.into(),
            CachedResponse {
                etag: "\"etag-race\"".into(),
                body: br#"{"value":7}"#.to_vec(),
                stored_at: Instant::now() - Duration::from_secs(61),
            },
        );

        let response = client
            .execute(request)
            .await
            .expect("send conditional request");
        let cached: TestPayload = handler
            .handle_response(response, Some(key))
            .await
            .expect("reuse cached body after ttl expiry");
        assert_eq!(cached, TestPayload { value: 7 });
    }
}
