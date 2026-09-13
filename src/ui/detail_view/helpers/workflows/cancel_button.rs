//! Cancelling a workflow's latest running run.
//!
//! Split out of the row builder in `workflows.rs`, alongside its sibling
//! `logs_button.rs`: same shape, same shared header state, opposite action.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn connect_cancel_button(
    cancel_btn: &gtk::Button,
    context: &WorkflowRowContext,
    workflow: &Workflow,
    client: &Arc<Mutex<GitHubGateway>>,
    owner: &str,
    repo: &str,
    repo_model: &Repo,
    parent_window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
) {
    let owner = owner.to_string();
    let repo = repo.to_string();
    let client = client.clone();
    let repo_model = repo_model.clone();
    let parent_window = parent_window.clone();
    let toast_overlay = toast_overlay.clone();
    // Cancel the workflow's latest (running) run.
    {
        let header_state = context.header.clone();
        let workflow_id_for_cancel = workflow.id;
        let client_for_cancel = client.clone();
        let owner_for_cancel = owner.clone();
        let repo_for_cancel = repo.clone();
        let repo_model_for_cancel = repo_model.clone();
        let parent_window_for_cancel = parent_window.clone();
        let toast_overlay_for_cancel = toast_overlay.clone();
        cancel_btn.connect_clicked(move |btn| {
            let Some(run) = header_state.latest_run(workflow_id_for_cancel) else {
                return;
            };
            if !run.is_cancellable() {
                return;
            }
            let action_context = RunActionContext {
                client: client_for_cancel.clone(),
                owner: owner_for_cancel.clone(),
                repo: repo_for_cancel.clone(),
                repo_model: repo_model_for_cancel.clone(),
                parent_window: parent_window_for_cancel.clone(),
                toast_overlay: toast_overlay_for_cancel.clone(),
            };
            confirm_and_cancel_run(&run, &action_context, btn);
        });
    }
}
