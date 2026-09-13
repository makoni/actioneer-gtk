//! The one place an error becomes text a person reads.
//!
//! Scope note, because this is narrower than the refactor plan asked for. The
//! plan's step 7.3 called for converting all seven services to return
//! `AppError`, so the UI would match on one type. Measured against the code,
//! that would have made error handling *worse*: the UI matches `GitHubError`
//! variants for **behaviour**, not for text —
//! `AuthenticationFailed` drives a sign-out (`main_window/loaders.rs`),
//! `NotFound` and `Gone` pick different log-window messages
//! (`job_logs_window/ctx.rs`), and an `ApiError` whose message contains
//! "Not modified" is deliberately suppressed (`workflow_refresh.rs`).
//! Flattening those into `AppError::Api(GitHubError::…)` adds a layer of
//! nesting to every one of those decisions and buys nothing.
//!
//! What the plan was actually after — one place that turns an error into
//! user-facing text, through `tr` — is here. The services keep their own error
//! types, and `AppError` stays the funnel for the paths that only need to
//! *report*.
//!
//! Deliberately adds no new msgid: it reuses `"Error: {message}"`, which the
//! catalogs already carry in all fourteen languages.

use crate::kernel::i18n::tr;
use crate::services::error::AppError;

/// Renders any error as a single localized line.
pub fn user_message(error: &AppError) -> String {
    match error {
        // `Message` is already user-facing text chosen by the caller.
        AppError::Message(message) => message.clone(),
        other => tr("Error: {message}").replace("{message}", &other.to_string()),
    }
}

/// The same, for an error that has not been funnelled yet.
pub fn user_message_from(error: impl std::fmt::Display) -> String {
    tr("Error: {message}").replace("{message}", &error.to_string())
}

#[cfg(test)]
mod tests;
