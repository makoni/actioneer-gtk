use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Not found")]
    NotFound,

    #[error("Resource no longer available")]
    Gone,
}
