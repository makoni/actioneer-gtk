use super::super::formatting::format_run_title;
use crate::api::GitHubClient;
use crate::api::models::WorkflowRun;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::error;

#[derive(Clone)]
pub(super) struct RunActionContext {
    pub(super) client: Arc<Mutex<GitHubClient>>,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) parent_window: adw::ApplicationWindow,
    pub(super) toast_overlay: adw::ToastOverlay,
}

pub(super) fn create_actions_box(run: &WorkflowRun, context: &RunActionContext) -> gtk::Box {
    let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions_box.set_valign(gtk::Align::Start);
    actions_box.set_halign(gtk::Align::End);

    if let Some(url) = run.html_url.as_ref() {
        actions_box.append(&create_open_button(url));
    }

    if run.is_rerunnable() {
        actions_box.append(&create_rerun_button(run, context));
    }

    if run.has_failed_jobs() {
        actions_box.append(&create_rerun_failed_button(run, context));
    }

    if run.is_cancellable() {
        actions_box.append(&create_cancel_button(run, context));
    }

    actions_box
}

fn create_open_button(url: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name("adw-external-link-symbolic");
    button.set_tooltip_text(Some("Open in GitHub"));
    button.add_css_class("flat");
    button.add_css_class("circular");
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
    button.set_tooltip_text(Some("Re-run workflow"));
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("warning");
    button.set_focus_on_click(false);

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let run_id = run.id;
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let run_title = format_run_title(run);

    button.connect_clicked(move |btn| {
        let dialog = gtk::MessageDialog::new(
            Some(&parent_window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::YesNo,
            format!("Do you want to re-run \"{}\"?", run_title),
        );
        dialog.set_title(Some("Re-run Workflow"));

        let btn_clone = btn.clone();
        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        dialog.connect_response(move |dialog, response| {
            dialog.close();
            if response != gtk::ResponseType::Yes {
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
                    adw::Toast::new(&format!("✓ Re-running '{}'", run_title))
                } else {
                    adw::Toast::new("✗ Failed to re-run workflow")
                };
                toast.set_timeout(if success { 3 } else { 5 });
                toast_overlay.add_toast(toast);
                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_guard = client.lock().clone();
                let rerun_result = client_guard.rerun_workflow(&owner, &repo, run_id).await;

                if rerun_result.is_err() {
                    error!("Failed to re-run workflow: {}", rerun_result.unwrap_err());
                    let _ = sender.send(false);
                } else {
                    let _ = sender.send(true);
                }
            });
        });

        dialog.present();
    });

    button
}

fn create_rerun_failed_button(run: &WorkflowRun, context: &RunActionContext) -> gtk::Button {
    let button = gtk::Button::from_icon_name("system-reboot-symbolic");
    button.set_tooltip_text(Some("Re-run failed jobs"));
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("error");
    button.set_focus_on_click(false);

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let run_id = run.id;
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let run_title = format_run_title(run);

    button.connect_clicked(move |btn| {
        let dialog = gtk::MessageDialog::new(
            Some(&parent_window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Warning,
            gtk::ButtonsType::YesNo,
            format!(
                "Do you want to re-run all failed jobs in \"{}\"?",
                run_title
            ),
        );
        dialog.set_title(Some("Re-run Failed Jobs"));

        let btn_clone = btn.clone();
        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        dialog.connect_response(move |dialog, response| {
            dialog.close();
            if response != gtk::ResponseType::Yes {
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
                    format!("✓ Re-running failed jobs for '{}'", run_title)
                } else {
                    "✗ Failed to re-run failed jobs".into()
                };
                let toast = adw::Toast::new(&message);
                toast.set_timeout(if success { 3 } else { 5 });
                toast_overlay.add_toast(toast);
                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_guard = client.lock().clone();
                let rerun_result = client_guard.rerun_failed_jobs(&owner, &repo, run_id).await;

                if rerun_result.is_err() {
                    error!(
                        "Failed to re-run failed jobs: {}",
                        rerun_result.unwrap_err()
                    );
                    let _ = sender.send(false);
                } else {
                    let _ = sender.send(true);
                }
            });
        });

        dialog.present();
    });

    button
}

fn create_cancel_button(run: &WorkflowRun, context: &RunActionContext) -> gtk::Button {
    let button = gtk::Button::from_icon_name("process-stop-symbolic");
    button.set_tooltip_text(Some("Cancel run"));
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("destructive-action");
    button.set_focus_on_click(false);

    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let run_id = run.id;
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let run_title = format_run_title(run);

    button.connect_clicked(move |btn| {
        let dialog = gtk::MessageDialog::new(
            Some(&parent_window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Warning,
            gtk::ButtonsType::YesNo,
            format!(
                "Do you want to cancel the in-progress run \"{}\"?\n\nThis action cannot be undone.",
                run_title
            ),
        );
        dialog.set_title(Some("Cancel Workflow Run"));

        let btn_clone = btn.clone();
        let client = client.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let toast_overlay = toast_overlay.clone();
        let run_title = run_title.clone();

        dialog.connect_response(move |dialog, response| {
            dialog.close();
            if response != gtk::ResponseType::Yes {
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
                    format!("✓ Cancelled run '{}'", run_title)
                } else {
                    "✗ Failed to cancel run".into()
                };
                let toast = adw::Toast::new(&message);
                toast.set_timeout(if success { 3 } else { 5 });
                toast_overlay.add_toast(toast);
                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_guard = client.lock().clone();
                let cancel_result = client_guard.cancel_run(&owner, &repo, run_id).await;

                if cancel_result.is_err() {
                    error!("Failed to cancel run: {}", cancel_result.unwrap_err());
                    let _ = sender.send(false);
                } else {
                    let _ = sender.send(true);
                }
            });
        });

        dialog.present();
    });

    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::WorkflowRun;
    use crate::ui::test_helpers::gtk_test_guard;

    fn run_stub() -> WorkflowRun {
        WorkflowRun {
            id: 1,
            run_number: None,
            name: Some("Test".into()),
            display_title: None,
            head_branch: None,
            status: None,
            conclusion: None,
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: Some("https://example.com".into()),
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn open_button_added_when_url_present() {
        let Some(_guard) = gtk_test_guard("open_button_added_when_url_present") else {
            return;
        };

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
    }
}
