use super::helpers::{LoadRunsParams, current_job_context_run_ids};
use super::workflow_refresh::run_list_for_expander;
use super::{RepoDetailPane, RunFilters};
use crate::preferences::RunFilterPreferences;
use crate::ui::utils::MainContextChannelExt;
use crate::ui::utils::widget_data::{get_data_clone, get_data_copy};
use gtk4::prelude::{Cast, ToggleButtonExt, WidgetExt};
use gtk4::{self as gtk, glib};
use std::collections::HashSet;
use tracing::{debug, info, warn};

#[derive(Copy, Clone)]
enum FilterKind {
    Success,
    Failed,
    Running,
}

impl RepoDetailPane {
    pub(super) fn connect_filter_chips(&self) {
        let chips = self.filter_chips.clone();
        self.attach_filter_chip_handler(&chips.success, FilterKind::Success);
        self.attach_filter_chip_handler(&chips.failed, FilterKind::Failed);
        self.attach_filter_chip_handler(&chips.running, FilterKind::Running);
    }

    pub(super) fn restore_run_filter_preferences(&self) {
        if let Some(manager) = &self.preferences_manager {
            let (sender, receiver) =
                glib::MainContext::default().channel::<RunFilters>(glib::Priority::default());
            let manager = manager.clone();
            crate::runtime_handle().spawn(async move {
                let prefs = manager.get().await;
                let _ = sender.send(RunFilters::from(prefs.run_filters));
            });

            let pane = self.clone();
            receiver.attach(None, move |filters| {
                pane.apply_saved_filters(filters);
                pane.refresh_visible_runs_with_filters();
                glib::ControlFlow::Break
            });
        } else {
            self.apply_saved_filters(RunFilters::default());
            self.refresh_visible_runs_with_filters();
        }
    }

    fn attach_filter_chip_handler(&self, button: &gtk::ToggleButton, kind: FilterKind) {
        let pane = self.clone();
        button.connect_toggled(move |btn| {
            pane.on_filter_chip_toggled(kind, btn.is_active());
        });
    }

    fn on_filter_chip_toggled(&self, kind: FilterKind, active: bool) {
        if self.filter_guard.get() {
            return;
        }

        {
            let mut filters = self.run_filters.lock();
            update_filters(&mut filters, kind, active);
            debug!(
                success = filters.include_success,
                failed = filters.include_failed,
                running = filters.include_running,
                "run filters updated via chip toggle"
            );
        }

        self.persist_run_filters();
        self.refresh_visible_runs_with_filters();
    }

    fn persist_run_filters(&self) {
        if let Some(manager) = &self.preferences_manager {
            let manager = manager.clone();
            let filters: RunFilterPreferences = self.run_filters.lock().clone().into();
            crate::runtime_handle().spawn(async move {
                if let Err(err) = manager.set_run_filters(filters).await {
                    warn!("Failed to persist run filters: {}", err);
                }
            });
        }
    }

    fn apply_saved_filters(&self, filters: RunFilters) {
        let normalized = filters.clone();

        {
            let mut guard = self.run_filters.lock();
            *guard = normalized.clone();
        }

        self.filter_guard.set(true);
        self.filter_chips
            .success
            .set_active(normalized.include_success);
        self.filter_chips
            .failed
            .set_active(normalized.include_failed);
        self.filter_chips
            .running
            .set_active(normalized.include_running);
        self.filter_guard.set(false);
    }

    fn refresh_visible_runs_with_filters(&self) {
        let context = self.workflow_list_context();
        let run_filters_arc = self.run_filters.clone();
        let filters_snapshot = run_filters_arc.lock().clone();
        let owner = context.owner.clone();
        let repo = context.repo.clone();
        let repo_model = context.repo_model.clone();
        let parent_window = context.parent_window.clone();
        let toast_overlay = context.toast_overlay.clone();
        let workflows_with_active = context.workflows_with_active_runs.clone();
        let job_contexts = context.job_contexts.clone();
        let run_digests = context.run_digests.clone();
        let notification_manager = context.notification_manager.clone();
        let preferences_manager = context.preferences_manager.clone();

        for row in super::workflow_list::collect_workflow_rows(&context.store) {
            visit_expanders(&row, &mut |expander, workflow_id| {
                if let Some(run_list) = run_list_for_expander(expander) {
                    let status_badge = Self::status_badge_for_expander(expander);
                    let mut preserved_runs: Vec<i64> =
                        run_list.expanded_run_ids().into_iter().collect();
                    if preserved_runs.is_empty() {
                        preserved_runs = current_job_context_run_ids(&job_contexts, workflow_id);
                    }
                    let preserved_run_ids: HashSet<i64> = preserved_runs.iter().copied().collect();
                    let re_applied =
                        run_list.reapply_filters(&filters_snapshot, &preserved_run_ids);

                    debug!(
                        workflow_id,
                        expanded = expander.is_expanded(),
                        re_applied,
                        "reapplied run filters for workflow"
                    );

                    if re_applied {
                        info!(workflow_id, "run filters re-applied from cache");
                    } else {
                        info!(workflow_id, "run filters skipped (no cached runs)");
                    }

                    if !expander.is_expanded() {
                        return;
                    }

                    if re_applied {
                        return;
                    }
                    let workflow_label = get_data_clone(expander, "actioneer-workflow-name")
                        .unwrap_or_else(|| {
                            format!("{}/{} • Workflow {}", owner, repo, workflow_id)
                        });

                    context.run_load_service.request(LoadRunsParams {
                        client: context.client.clone(),
                        owner: owner.clone(),
                        repo: repo.clone(),
                        repo_model: repo_model.clone(),
                        workflow_id,
                        workflow_name: workflow_label,
                        run_list,
                        parent_window: parent_window.clone(),
                        status_badge,
                        expander: expander.clone(),
                        toast_overlay: toast_overlay.clone(),
                        job_contexts: job_contexts.clone(),
                        expanded_run_ids: preserved_runs,
                        workflows_with_active: workflows_with_active.clone(),
                        workflows_last_loaded: context.workflows_last_loaded.clone(),
                        workflows_loading: context.workflows_loading_runs.clone(),
                        background: false,
                        run_digests: run_digests.clone(),
                        notification_manager: notification_manager.clone(),
                        preferences_manager: preferences_manager.clone(),
                        run_filters: run_filters_arc.clone(),
                    });
                } else {
                    debug!(
                        workflow_id,
                        "no run list attached to expander; skipping reapply"
                    );
                }
            });
        }
    }
}

fn visit_expanders<F: FnMut(&gtk::Expander, i64)>(widget: &gtk::Widget, f: &mut F) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && let Some(workflow_id) = get_data_copy(expander, "actioneer-workflow-id")
    {
        f(expander, workflow_id);
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        visit_expanders(&current, f);
        child = current.next_sibling();
    }
}

fn update_filters(filters: &mut RunFilters, kind: FilterKind, active: bool) {
    match kind {
        FilterKind::Success => filters.include_success = active,
        FilterKind::Failed => filters.include_failed = active,
        FilterKind::Running => filters.include_running = active,
    }
}

#[cfg(test)]
mod tests {
    use super::{FilterKind, update_filters};
    use crate::ui::detail_view::RunFilters;

    #[test]
    fn update_filters_sets_success_field() {
        let mut filters = RunFilters::default();
        update_filters(&mut filters, FilterKind::Success, false);
        assert!(!filters.include_success);
        assert!(filters.include_failed);
        assert!(filters.include_running);
    }

    #[test]
    fn update_filters_sets_failed_field() {
        let mut filters = RunFilters::default();
        update_filters(&mut filters, FilterKind::Failed, false);
        assert!(filters.include_success);
        assert!(!filters.include_failed);
    }

    #[test]
    fn update_filters_sets_running_field() {
        let mut filters = RunFilters::default();
        update_filters(&mut filters, FilterKind::Running, false);
        assert!(filters.include_success);
        assert!(filters.include_failed);
        assert!(!filters.include_running);
    }
}
