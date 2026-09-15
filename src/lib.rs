//! Actioneer — a GitHub Actions client for Linux (GNOME).
//!
//! The binary in `src/main.rs` is a thin composition root over this library;
//! everything testable lives here.

// Kernel and runtime first: nothing below them may depend on the layers above.
pub mod kernel;
pub mod runtime;
pub mod services;

pub mod demo;
pub mod domain;
pub mod ui;

// Convenience re-exports for the binary and for `tests/`. In-crate code uses
// the explicit `kernel::app::…` path instead.
pub use kernel::app::{APP_ICON_NAME, APP_ID, resolved_app_id, version_string};
