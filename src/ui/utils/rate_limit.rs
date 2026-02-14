/// Utility functions for rate limit display
use crate::api::models::RateLimitInfo;
use crate::i18n::tr;
use chrono::{DateTime, Utc};
use gtk4::{self as gtk};

/// Update rate limit label with current info
pub fn update_rate_limit_label(label: &gtk::Label, info: Option<RateLimitInfo>) {
    if let Some(info) = info {
        let reset_time = DateTime::from_timestamp(info.reset, 0).unwrap_or_else(Utc::now);

        let now = Utc::now();
        let duration = reset_time.signed_duration_since(now);

        let time_str = if duration.num_seconds() < 0 {
            tr("now")
        } else if duration.num_minutes() < 1 {
            format!("{}s", duration.num_seconds())
        } else if duration.num_hours() < 1 {
            format!("{}m", duration.num_minutes())
        } else {
            format!("{}h", duration.num_hours())
        };

        let prefix = if info.is_low() {
            tr("Rate limit (low)")
        } else {
            tr("Rate limit")
        };

        label.set_text(
            tr("{prefix}: {remaining}/{limit} (resets in {time})")
                .replace("{prefix}", prefix.as_str())
                .replace("{remaining}", info.remaining.to_string().as_str())
                .replace("{limit}", info.limit.to_string().as_str())
                .replace("{time}", time_str.as_str())
                .as_str(),
        );
    } else {
        label.set_text(tr("Rate limit: –").as_str());
    }
}
