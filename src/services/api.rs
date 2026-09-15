mod error;
mod http;
mod jobs;
mod repos;
mod runs;
mod workflows;

pub mod client;

// The data models are domain vocabulary and live in `domain/`. This re-export
// keeps every existing `crate::services::api::models::…` path resolving.
pub use crate::domain::models;

// Re-export main types for convenience
pub use client::GitHubClient;
pub use error::GitHubError;
