//! Opening the job-logs window for a workflow's latest run.
//!
//! Split out of the row builder in `workflows.rs`. Self-contained: it reads the
//! latest run off the shared header state and hands off to `JobLogsWindow`.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn connect_logs_button(
    logs_btn: &gtk::Button,
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
    // Open the logs of the latest run's most relevant job.
    {
        let header_state = context.header.clone();
        let workflow_id_for_logs = workflow.id;
        let client_for_logs = client.clone();
        let owner_for_logs = owner.clone();
        let repo_for_logs = repo.clone();
        let repo_model_for_logs = repo_model.clone();
        let parent_window_for_logs = parent_window.clone();
        let toast_overlay_for_logs = toast_overlay.clone();
        logs_btn.connect_clicked(move |btn| {
            let Some(run) = header_state.latest_run(workflow_id_for_logs) else {
                toast_overlay_for_logs.add_toast(adw::Toast::new(tr("No runs yet").as_str()));
                return;
            };

            // Guard against a double click opening two log windows.
            btn.set_sensitive(false);
            let btn_for_result = btn.clone();

            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<Result<Vec<crate::services::api::models::Job>, String>>(
                        glib::Priority::default(),
                    );

            let parent_window = parent_window_for_logs.clone();
            let repo_model = repo_model_for_logs.clone();
            let toast_overlay = toast_overlay_for_logs.clone();
            let client_for_window = client_for_logs.clone();
            let run_title = crate::ui::detail_view::helpers::formatting::format_run_title(&run);
            receiver.attach(None, move |result| {
                btn_for_result.set_sensitive(true);
                match result {
                    Ok(jobs) => {
                        // A run has no log of its own — it is the set of its job
                        // logs — so the window lists them all and preselects the
                        // one a reader most likely wants.
                        match crate::ui::job_logs_window::most_relevant_job(&jobs) {
                            Some(selected) => {
                                let logs_window = JobLogsWindow::for_run(
                                    &parent_window,
                                    repo_model.clone(),
                                    run_title.clone(),
                                    jobs,
                                    selected,
                                    client_for_window.clone(),
                                );
                                logs_window.present();
                            }
                            None => {
                                toast_overlay
                                    .add_toast(adw::Toast::new(tr("No jobs found").as_str()));
                            }
                        }
                    }
                    Err(message) => {
                        error!("Failed to load jobs for logs: {}", message);
                        toast_overlay
                            .add_toast(adw::Toast::new(tr("Unable to load jobs").as_str()));
                    }
                }
                glib::ControlFlow::Break
            });

            let client = client_for_logs.clone();
            let owner = owner_for_logs.clone();
            let repo = repo_for_logs.clone();
            crate::runtime::handle().spawn(async move {
                let client_guard = client.lock().clone();
                let result = client_guard
                    .list_jobs(&owner, &repo, run.id)
                    .await
                    .map_err(|err| err.to_string());
                let _ = sender.send(result);
            });
        });
    }
}
