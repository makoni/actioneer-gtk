use super::filter_controls::FilterChips;
use super::helpers::runs::filters::{RunStatusFilterKind, classify_run_status};
use crate::api::models::WorkflowRun;
use crate::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

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
    last_refresh: Rc<RefCell<Option<Instant>>>,
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
        *self.last_refresh.borrow_mut() = Some(Instant::now());
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
        ));
    }

    /// Handle for the periodic "updated …" refresh. It holds the label weakly so
    /// the timer stops once the pane is gone instead of pinning it alive.
    pub(crate) fn subtitle_ticker(&self) -> SubtitleTicker {
        SubtitleTicker {
            subtitle_label: self.subtitle_label.downgrade(),
            workflow_count: self.workflow_count.clone(),
            last_refresh: self.last_refresh.clone(),
        }
    }
}

/// Weak-referencing view of the header subline, used by the 30s ticker.
pub(crate) struct SubtitleTicker {
    subtitle_label: glib::WeakRef<gtk::Label>,
    workflow_count: Rc<Cell<usize>>,
    last_refresh: Rc<RefCell<Option<Instant>>>,
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
        ));
        true
    }
}

fn subtitle_text(count: usize, last_refresh: Option<Instant>) -> String {
    let count_text = if count == 1 {
        tr("1 workflow")
    } else {
        tr("{count} workflows").replace("{count}", count.to_string().as_str())
    };

    let updated_text = match last_refresh {
        Some(instant) => {
            let elapsed = instant.elapsed().as_secs();
            let when = if elapsed < 60 {
                tr("Just now")
            } else if elapsed < 3600 {
                tr("{count}m ago").replace("{count}", (elapsed / 60).to_string().as_str())
            } else {
                tr("{count}h ago").replace("{count}", (elapsed / 3600).to_string().as_str())
            };
            tr("Updated {when}").replace("{when}", when.as_str())
        }
        None => tr("Not updated yet"),
    };

    format!("{count_text} · {updated_text}")
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
        assert!(subtitle_text(1, None).starts_with(&tr("1 workflow")));
        assert!(
            subtitle_text(2, None).starts_with(&tr("{count} workflows").replace("{count}", "2"))
        );
    }
}
