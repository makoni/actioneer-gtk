use super::helpers::{
    WorkflowRowContext, WorkflowRowSettings, create_workflow_expander_row,
    current_job_context_run_ids, set_expander_active, update_workflow_row_header,
};
use super::{RepoDetailPane, WorkflowListContext};
use crate::api::models::{Workflow, WorkflowRun};
use crate::i18n::tr;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tracing::{debug, info, warn};

impl RepoDetailPane {
    pub(super) fn workflow_list_context(&self) -> WorkflowListContext {
        WorkflowListContext {
            store: self.workflow_store.clone(),
            client: self.client.clone(),
            owner: self.repo.owner.login.clone(),
            repo: self.repo.name.clone(),
            repo_model: self.repo.clone(),
            parent_window: self.parent.clone(),
            toast_overlay: self.toast_overlay.clone(),
            job_contexts: self.job_contexts.clone(),
            run_badge_summaries: self.run_badge_summaries.clone(),
            workflows_with_active_runs: self.workflows_with_active_runs.clone(),
            workflows_last_loaded: self.workflows_last_loaded.clone(),
            workflows_loading_runs: self.workflows_loading_runs.clone(),
            run_digests: self.run_digests.clone(),
            notification_manager: self.notification_manager.clone(),
            preferences_manager: self.preferences_manager.clone(),
            run_filters: self.run_filters.clone(),
            run_load_service: self.run_load_service.clone(),
            expand_first_workflow: self.expand_first_workflow_on_load.clone(),
            header: self.header.clone(),
        }
    }
}

fn expand_first_expander(widget: &gtk::Widget) -> bool {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
        if !expander.is_expanded() {
            expander.set_expanded(true);
        }
        return true;
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        if expand_first_expander(&current) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

pub(super) fn update_workflows_list(context: &WorkflowListContext, workflows: &[Workflow]) {
    let store = context.store.clone();
    let render_start = Instant::now();

    let mut expanded_ids = HashSet::new();
    for row in collect_workflow_rows(&store) {
        capture_expanded_workflows(&row, &mut expanded_ids);
    }

    let mut expanded_runs_by_workflow: HashMap<i64, HashSet<i64>> = HashMap::new();
    for row in collect_workflow_rows(&store) {
        collect_expanded_runs(&row, &mut expanded_runs_by_workflow);
    }

    info!("💾 Preserved {} expanded workflow(s)", expanded_ids.len());

    let visible_workflows: HashSet<i64> = workflows.iter().map(|w| w.id).collect();
    {
        let mut contexts = context.job_contexts.borrow_mut();
        contexts.retain(|_, ctx| visible_workflows.contains(&ctx.workflow_id()));
    }
    {
        let mut active = context.workflows_with_active_runs.lock();
        active.retain(|id| visible_workflows.contains(id));
    }
    {
        let mut digests = context.run_digests.lock();
        digests.retain(|workflow_id, _| visible_workflows.contains(workflow_id));
    }

    store.remove_all();

    if workflows.is_empty() {
        let placeholder = gtk::Label::new(Some(tr("No workflows found.").as_str()));
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(24);
        placeholder.set_margin_bottom(24);
        placeholder.set_margin_start(12);
        placeholder.set_margin_end(12);

        let placeholder_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        placeholder_box.add_css_class("workflow-item");
        placeholder_box.add_css_class("workflow-item-first");
        placeholder_box.append(&placeholder);

        store.append(&placeholder_box);
        context.header.set_workflow_count(0);
        context.header.note_refreshed();
        context.header.retain_workflows(&visible_workflows);
        return;
    }

    let base_row_context = WorkflowRowContext {
        client: context.client.clone(),
        owner: context.owner.clone(),
        repo: context.repo.clone(),
        repo_model: context.repo_model.clone(),
        parent_window: context.parent_window.clone(),
        toast_overlay: context.toast_overlay.clone(),
        job_contexts: context.job_contexts.clone(),
        run_badge_summaries: context.run_badge_summaries.clone(),
        workflows_with_active_runs: context.workflows_with_active_runs.clone(),
        workflows_last_loaded: context.workflows_last_loaded.clone(),
        workflows_loading_runs: context.workflows_loading_runs.clone(),
        run_digests: context.run_digests.clone(),
        notification_manager: context.notification_manager.clone(),
        preferences_manager: context.preferences_manager.clone(),
        run_filters: context.run_filters.clone(),
        run_load_service: context.run_load_service.clone(),
        header: context.header.clone(),
    };

    for (index, workflow) in workflows.iter().enumerate() {
        let should_expand = expanded_ids.contains(&workflow.id);
        let preserved_run_ids = expanded_runs_by_workflow
            .get(&workflow.id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_else(|| current_job_context_run_ids(&context.job_contexts, workflow.id));

        let row_context = base_row_context.clone();
        let settings = WorkflowRowSettings {
            should_expand,
            initial_expanded_run_ids: preserved_run_ids,
            is_first: index == 0,
        };

        let expander_row = create_workflow_expander_row(workflow, &row_context, settings);
        store.append(&expander_row);
    }

    context.header.set_workflow_count(workflows.len());
    context.header.note_refreshed();
    context.header.retain_workflows(&visible_workflows);
    fetch_latest_runs_summary(context);

    if context.expand_first_workflow.get() {
        for idx in 0..store.n_items() {
            if let Some(obj) = store.item(idx)
                && let Ok(widget) = obj.downcast::<gtk::Widget>()
                && expand_first_expander(&widget)
            {
                break;
            }
        }
        context.expand_first_workflow.set(false);
    }

    let elapsed = render_start.elapsed();
    if workflows.len() >= 50 {
        debug!(
            workflow_count = workflows.len(),
            duration_ms = elapsed.as_millis(),
            "Rebuilt workflow detail list"
        );
    }
}

pub(super) fn workflows_differ(a: &[Workflow], b: &[Workflow]) -> bool {
    if a.len() != b.len() {
        return true;
    }

    let a_ids: HashSet<_> = a.iter().map(|w| w.id).collect();
    let b_ids: HashSet<_> = b.iter().map(|w| w.id).collect();

    a_ids != b_ids
}

/// Fetches the repo-wide run list once and records the latest run per workflow
/// so collapsed rows can show their status dot and meta line. Runs on the Tokio
/// runtime; UI updates happen on the GLib main context.
pub(super) fn fetch_latest_runs_summary(context: &WorkflowListContext) {
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let header = context.header.clone();
    let store = context.store.clone();

    let (sender, receiver) = glib::MainContext::default()
        .channel::<Result<Vec<WorkflowRun>, String>>(glib::Priority::default());

    receiver.attach(None, move |result| {
        let runs = match result {
            Ok(runs) => runs,
            Err(error) => {
                // Keep whatever the rows already show: blanking them would relabel
                // the entire repository as "No runs yet" on a transient API error.
                warn!("Failed to load repository run summary: {}", error);
                return glib::ControlFlow::Break;
            }
        };

        // Repo-wide runs arrive newest-first; keep the first hit per workflow.
        let mut seen = HashSet::new();
        let mut latest_runs: Vec<(i64, WorkflowRun)> = Vec::new();
        for run in runs {
            let Some(workflow_id) = run.workflow_id else {
                continue;
            };
            if seen.insert(workflow_id) {
                latest_runs.push((workflow_id, run));
            }
        }

        for (workflow_id, run) in latest_runs {
            header.record_latest_run(workflow_id, Some(&run));
        }

        // Refresh every visible row in place.
        for row in collect_workflow_rows(&store) {
            super::workflow_refresh::visit_workflow_expanders(
                &row,
                &mut |expander, workflow_id, was_active| {
                    let Some(run_list) = super::workflow_refresh::run_list_for_expander(expander)
                    else {
                        return;
                    };
                    // The summary only carries the newest run per workflow and is
                    // capped at one page of repository runs, so a workflow missing
                    // from it means "no data here", not "never ran" — leave the row
                    // untouched rather than downgrading it.
                    let Some(latest) = header.latest_run(workflow_id) else {
                        return;
                    };
                    let summary = run_list.job_summaries().borrow().get(&latest.id).cloned();
                    if let Some(row_header) = run_list.row_header() {
                        update_workflow_row_header(&row_header, Some(&latest), summary.as_ref());
                    }
                    // A per-workflow load may have seen an older run still running;
                    // the newest run alone must not clear that flag, or background
                    // polling would stop before that run finishes.
                    set_expander_active(expander, was_active || latest.is_active());
                },
            );
        }

        glib::ControlFlow::Break
    });

    crate::runtime_handle().spawn(async move {
        let client_guard = client.lock().clone();
        let runs = client_guard
            .list_repository_runs(&owner, &repo)
            .await
            .map_err(|error| error.to_string());
        let _ = sender.send(runs);
    });
}

pub(super) fn collect_workflow_rows(store: &gio::ListStore) -> Vec<gtk::Widget> {
    (0..store.n_items())
        .filter_map(|idx| store.item(idx))
        .filter_map(|obj| obj.downcast::<gtk::Widget>().ok())
        .collect()
}

fn capture_expanded_workflows(widget: &gtk::Widget, expanded_ids: &mut HashSet<i64>) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && expander.is_expanded()
        && let Some((workflow_id, _)) =
            super::workflow_refresh::parse_expander_widget_name(expander.widget_name().as_str())
    {
        info!("Preserving expansion for workflow ID {}", workflow_id);
        expanded_ids.insert(workflow_id);
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        capture_expanded_workflows(&current, expanded_ids);
        child = current.next_sibling();
    }
}

fn collect_expanded_runs(widget: &gtk::Widget, expanded_runs: &mut HashMap<i64, HashSet<i64>>) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && let Some((workflow_id, _)) =
            super::workflow_refresh::parse_expander_widget_name(expander.widget_name().as_str())
        && let Some(run_list) = super::workflow_refresh::run_list_for_expander(expander)
    {
        let expanded = run_list.expanded_run_ids();
        if !expanded.is_empty() {
            expanded_runs
                .entry(workflow_id)
                .or_default()
                .extend(expanded);
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_expanded_runs(&current, expanded_runs);
        child = current.next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::workflows_differ;
    use crate::api::models::Workflow;

    fn workflow(id: i64) -> Workflow {
        Workflow {
            id,
            name: format!("wf-{id}"),
            path: format!(".github/workflows/{id}.yml"),
        }
    }

    #[test]
    fn workflows_differ_returns_true_for_length_mismatch() {
        let a = vec![workflow(1)];
        let b = vec![workflow(1), workflow(2)];
        assert!(workflows_differ(&a, &b));
    }

    #[test]
    fn workflows_differ_detects_id_changes() {
        let a = vec![workflow(1), workflow(2)];
        let b = vec![workflow(1), workflow(3)];
        assert!(workflows_differ(&a, &b));
    }

    #[test]
    fn workflows_differ_returns_false_for_identical_sets() {
        let a = vec![workflow(1), workflow(2)];
        let b = vec![workflow(2), workflow(1)];
        assert!(!workflows_differ(&a, &b));
    }
}
