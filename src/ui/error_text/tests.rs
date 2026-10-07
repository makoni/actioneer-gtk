//! Tests for [`super`].

use super::*;
use crate::kernel::i18n::{apply_language_preference, i18n_test_guard, init};
use crate::services::api::GitHubError;
use crate::services::preferences::LanguagePreference;

/// The messages go through `tr`, and other tests switch the process-wide
/// language, so hold the i18n lock and pin English for the assertion.
fn in_english() -> std::sync::MutexGuard<'static, ()> {
    let guard = i18n_test_guard();
    init(None);
    let _ = apply_language_preference(LanguagePreference::En);
    guard
}

#[test]
fn a_service_error_is_wrapped_in_the_shared_line() {
    let _english = in_english();
    let text = user_message_from(GitHubError::NotFound);
    assert!(text.contains("Not found"), "got {text:?}");
    assert!(text.starts_with("Error:"), "got {text:?}");
}

#[test]
fn an_io_error_reads_as_a_sentence_not_a_debug_dump() {
    let _english = in_english();
    let text = user_message_from(std::io::Error::other("disk on fire"));
    assert!(text.contains("disk on fire"), "got {text:?}");
    assert!(
        !text.contains("Custom {"),
        "must not leak Debug output: {text:?}"
    );
}

#[test]
fn a_plain_string_passes_through_the_same_shape() {
    let _english = in_english();
    assert_eq!(
        user_message_from("the token has expired"),
        "Error: the token has expired"
    );
}
