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

impl GitHubError {
    /// True when GitHub could not be reached at all — no connection, no DNS,
    /// a connection that died before the answer, or no answer in time — as
    /// opposed to GitHub answering with an error.
    pub fn is_offline(&self) -> bool {
        match self {
            // `is_request`: the request could not be sent, or the connection
            // closed before any answer — what a pooled connection does when
            // the network goes away under it.
            GitHubError::RequestFailed(error) => {
                error.is_connect()
                    || error.is_timeout()
                    || error.is_request()
                    || lost_connection(error)
            }
            _ => false,
        }
    }
}

/// A connection that died while the answer was being read surfaces as an I/O
/// error somewhere in the chain.
fn lost_connection(error: &reqwest::Error) -> bool {
    use std::io::ErrorKind;
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return matches!(
                io.kind(),
                ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
                    | ErrorKind::NotConnected
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::TimedOut
                    | ErrorKind::NetworkUnreachable
                    | ErrorKind::HostUnreachable
                    | ErrorKind::NetworkDown
            );
        }
        source = cause.source();
    }
    false
}
