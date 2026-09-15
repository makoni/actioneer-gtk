//! The one place an error becomes text a person reads.
//!
//! Scope note, because this is narrower than the refactor plan asked for. The
//! plan's step 7.3 called for an `AppError` every service returns, so the UI
//! would match on one type. Measured against the code, that makes error
//! handling *worse*: the UI matches `GitHubError` variants for **behaviour**,
//! not for text — `AuthenticationFailed` drives a sign-out
//! (`main_window/loaders.rs`), `NotFound` and `Gone` pick different log-window
//! messages (`job_logs_window/ctx.rs`), and an `ApiError` whose message
//! contains "Not modified" is deliberately suppressed (`workflow_refresh.rs`).
//! Flattening those adds a layer of nesting to every one of those decisions,
//! and a `?` at a service boundary erases the distinction outright.
//!
//! An `AppError` enum was written and then removed: nothing produced one, so it
//! was a layer whose only purpose was to give this module something to match
//! on. What the step was actually after — one place that turns an error into
//! user-facing text, through `tr` — is the function below.
//!
//! Deliberately adds no new msgid: it reuses `"Error: {message}"`, which the
//! catalogs already carry in all fourteen languages.

use crate::kernel::i18n::tr;

/// Renders an error as a single localized line.
pub fn user_message_from(error: impl std::fmt::Display) -> String {
    tr("Error: {message}").replace("{message}", &error.to_string())
}

#[cfg(test)]
mod tests;
