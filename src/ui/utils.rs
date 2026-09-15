//! Genuinely cross-cutting GTK helpers: each of these is used by two or more
//! features, which is what keeps them here rather than beside one owner.
//! Helpers with a single owning feature moved next to it in Phase 7.

pub mod duration;
pub mod widget_data;

mod icons;
pub use icons::favorite_icon_name;

mod a11y;
pub use a11y::describe_control;

mod typography;
pub use typography::section_heading;

mod favorites;
pub use favorites::apply_favorite_result;
