use crate::auth::device::{
    AccessToken, AuthError, DeviceFlowInfo, poll_device_token, start_device_flow,
};
use crate::config::Config;
use crate::i18n::tr;
use crate::runtime_handle;
use crate::storage::TokenStorage;
use crate::ui::utils::MainContextChannelExt;
use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{error, info};

enum AuthMessage {
    FlowReady(u64, DeviceFlowInfo),
    FlowError(u64, String),
    PollSuccess(u64, AccessToken),
    PollError(u64, String),
    SaveCompleted(u64, Result<(), String>),
}

impl AuthMessage {
    fn attempt_id(&self) -> u64 {
        match self {
            Self::FlowReady(attempt_id, _)
            | Self::FlowError(attempt_id, _)
            | Self::PollSuccess(attempt_id, _)
            | Self::PollError(attempt_id, _)
            | Self::SaveCompleted(attempt_id, _) => *attempt_id,
        }
    }
}

#[derive(Default)]
struct AuthAttemptTracker {
    generation: AtomicU64,
}

impl AuthAttemptTracker {
    fn begin_attempt(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn cancel_active_attempt(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn is_current(&self, attempt_id: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == attempt_id
    }
}

pub struct AuthWindow {
    dialog: adw::Dialog,
    device_info: Arc<Mutex<Option<DeviceFlowInfo>>>,
    status_label: gtk::Label,
    code_box: gtk::Box,
    user_code: gtk::Label,
    open_button: gtk::Button,
    copy_button: gtk::Button,
    spinner: gtk::Spinner,
    attempt_tracker: Arc<AuthAttemptTracker>,
    poll_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl AuthWindow {
    pub fn new() -> Self {
        let dialog = adw::Dialog::builder()
            .title(tr("Sign in to GitHub"))
            .build();
        dialog.set_content_width(460);

        let device_info = Arc::new(Mutex::new(None));
        let attempt_tracker = Arc::new(AuthAttemptTracker::default());
        let poll_task = Arc::new(Mutex::new(None));

        let (
            content_box,
            status_label,
            code_box,
            user_code,
            open_button,
            copy_button,
            spinner,
            cancel_button,
        ) = Self::build_ui_content();

        dialog.set_child(Some(&content_box));

        let auth_window = Self {
            dialog: dialog.clone(),
            device_info: device_info.clone(),
            status_label,
            code_box,
            user_code,
            open_button: open_button.clone(),
            copy_button,
            spinner,
            attempt_tracker: attempt_tracker.clone(),
            poll_task: poll_task.clone(),
        };

        {
            let device_info_for_open = device_info.clone();
            open_button.connect_clicked(move |_| {
                if let Some(info) = device_info_for_open.lock().clone() {
                    let _ = open::that(&info.verification_uri);
                }
            });
        }

        {
            let dialog = dialog.clone();
            let attempt_tracker = attempt_tracker.clone();
            let poll_task = poll_task.clone();
            cancel_button.connect_clicked(move |_| {
                cancel_attempt(&attempt_tracker, &poll_task);
                dialog.close();
            });
        }

        {
            let attempt_tracker = attempt_tracker.clone();
            let poll_task = poll_task.clone();
            dialog.connect_closed(move |_| {
                cancel_attempt(&attempt_tracker, &poll_task);
            });
        }

        auth_window
    }

    fn build_ui_content() -> (
        gtk::Box,
        gtk::Label,
        gtk::Box,
        gtk::Label,
        gtk::Button,
        gtk::Button,
        gtk::Spinner,
        gtk::Button,
    ) {
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        content_box.set_margin_top(48);
        content_box.set_margin_bottom(48);
        content_box.set_margin_start(48);
        content_box.set_margin_end(48);
        content_box.set_valign(gtk::Align::Center);

        let title = gtk::Label::new(Some(tr("Sign in to GitHub").as_str()));
        title.add_css_class("title-1");
        content_box.append(&title);

        // Status with spinner in a horizontal box
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_box.set_halign(gtk::Align::Center);

        let status_label = gtk::Label::new(Some(tr("Initializing authentication...").as_str()));
        status_label.set_wrap(true);
        status_label.set_justify(gtk::Justification::Center);
        status_box.append(&status_label);

        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        status_box.append(&spinner);

        content_box.append(&status_box);

        let code_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        code_box.set_visible(false);

        let code_label = gtk::Label::new(Some(tr("Enter this code on GitHub:").as_str()));
        code_box.append(&code_label);

        // Code display with copy button
        let code_display_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        code_display_box.set_halign(gtk::Align::Center);

        let user_code = gtk::Label::new(Some(""));
        user_code.add_css_class("title-2");
        user_code.set_selectable(true);
        code_display_box.append(&user_code);

        let copy_button = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_button.set_tooltip_text(Some(tr("Copy code to clipboard").as_str()));
        copy_button.add_css_class("flat");
        copy_button.add_css_class("circular");
        copy_button.set_visible(false);

        let user_code_for_copy = user_code.clone();
        copy_button.connect_clicked(move |btn| {
            let code_text = user_code_for_copy.text();
            if let Some(display) = gtk::gdk::Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&code_text);
            }

            btn.set_icon_name("emblem-ok-symbolic");
            glib::timeout_add_seconds_local(2, {
                let btn = btn.clone();
                move || {
                    btn.set_icon_name("edit-copy-symbolic");
                    glib::ControlFlow::Break
                }
            });
        });

        code_display_box.append(&copy_button);
        code_box.append(&code_display_box);

        content_box.append(&code_box);

        let open_button = gtk::Button::with_label(tr("Open GitHub in Browser").as_str());
        open_button.add_css_class("suggested-action");
        open_button.add_css_class("pill");
        open_button.set_visible(false);
        content_box.append(&open_button);

        let cancel_button = gtk::Button::with_label(tr("Cancel").as_str());
        content_box.append(&cancel_button);

        (
            content_box,
            status_label,
            code_box,
            user_code,
            open_button,
            copy_button,
            spinner,
            cancel_button,
        )
    }

    pub fn present<F>(&self, parent: Option<&impl gtk::prelude::IsA<gtk::Widget>>, on_success: F)
    where
        F: Fn() + 'static,
    {
        self.cancel_active_attempt();
        self.reset_ui();
        let attempt_id = self.attempt_tracker.begin_attempt();
        self.start_device_flow(attempt_id, Rc::new(on_success));
        self.dialog.present(parent);
    }

    fn reset_ui(&self) {
        *self.device_info.lock() = None;
        self.status_label
            .set_text(tr("Initializing authentication...").as_str());
        self.code_box.set_visible(false);
        self.user_code.set_text("");
        self.open_button.set_visible(false);
        self.copy_button.set_visible(false);
        self.spinner.stop();
        self.spinner.set_visible(false);
    }

    fn cancel_active_attempt(&self) {
        cancel_attempt(&self.attempt_tracker, &self.poll_task);
    }

    fn start_device_flow(&self, attempt_id: u64, on_success: Rc<dyn Fn()>) {
        let (sender, receiver) =
            glib::MainContext::default().channel::<AuthMessage>(glib::Priority::default());

        let start_sender = sender.clone();
        runtime_handle().spawn(async move {
            info!("Starting device flow authentication");
            let scopes = ["repo", "workflow"];
            let message = match start_device_flow(Config::github_client_id(), &scopes).await {
                Ok(flow_info) => AuthMessage::FlowReady(attempt_id, flow_info),
                Err(err) => AuthMessage::FlowError(attempt_id, err.to_string()),
            };

            if start_sender.send(message).is_err() {
                error!("Failed to deliver authentication flow result to UI");
            }
        });

        let poll_sender = sender.clone();
        let device_info_clone = self.device_info.clone();
        let status_clone = self.status_label.clone();
        let code_box_clone = self.code_box.clone();
        let user_code_clone = self.user_code.clone();
        let open_button_clone = self.open_button.clone();
        let copy_button_clone = self.copy_button.clone();
        let spinner_clone = self.spinner.clone();
        let dialog_clone = self.dialog.clone();
        let completion = on_success.clone();
        let attempt_tracker = self.attempt_tracker.clone();
        let poll_task = self.poll_task.clone();

        receiver.attach(None, move |message| {
            if !attempt_tracker.is_current(message.attempt_id()) {
                return ControlFlow::Continue;
            }

            match message {
                AuthMessage::FlowReady(_, info) => {
                    *device_info_clone.lock() = Some(info.clone());
                    user_code_clone.set_text(&info.user_code);
                    code_box_clone.set_visible(true);
                    open_button_clone.set_visible(true);
                    copy_button_clone.set_visible(true);
                    spinner_clone.set_visible(true);
                    spinner_clone.start();
                    status_clone
                        .set_text(tr("Open GitHub in your browser and enter the code.").as_str());

                    let poll_info = info.clone();
                    let sender_for_polling = poll_sender.clone();
                    let poll_handle = runtime_handle().spawn(async move {
                        let interval_secs = poll_info.interval.max(1) as u64;
                        let interval = Duration::from_secs(interval_secs);
                        let max_attempts =
                            (poll_info.expires_in / poll_info.interval.max(1)).max(1) as usize;
                        let mut attempts = 0usize;

                        loop {
                            if attempts >= max_attempts {
                                let _ = sender_for_polling.send(AuthMessage::PollError(
                                    attempt_id,
                                    tr("Authentication timeout"),
                                ));
                                break;
                            }

                            tokio::time::sleep(interval).await;
                            attempts += 1;

                            let poll_result = poll_device_token(
                                Config::github_client_id(),
                                &poll_info.device_code,
                            )
                            .await;

                            match poll_result {
                                Ok(token) => {
                                    info!("Authentication successful");
                                    if sender_for_polling
                                        .send(AuthMessage::PollSuccess(attempt_id, token))
                                        .is_err()
                                    {
                                        error!("Failed to deliver authentication success to UI");
                                    }
                                    break;
                                }
                                Err(AuthError::AuthorizationPending) => continue,
                                Err(AuthError::SlowDown) => {
                                    tokio::time::sleep(interval).await;
                                }
                                Err(AuthError::ExpiredToken) => {
                                    let _ = sender_for_polling.send(AuthMessage::PollError(
                                        attempt_id,
                                        tr("Authentication timeout"),
                                    ));
                                    break;
                                }
                                Err(AuthError::AccessDenied) => {
                                    let _ = sender_for_polling.send(AuthMessage::PollError(
                                        attempt_id,
                                        tr("Access denied"),
                                    ));
                                    break;
                                }
                                Err(AuthError::RequestFailed(err)) => {
                                    let _ = sender_for_polling
                                        .send(AuthMessage::PollError(attempt_id, err.to_string()));
                                    break;
                                }
                                Err(AuthError::Unknown(err)) => {
                                    let _ = sender_for_polling
                                        .send(AuthMessage::PollError(attempt_id, err));
                                    break;
                                }
                            }
                        }
                    });
                    if let Some(existing) = poll_task.lock().replace(poll_handle) {
                        existing.abort();
                    }

                    status_clone.set_text(tr("Waiting for authorization...").as_str());
                    ControlFlow::Continue
                }
                AuthMessage::FlowError(_, err) => {
                    error!("Failed to start device flow: {}", err);
                    status_clone.set_text(
                        tr("Error: {message}")
                            .replace("{message}", err.as_str())
                            .as_str(),
                    );
                    if let Some(existing) = poll_task.lock().take() {
                        existing.abort();
                    }
                    spinner_clone.stop();
                    spinner_clone.set_visible(false);
                    ControlFlow::Break
                }
                AuthMessage::PollSuccess(_, token) => {
                    status_clone.set_text(tr("Saving token...").as_str());
                    spinner_clone.set_visible(true);
                    spinner_clone.start();
                    poll_task.lock().take();

                    let save_sender = poll_sender.clone();
                    runtime_handle().spawn(async move {
                        let save_result = tokio::task::spawn_blocking(move || save_token(token))
                            .await
                            .map_err(|err| format!("Failed to join token save task: {err}"))
                            .and_then(|result| result);

                        if save_sender
                            .send(AuthMessage::SaveCompleted(attempt_id, save_result))
                            .is_err()
                        {
                            error!("Failed to deliver token save result to UI");
                        }
                    });

                    ControlFlow::Continue
                }
                AuthMessage::PollError(_, err) => {
                    error!("Polling failed: {}", err);
                    if let Some(existing) = poll_task.lock().take() {
                        existing.abort();
                    }
                    spinner_clone.stop();
                    spinner_clone.set_visible(false);
                    status_clone.set_text(
                        tr("Error: {message}")
                            .replace("{message}", err.as_str())
                            .as_str(),
                    );
                    ControlFlow::Break
                }
                AuthMessage::SaveCompleted(_, save_result) => {
                    spinner_clone.stop();
                    spinner_clone.set_visible(false);
                    match save_result {
                        Ok(()) => {
                            status_clone.set_text(tr("Signed in successfully").as_str());
                            dialog_clone.close();
                            completion();
                        }
                        Err(err) => {
                            error!("Failed to save token: {}", err);
                            status_clone.set_text(
                                tr("Error saving token: {message}")
                                    .replace("{message}", err.as_str())
                                    .as_str(),
                            );
                        }
                    }
                    ControlFlow::Break
                }
            }
        });
    }
}

fn cancel_attempt(
    attempt_tracker: &AuthAttemptTracker,
    poll_task: &Mutex<Option<tokio::task::JoinHandle<()>>>,
) {
    attempt_tracker.cancel_active_attempt();
    if let Some(existing) = poll_task.lock().take() {
        existing.abort();
    }
}

fn save_token(token: AccessToken) -> Result<(), String> {
    let storage = TokenStorage::new().map_err(|err| err.to_string())?;
    storage
        .save_token(&token.token)
        .map_err(|err| err.to_string())?;
    if token.scope.is_empty() {
        info!("Token saved successfully (type: {})", token.token_type);
    } else {
        info!(
            "Token saved successfully (type: {}, scope: {})",
            token.token_type, token.scope
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AuthAttemptTracker;

    #[test]
    fn auth_attempt_tracker_invalidates_stale_attempts() {
        let tracker = AuthAttemptTracker::default();
        let first = tracker.begin_attempt();
        assert!(tracker.is_current(first));

        let second = tracker.begin_attempt();
        assert!(!tracker.is_current(first));
        assert!(tracker.is_current(second));

        tracker.cancel_active_attempt();
        assert!(!tracker.is_current(second));
    }
}
