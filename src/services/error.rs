//! The single error every layer funnels into.
//!
//! It lives above the services and not in `kernel/`: the variants name
//! `GitHubError` and `StorageError`, so a kernel-hosted funnel would make the
//! bottom layer depend on the adapters — exactly the edge the kernel exists to
//! forbid.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Api(#[from] crate::services::api::GitHubError),

    #[error(transparent)]
    Storage(#[from] crate::services::tokens::StorageError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Message(String),
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Message(err.to_string())
    }
}
