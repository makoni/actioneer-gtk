use super::helpers::{
    WorkflowRowContext, WorkflowRowSettings, create_workflow_expander_row,
    take_job_context_run_ids, workflow_row_card,
};
use super::{RepoDetailPane, WorkflowListContext};
use crate::api::models::Workflow;
use gtk4::prelude::*;
use gtk4::{self as gtk, gio};
use std::collections::HashSet;
use std::time::Instant;
use tracing::{debug, info};

impl RepoDetailPane {
    pub(super) fn workflow_list_context(&self) -> WorkflowListContext {
        WorkflowListContext {
            store: self.workflow_store.clone(),
            client: self.client.clone(),
            owner: self.repo.owner.login.clone(),
            repo: self.repo.name.clone(),
            repo_model: self.repo.clone(),
            parent_window: self.parent.clone(),
            cache: self.cache.clone(),
            toast_overlay: self.toast_overlay.clone(),
            job_contexts: self.job_contexts.clone(),
            workflows_with_active_runs: self.workflows_with_active_runs.clone(),
            run_digests: self.run_digests.clone(),
            notification_manager: self.notification_manager.clone(),
            preferences_manager: self.preferences_manager.clone(),
            run_filters: self.run_filters.clone(),
        }
    }
}

pub(super) fn update_workflows_list(context: &WorkflowListContext, workflows: &[Workflow]) {
    let store = context.store.clone();
    let render_start = Instant::now();

    let mut expanded_ids = HashSet::new();
    for row in collect_workflow_rows(&store) {
        if let Some(row_child) = row.child() {
            capture_expanded_workflows(&row_child, &mut expanded_ids);
        }
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
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        row.set_can_focus(false);
        row.add_css_class("hoverless-row");
        row.add_css_class("workflow-row");

        let placeholder = gtk::Label::new(Some("No workflows found."));
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(24);
        placeholder.set_margin_bottom(24);
        placeholder.set_margin_start(12);
        placeholder.set_margin_end(12);

        let placeholder_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        placeholder_box.append(&placeholder);

        let card = workflow_row_card(&placeholder_box);
        row.set_child(Some(&card));
        store.append(&row);
        return;
    }

    let base_row_context = WorkflowRowContext {
        client: context.client.clone(),
        owner: context.owner.clone(),
        repo: context.repo.clone(),
        repo_model: context.repo_model.clone(),
        parent_window: context.parent_window.clone(),
        cache: context.cache.clone(),
        toast_overlay: context.toast_overlay.clone(),
        job_contexts: context.job_contexts.clone(),
        workflows_with_active_runs: context.workflows_with_active_runs.clone(),
        run_digests: context.run_digests.clone(),
        notification_manager: context.notification_manager.clone(),
        preferences_manager: context.preferences_manager.clone(),
        run_filters: context.run_filters.clone(),
    };

    for workflow in workflows {
        let should_expand = expanded_ids.contains(&workflow.id);
        let preserved_run_ids = take_job_context_run_ids(&context.job_contexts, workflow.id);

        let row_context = base_row_context.clone();
        let settings = WorkflowRowSettings {
            should_expand,
            initial_expanded_run_ids: preserved_run_ids,
        };

        let expander_row = create_workflow_expander_row(workflow, &row_context, settings);
        store.append(&expander_row);
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

pub(super) fn collect_workflow_rows(store: &gio::ListStore) -> Vec<gtk::ListBoxRow> {
    (0..store.n_items())
        .filter_map(|idx| store.item(idx))
        .filter_map(|obj| obj.downcast::<gtk::ListBoxRow>().ok())
        .collect()
}

fn capture_expanded_workflows(widget: &gtk::Widget, expanded_ids: &mut HashSet<i64>) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && expander.is_expanded()
    {
        let name = expander.widget_name();
        if let Some(id) = name
            .as_str()
            .strip_prefix("workflow_")
            .and_then(|id_str| id_str.parse::<i64>().ok())
        {
            info!("Preserving expansion for workflow ID {}", id);
            expanded_ids.insert(id);
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        capture_expanded_workflows(&current, expanded_ids);
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
