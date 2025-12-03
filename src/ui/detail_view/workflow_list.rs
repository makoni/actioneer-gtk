use super::helpers::{
    WorkflowRowContext, WorkflowRowSettings, create_workflow_expander_row, take_job_context_run_ids,
};
use super::{RepoDetailPane, WorkflowListContext};
use crate::api::models::Workflow;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::collections::HashSet;
use tracing::info;

impl RepoDetailPane {
    pub(super) fn workflow_list_context(&self) -> WorkflowListContext {
        WorkflowListContext {
            list_box: self.list_box.clone(),
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
    let list_box = context.list_box.clone();

    let mut expanded_ids = HashSet::new();
    let mut child = list_box.first_child();
    while let Some(widget) = child.as_ref() {
        let next_sibling = widget.next_sibling();

        if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>()
            && let Some(row_child) = row.child()
            && let Some(box_widget) = row_child.downcast_ref::<gtk::Box>()
        {
            let mut inner_child = box_widget.first_child();
            while let Some(widget) = inner_child.as_ref() {
                let next = widget.next_sibling();

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

                inner_child = next;
            }
        }
        child = next_sibling;
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

    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if workflows.is_empty() {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);

        let placeholder = gtk::Label::new(Some("No workflows found."));
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(24);
        placeholder.set_margin_bottom(24);
        placeholder.set_margin_start(12);
        placeholder.set_margin_end(12);

        row.set_child(Some(&placeholder));
        list_box.append(&row);
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
        list_box.append(&expander_row);
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
