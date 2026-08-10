use super::filter_controls::FilterChips;
use crate::api::models::WorkflowRun;
use crate::i18n::tr;
use gtk4::{self as gtk};
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

    /// Re-renders the relative "updated …" part (called by a periodic timer).
    pub(crate) fn refresh_relative_time(&self) {
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
                Some(StatusGroup::Success) => success += 1,
                Some(StatusGroup::Running) => running += 1,
                Some(StatusGroup::Failed) => failed += 1,
                None => {}
            }
        }
        self.filter_chips.set_counts(success, running, failed);
    }

    fn render_subtitle(&self) {
        let count = self.workflow_count.get();
        let count_text = tr("{count} workflows").replace("{count}", count.to_string().as_str());

        let updated_text = match *self.last_refresh.borrow() {
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

        self.subtitle_label
            .set_text(&format!("{count_text} · {updated_text}"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusGroup {
    Success,
    Running,
    Failed,
}

/// Buckets a workflow's latest run into the header counter groups.
pub(crate) fn classify_run(run: &WorkflowRun) -> Option<StatusGroup> {
    if run.is_active() {
        return Some(StatusGroup::Running);
    }
    match run.conclusion.as_deref() {
        Some("success") => Some(StatusGroup::Success),
        Some("failure") => Some(StatusGroup::Failed),
        _ => None,
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
            Some(StatusGroup::Success)
        );
        assert_eq!(
            classify_run(&run_stub(Some("completed"), Some("failure"))),
            Some(StatusGroup::Failed)
        );
        assert_eq!(
            classify_run(&run_stub(Some("in_progress"), None)),
            Some(StatusGroup::Running)
        );
        assert_eq!(
            classify_run(&run_stub(Some("queued"), None)),
            Some(StatusGroup::Running)
        );
        assert_eq!(
            classify_run(&run_stub(Some("completed"), Some("cancelled"))),
            None
        );
        assert_eq!(classify_run(&run_stub(None, None)), None);
    }
}
