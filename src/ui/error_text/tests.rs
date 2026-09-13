//! Tests for [`super`].

use super::*;
use crate::services::api::GitHubError;

#[test]
fn a_plain_message_is_passed_through_unchanged() {
    // Already user-facing: wrapping it in "Error: …" would double the prefix.
    let error = AppError::Message("The token has expired.".into());
    assert_eq!(user_message(&error), "The token has expired.");
}

#[test]
fn a_service_error_is_wrapped_in_the_shared_line() {
    let error = AppError::Api(GitHubError::NotFound);
    let text = user_message(&error);
    assert!(text.contains("Not found"), "got {text:?}");
    assert!(text.starts_with("Error:"), "got {text:?}");
}

#[test]
fn an_io_error_reads_as_a_sentence_not_a_debug_dump() {
    let error = AppError::Io(std::io::Error::other("disk on fire"));
    let text = user_message(&error);
    assert!(text.contains("disk on fire"), "got {text:?}");
    assert!(
        !text.contains("Custom {"),
        "must not leak Debug output: {text:?}"
    );
}

#[test]
fn an_unfunnelled_error_renders_the_same_way() {
    assert_eq!(
        user_message_from(GitHubError::NotFound),
        user_message(&AppError::Api(GitHubError::NotFound)),
    );
}
