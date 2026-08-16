use super::super::formatting::format_run_title;
use crate::api::GitHubClient;
use crate::api::models::WorkflowRun;
use crate::i18n::tr;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::error;

#[derive(Clone)]
pub(crate) struct RunActionContext {
    pub(crate) client: Arc<Mutex<GitHubClient>>,
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) parent_window: adw::ApplicationWindow,
    pub(crate) toast_overlay: adw::ToastOverlay,
}

/// Builds a Yes/No confirmation alert. The confirming action uses the `confirm`
/// response id (styled destructive or suggested); `cancel` is the safe
/// default/close response.
fn confirm_dialog(heading: &str, body: &str, destructive: bool) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(Some(heading), Some(body));
    dialog.add_response("cancel", tr("No").as_str());
    dialog.add_response("confirm", tr("Yes").as_str());
    dialog.set_response_appearance(
        "confirm",
        if destructive {
            adw::ResponseAppearance::Destructive
        } else {
            adw::ResponseAppearance::Suggested
        },
    );
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog
}

pub(super) fn create_actions_box(run: &WorkflowRun, context: &RunActionContext) -> gtk::Box {
    let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    actions_box.set_valign(gtk::Align::Start);
    actions_box.set_halign(gtk::Align::End);
    actions_box.set_margin_top(1);

    if run.is_rerunnable() {
        actions_box.append(&create_rerun_button(run, context));
    }

    if run.has_failed_jobs() {
        actions_box.append(&create_rerun_failed_button(run, context));
    }

    if run.is_cancellable() {
        actions_box.append(&create_cancel_button(run, context));
    }

    if let Some(url) = run.html_url.as_ref() {
        actions_box.append(&create_open_button(url));
    }

    actions_box
}

fn create_open_button(url: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name("adw-external-link-symbolic");
    crate::ui::utils::describe_control(&button, tr("Open in GitHub").as_str());
    button.add_css_class("row-action-btn");
    button.set_focus_on_click(false);

    let url = url.to_string();
    button.connect_clicked(move |_| {
        if let Err(err) = open::that(&url) {
            error!("Failed to open URL: {}", err);
        }
    });

    button
}

fn create_rerun_button(run: &WorkflowRun, context: &RunActionContext) -> gtk::Button {
    let button = gtk::Button::from_icon_name("view-refresh-symbolic");
    crate::ui::utils::describe_control(&button, tr("Re-run workflow").as_str());
    button.add_css_class("row-action-btn");
    button.set_focus_on_click(false);

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let run_id = run.id;
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let run_title = format_run_title(run);

    button.connect_clicked(move |btn| {
        let dialog = confirm_dialog(
            tr("Re-run Workflow").as_str(),
            tr("Do you want to re-run \"{run}\"?")
                .replace("{run}", run_title.as_str())
                .as_str(),
            false,
        );

        let btn_clone = btn.clone();
        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        dialog.connect_response(None, move |_dialog, response| {
            if response != "confirm" {
                return;
            }

            btn_clone.set_sensitive(false);

            let client = client.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let toast_overlay = toast_overlay.clone();
            let run_title = run_title.clone();

            let (sender, receiver) =
                glib::MainContext::default().channel::<bool>(glib::Priority::default());

            receiver.attach(None, move |success| {
                let toast = if success {
                    adw::Toast::new(
                        tr("✓ Re-running '{run}'")
                            .replace("{run}", run_title.as_str())
                            .as_str(),
                    )
                } else {
                    adw::Toast::new(tr("✗ Failed to re-run workflow").as_str())
                };
                toast.set_timeout(if success { 3 } else { 5 });
                toast_overlay.add_toast(toast);
                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_guard = client.lock().clone();
                let rerun_result = client_guard.rerun_workflow(&owner, &repo, run_id).await;
                if let Err(err) = rerun_result {
                    error!("Failed to re-run workflow: {}", err);
                    let _ = sender.send(false);
                } else {
                    let _ = sender.send(true);
                }
            });
        });

        dialog.present(Some(&parent_window));
    });

    button
}

fn create_rerun_failed_button(run: &WorkflowRun, context: &RunActionContext) -> gtk::Button {
    let button = gtk::Button::from_icon_name("system-reboot-symbolic");
    crate::ui::utils::describe_control(&button, tr("Re-run failed jobs").as_str());
    button.add_css_class("row-action-btn");
    button.set_focus_on_click(false);

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let run_id = run.id;
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let run_title = format_run_title(run);

    button.connect_clicked(move |btn| {
        let dialog = confirm_dialog(
            tr("Re-run Failed Jobs").as_str(),
            tr("Do you want to re-run all failed jobs in \"{run}\"?")
                .replace("{run}", run_title.as_str())
                .as_str(),
            false,
        );

        let btn_clone = btn.clone();
        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        dialog.connect_response(None, move |_dialog, response| {
            if response != "confirm" {
                return;
            }

            btn_clone.set_sensitive(false);

            let client = client.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let toast_overlay = toast_overlay.clone();
            let run_title = run_title.clone();

            let (sender, receiver) =
                glib::MainContext::default().channel::<bool>(glib::Priority::default());

            receiver.attach(None, move |success| {
                let message = if success {
                    tr("✓ Re-running failed jobs for '{run}'").replace("{run}", run_title.as_str())
                } else {
                    tr("✗ Failed to re-run failed jobs")
                };
                let toast = adw::Toast::new(&message);
                toast.set_timeout(if success { 3 } else { 5 });
                toast_overlay.add_toast(toast);
                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_guard = client.lock().clone();
                let rerun_result = client_guard.rerun_failed_jobs(&owner, &repo, run_id).await;
                if let Err(err) = rerun_result {
                    error!("Failed to re-run failed jobs: {}", err);
                    let _ = sender.send(false);
                } else {
                    let _ = sender.send(true);
                }
            });
        });

        dialog.present(Some(&parent_window));
    });

    button
}

fn create_cancel_button(run: &WorkflowRun, context: &RunActionContext) -> gtk::Button {
    let button = gtk::Button::from_icon_name("process-stop-symbolic");
    crate::ui::utils::describe_control(&button, tr("Cancel run").as_str());
    button.add_css_class("row-action-btn");
    button.add_css_class("cancel-action");
    button.set_focus_on_click(false);

    let run = run.clone();
    let context = context.clone();
    button.connect_clicked(move |btn| {
        confirm_and_cancel_run(&run, &context, btn);
    });

    button
}

/// Presents the cancel confirmation dialog for `run` and issues the API call on
/// confirmation. Shared between the per-run button and the workflow-row button.
pub(crate) fn confirm_and_cancel_run(
    run: &WorkflowRun,
    context: &RunActionContext,
    btn: &gtk::Button,
) {
    let dialog = confirm_dialog(
        tr("Cancel Workflow Run").as_str(),
        tr("Do you want to cancel the in-progress run \"{run}\"?\n\nThis action cannot be undone.")
            .replace("{run}", format_run_title(run).as_str())
            .as_str(),
        true,
    );

    let btn_clone = btn.clone();
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let toast_overlay = context.toast_overlay.clone();
    let parent_window = context.parent_window.clone();
    let run_title = format_run_title(run);
    let run_id = run.id;

    dialog.connect_response(None, move |_dialog, response| {
        if response != "confirm" {
            return;
        }

        btn_clone.set_sensitive(false);

        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        let (sender, receiver) =
            glib::MainContext::default().channel::<bool>(glib::Priority::default());

        receiver.attach(None, move |success| {
            let message = if success {
                tr("✓ Cancelled run '{run}'").replace("{run}", run_title.as_str())
            } else {
                tr("✗ Failed to cancel run")
            };
            let toast = adw::Toast::new(&message);
            toast.set_timeout(if success { 3 } else { 5 });
            toast_overlay.add_toast(toast);
            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_guard = client.lock().clone();
            let cancel_result = client_guard.cancel_run(&owner, &repo, run_id).await;
            if let Err(err) = cancel_result {
                error!("Failed to cancel run: {}", err);
                let _ = sender.send(false);
            } else {
                let _ = sender.send(true);
            }
        });
    });

    dialog.present(Some(&parent_window));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::WorkflowRun;
    use crate::ui::test_helpers::run_gtk_test;

    fn run_stub() -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: None,
            workflow_id: None,
            name: Some("Test".into()),
            display_title: None,
            head_branch: None,
            head_commit: None,
            status: None,
            conclusion: None,
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: Some("https://example.com".into()),
            actor: None,
            triggering_actor: None,
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn confirm_dialog_wires_responses_and_appearance() {
        run_gtk_test("confirm_dialog_wires_responses_and_appearance", || {
            let suggested = confirm_dialog("Re-run", "Re-run this?", false);
            assert!(suggested.has_response("cancel"));
            assert!(suggested.has_response("confirm"));
            assert_eq!(suggested.default_response().as_deref(), Some("cancel"));
            assert_eq!(suggested.close_response().as_str(), "cancel");
            assert_eq!(
                suggested.response_appearance("confirm"),
                adw::ResponseAppearance::Suggested
            );

            let destructive = confirm_dialog("Cancel run", "Cancel this?", true);
            assert_eq!(
                destructive.response_appearance("confirm"),
                adw::ResponseAppearance::Destructive
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn open_button_added_when_url_present() {
        run_gtk_test("open_button_added_when_url_present", || {
            let run = run_stub();
            let client = Arc::new(Mutex::new(GitHubClient::new(None).unwrap()));
            let parent = adw::ApplicationWindow::builder().build();
            let overlay = adw::ToastOverlay::new();

            let context = RunActionContext {
                client: client.clone(),
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                parent_window: parent.clone(),
                toast_overlay: overlay.clone(),
            };

            let box_widget = create_actions_box(&run, &context);

            let mut child = box_widget.first_child();
            let mut button_count = 0;
            while let Some(widget) = child {
                if widget.is::<gtk::Button>() {
                    button_count += 1;
                }
                child = widget.next_sibling();
            }

            assert!(button_count >= 1);
        });
    }
}
