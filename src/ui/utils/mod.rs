pub mod duration;
pub mod layout;
pub mod rate_limit;
pub mod source;
pub mod widget_data;

pub use layout::{create_detail_clamp, create_sidebar_clamp};
pub use rate_limit::update_rate_limit_label;
pub use source::try_remove_source;

mod icons;
pub use icons::favorite_icon_name;

mod a11y;
pub use a11y::describe_control;

mod typography;
pub use typography::section_heading;

mod favorites;
pub use favorites::apply_favorite_result;
