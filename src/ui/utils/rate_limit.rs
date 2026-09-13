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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    fn info(remaining: i64, reset_in_secs: i64) -> RateLimitInfo {
        RateLimitInfo {
            limit: 5000,
            remaining,
            reset: (Utc::now() + chrono::Duration::seconds(reset_in_secs)).timestamp(),
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_missing_rate_limit_renders_a_dash() {
        run_gtk_test("rate_limit_missing", || {
            let label = gtk::Label::new(None);
            update_rate_limit_label(&label, None);
            assert!(
                label.text().contains('\u{2013}'),
                "expected an en dash, got {:?}",
                label.text()
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_healthy_budget_shows_the_counts_without_the_low_marker() {
        run_gtk_test("rate_limit_healthy", || {
            let label = gtk::Label::new(None);
            update_rate_limit_label(&label, Some(info(4987, 3600)));
            let text = label.text().to_string();
            assert!(text.contains("4987/5000"), "got {text:?}");
            assert!(
                !text.contains("(low)"),
                "a healthy budget is not low: {text:?}"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn fewer_than_a_hundred_calls_left_is_marked_low() {
        run_gtk_test("rate_limit_low", || {
            let label = gtk::Label::new(None);
            update_rate_limit_label(&label, Some(info(99, 3600)));
            assert!(
                label.text().to_string().contains("(low)"),
                "99 remaining must be marked low, got {:?}",
                label.text()
            );

            update_rate_limit_label(&label, Some(info(100, 3600)));
            assert!(
                !label.text().to_string().contains("(low)"),
                "100 remaining is the first healthy value, got {:?}",
                label.text()
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_reset_countdown_changes_unit_with_distance() {
        run_gtk_test("rate_limit_units", || {
            let label = gtk::Label::new(None);

            // Assert the unit, not the number: the countdown is computed against
            // `Utc::now()` at render time, so 300s renders as "4m" once a few
            // milliseconds have passed. The unit is what the branch selects.
            let unit = |seconds: i64| {
                update_rate_limit_label(&label, Some(info(1000, seconds)));
                let text = label.text().to_string();
                let inside = text
                    .rsplit_once("in ")
                    .map(|(_, rest)| rest.trim_end_matches(')').to_string())
                    .unwrap_or(text);
                inside.chars().last().expect("a non-empty countdown")
            };

            assert_eq!(unit(30), 's', "under a minute counts seconds");
            assert_eq!(unit(300), 'm', "under an hour counts minutes");
            assert_eq!(unit(7200), 'h', "an hour or more counts hours");
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_reset_in_the_past_reads_as_now() {
        run_gtk_test("rate_limit_past_reset", || {
            let label = gtk::Label::new(None);
            update_rate_limit_label(&label, Some(info(1000, -60)));
            assert!(
                label.text().to_string().contains("now"),
                "a stale reset time must not render a negative countdown, got {:?}",
                label.text()
            );
        });
    }
}
