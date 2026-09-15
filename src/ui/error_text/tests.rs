//! Tests for [`super`].

use super::*;
use crate::services::api::GitHubError;

#[test]
fn a_service_error_is_wrapped_in_the_shared_line() {
    let text = user_message_from(GitHubError::NotFound);
    assert!(text.contains("Not found"), "got {text:?}");
    assert!(text.starts_with("Error:"), "got {text:?}");
}

#[test]
fn an_io_error_reads_as_a_sentence_not_a_debug_dump() {
    let text = user_message_from(std::io::Error::other("disk on fire"));
    assert!(text.contains("disk on fire"), "got {text:?}");
    assert!(
        !text.contains("Custom {"),
        "must not leak Debug output: {text:?}"
    );
}

#[test]
fn a_plain_string_passes_through_the_same_shape() {
    assert_eq!(
        user_message_from("the token has expired"),
        "Error: the token has expired"
    );
}
