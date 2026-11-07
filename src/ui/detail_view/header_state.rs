use super::filter_controls::FilterChips;
use crate::domain::filters::{RunStatusFilterKind, classify_run_status};
use crate::kernel::i18n::tr;
use crate::services::api::models::WorkflowRun;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::SystemTime;

/// Shared state backing the repository pane header: the "N workflows · updated
/// …" subline, the per-status counters in the segmented filter, the auto-refresh
/// footer note, and the latest known run per workflow (used for status dots and
/// row meta lines without expanding each workflow).
#[derive(Clone)]
pub(crate) struct DetailHeaderState {
    subtitle_label: gtk::Label,
    footer_label: gtk::Label,
    filter_chips: FilterChips,
    latest_runs: Rc<RefCell<HashMap<i64, WorkflowRun>>>,
    workflow_count: Rc<Cell<usize>>,
    /// Wall-clock time, not `Instant`: the data may come from the cache, fetched
    /// before the app started, and a monotonic clock also stops while the
    /// machine sleeps.
    last_refresh: Rc<RefCell<Option<SystemTime>>>,
    /// GitHub could not be reached on the last attempt; the subline says so,
    /// since the age alone does not tell cached data from live.
    offline: Rc<Cell<bool>>,
}

impl DetailHeaderState {
    pub(crate) fn new(
        subtitle_label: gtk::Label,
        footer_label: gtk::Label,
        filter_chips: FilterChips,
    ) -> Self {
        let state = Self {
            subtitle_label,
            footer_label,
            filter_chips,
            latest_runs: Rc::new(RefCell::new(HashMap::new())),
            workflow_count: Rc::new(Cell::new(0)),
            last_refresh: Rc::new(RefCell::new(None)),
            offline: Rc::new(Cell::new(false)),
        };
        state.render_subtitle();
        state
    }

    pub(crate) fn set_workflow_count(&self, count: usize) {
        self.workflow_count.set(count);
        self.render_subtitle();
    }

    /// Marks the workflow list as freshly loaded and re-renders the subline.
    pub(crate) fn note_refreshed(&self) {
        self.offline.set(false);
        self.note_fetched_at(SystemTime::now());
    }

    /// Marks the shown data as unrefreshable for now, or clears that.
    pub(crate) fn set_offline(&self, offline: bool) {
        if self.offline.replace(offline) != offline {
            self.render_subtitle();
        }
    }

    /// Marks the shown data as fetched at `fetched_at` — in the past, for a
    /// paint from the cache — and re-renders the subline.
    pub(crate) fn note_fetched_at(&self, fetched_at: SystemTime) {
        *self.last_refresh.borrow_mut() = Some(fetched_at);
        self.render_subtitle();
    }

    /// Records the latest known run for a workflow (`None` = no runs) and
    /// recomputes the header counters.
    pub(crate) fn record_latest_run(&self, workflow_id: i64, run: Option<&WorkflowRun>) {
        {
            let mut latest = self.latest_runs.borrow_mut();
            match run {
                Some(run) => {
                    latest.insert(workflow_id, run.clone());
                }
                None => {
                    latest.remove(&workflow_id);
                }
            }
        }
        self.refresh_counts();
    }

    /// Latest known run for a workflow, if any was observed so far.
    pub(crate) fn latest_run(&self, workflow_id: i64) -> Option<WorkflowRun> {
        self.latest_runs.borrow().get(&workflow_id).cloned()
    }

    /// Drops entries for workflows that no longer exist.
    pub(crate) fn retain_workflows(&self, ids: &std::collections::HashSet<i64>) {
        self.latest_runs
            .borrow_mut()
            .retain(|id, _| ids.contains(id));
        self.refresh_counts();
    }

    pub(crate) fn set_refresh_interval(&self, seconds: u64) {
        if seconds == 0 {
            self.footer_label
                .set_text(tr("Auto-refresh is disabled.").as_str());
        } else {
            self.footer_label.set_text(
                tr("Data refreshes automatically every {seconds} seconds.")
                    .replace("{seconds}", seconds.to_string().as_str())
                    .as_str(),
            );
        }
    }

    pub(crate) fn subtitle_label(&self) -> gtk::Label {
        self.subtitle_label.clone()
    }

    pub(crate) fn footer_label(&self) -> gtk::Label {
        self.footer_label.clone()
    }

    fn refresh_counts(&self) {
        let (mut success, mut running, mut failed) = (0_usize, 0_usize, 0_usize);
        for run in self.latest_runs.borrow().values() {
            match classify_run(run) {
                StatusGroup::Success => success += 1,
                StatusGroup::Running => running += 1,
                StatusGroup::Failed => failed += 1,
            }
        }
        self.filter_chips.set_counts(success, running, failed);
    }

    fn render_subtitle(&self) {
        self.subtitle_label.set_text(&subtitle_text(
            self.workflow_count.get(),
            *self.last_refresh.borrow(),
            self.offline.get(),
            SystemTime::now(),
        ));
    }

    /// Handle for the periodic "updated …" refresh. It holds the label weakly so
    /// the timer stops once the pane is gone instead of pinning it alive.
    pub(crate) fn subtitle_ticker(&self) -> SubtitleTicker {
        SubtitleTicker {
            subtitle_label: self.subtitle_label.downgrade(),
            workflow_count: self.workflow_count.clone(),
            last_refresh: self.last_refresh.clone(),
            offline: self.offline.clone(),
        }
    }
}

/// Weak-referencing view of the header subline, used by the 30s ticker.
pub(crate) struct SubtitleTicker {
    subtitle_label: glib::WeakRef<gtk::Label>,
    workflow_count: Rc<Cell<usize>>,
    last_refresh: Rc<RefCell<Option<SystemTime>>>,
    offline: Rc<Cell<bool>>,
}

impl SubtitleTicker {
    /// Re-renders the subline. Returns `false` once the label has been dropped,
    /// which is the signal for the caller to stop ticking.
    pub(crate) fn tick(&self) -> bool {
        let Some(label) = self.subtitle_label.upgrade() else {
            return false;
        };
        label.set_text(&subtitle_text(
            self.workflow_count.get(),
            *self.last_refresh.borrow(),
            self.offline.get(),
            SystemTime::now(),
        ));
        true
    }
}

fn subtitle_text(
    count: usize,
    last_refresh: Option<SystemTime>,
    offline: bool,
    now: SystemTime,
) -> String {
    let count_text = if count == 1 {
        tr("1 workflow")
    } else {
        tr("{count} workflows").replace("{count}", count.to_string().as_str())
    };

    let updated_text = match last_refresh {
        Some(fetched_at) => {
            // A clock set back makes the stamp look like the future; call it
            // fresh rather than fail.
            let elapsed = now
                .duration_since(fetched_at)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0);
            let when = if elapsed < 60 {
                tr("Just now")
            } else if elapsed < 3600 {
                tr("{count}m ago").replace("{count}", (elapsed / 60).to_string().as_str())
            } else if elapsed < 86_400 {
                tr("{count}h ago").replace("{count}", (elapsed / 3600).to_string().as_str())
            } else {
                tr("{count}d ago").replace("{count}", (elapsed / 86_400).to_string().as_str())
            };
            tr("Updated {when}").replace("{when}", when.as_str())
        }
        None => tr("Not updated yet"),
    };

    if offline {
        format!("{count_text} · {} · {updated_text}", tr("Offline"))
    } else {
        format!("{count_text} · {updated_text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusGroup {
    Success,
    Running,
    Failed,
}

/// Buckets a workflow's latest run into the header counter groups.
///
/// This delegates to the same classifier the status chips actually filter on,
/// so the counts and the filtering can never drift apart: every run lands in
/// exactly one group (anything finished that is not a success counts as
/// failed — cancelled, timed out, startup failure and friends included).
pub(crate) fn classify_run(run: &WorkflowRun) -> StatusGroup {
    match classify_run_status(run) {
        RunStatusFilterKind::Success => StatusGroup::Success,
        RunStatusFilterKind::Running => StatusGroup::Running,
        RunStatusFilterKind::Failed => StatusGroup::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::i18n::{i18n_test_guard, init};
    use std::time::Duration;

    fn run_stub(status: Option<&str>, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: Some(1),
            workflow_id: None,
            name: Some("CI".into()),
            display_title: None,
            head_branch: Some("main".into()),
            head_commit: None,
            status: status.map(str::to_string),
            conclusion: conclusion.map(str::to_string),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: None,
            actor: None,
            triggering_actor: None,
        }
    }

    #[test]
    fn classify_run_buckets_statuses() {
        assert_eq!(
            classify_run(&run_stub(Some("completed"), Some("success"))),
            StatusGroup::Success
        );
        assert_eq!(
            classify_run(&run_stub(Some("completed"), Some("failure"))),
            StatusGroup::Failed
        );
        assert_eq!(
            classify_run(&run_stub(Some("in_progress"), None)),
            StatusGroup::Running
        );
        assert_eq!(
            classify_run(&run_stub(Some("queued"), None)),
            StatusGroup::Running
        );
    }

    #[test]
    fn classify_run_counts_non_success_conclusions_as_failed() {
        // These used to fall through to "no group", so the red chip could read 0
        // while unchecking it still hid the run.
        for conclusion in [
            "cancelled",
            "timed_out",
            "startup_failure",
            "action_required",
            "neutral",
            "skipped",
        ] {
            assert_eq!(
                classify_run(&run_stub(Some("completed"), Some(conclusion))),
                StatusGroup::Failed,
                "conclusion {conclusion} should be counted as failed"
            );
        }
    }

    #[test]
    fn subtitle_uses_singular_workflow_form() {
        // `tr()` reads process-global language state that other tests mutate.
        let _guard = i18n_test_guard();
        init(None);

        assert!(subtitle_text(1, None, false, SystemTime::now()).starts_with(&tr("1 workflow")));
        assert!(
            subtitle_text(2, None, false, SystemTime::now())
                .starts_with(&tr("{count} workflows").replace("{count}", "2"))
        );
    }

    #[test]
    fn subtitle_ages_cover_minutes_hours_and_days() {
        let _guard = i18n_test_guard();
        init(None);
        let now = SystemTime::now();
        let ago = |secs: u64| subtitle_text(3, Some(now - Duration::from_secs(secs)), false, now);
        let updated = |when: String| tr("Updated {when}").replace("{when}", &when);

        assert!(ago(10).ends_with(&updated(tr("Just now"))));
        assert!(ago(5 * 60).ends_with(&updated(tr("{count}m ago").replace("{count}", "5"))));
        assert!(ago(3 * 3600).ends_with(&updated(tr("{count}h ago").replace("{count}", "3"))));
        // Cached data can be days old; it must not read "72h ago".
        assert!(ago(3 * 86_400).ends_with(&updated(tr("{count}d ago").replace("{count}", "3"))));
    }

    #[test]
    fn the_subline_says_offline_when_github_cannot_be_reached() {
        let _guard = i18n_test_guard();
        init(None);
        let now = SystemTime::now();
        let text = subtitle_text(2, Some(now - Duration::from_secs(7200)), true, now);
        assert!(text.contains(&tr("Offline")), "{text:?}");
        assert!(text.contains(&tr("{count}h ago").replace("{count}", "2")));
        assert!(!subtitle_text(2, Some(now), false, now).contains(&tr("Offline")));
    }

    #[test]
    fn a_stamp_in_the_future_reads_as_just_now() {
        let _guard = i18n_test_guard();
        init(None);
        let now = SystemTime::now();
        let text = subtitle_text(1, Some(now + Duration::from_secs(600)), false, now);
        assert!(text.ends_with(&tr("Updated {when}").replace("{when}", &tr("Just now"))));
    }
}
