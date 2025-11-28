use crate::auth::device::{
    AccessToken, AuthError, DeviceFlowInfo, poll_device_token, start_device_flow,
};
use crate::config::Config;
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
use std::time::Duration;
use tracing::{error, info};

enum AuthMessage {
    FlowReady(DeviceFlowInfo),
    FlowError(String),
    PollSuccess(AccessToken),
    PollError(String),
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
}

impl AuthWindow {
    pub fn new() -> Self {
        let dialog = adw::Dialog::builder().title("Sign in to GitHub").build();
        dialog.set_content_width(460);

        let device_info = Arc::new(Mutex::new(None));

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
            cancel_button.connect_clicked(move |_| {
                dialog.close();
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

        let title = gtk::Label::new(Some("Sign in to GitHub"));
        title.add_css_class("title-1");
        content_box.append(&title);

        // Status with spinner in a horizontal box
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_box.set_halign(gtk::Align::Center);

        let status_label = gtk::Label::new(Some("Initializing authentication..."));
        status_label.set_wrap(true);
        status_label.set_justify(gtk::Justification::Center);
        status_box.append(&status_label);

        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        status_box.append(&spinner);

        content_box.append(&status_box);

        let code_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        code_box.set_visible(false);

        let code_label = gtk::Label::new(Some("Enter this code on GitHub:"));
        code_box.append(&code_label);

        // Code display with copy button
        let code_display_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        code_display_box.set_halign(gtk::Align::Center);

        let user_code = gtk::Label::new(Some(""));
        user_code.add_css_class("title-2");
        user_code.set_selectable(true);
        code_display_box.append(&user_code);

        let copy_button = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_button.set_tooltip_text(Some("Copy code to clipboard"));
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

        let open_button = gtk::Button::with_label("Open GitHub in Browser");
        open_button.add_css_class("suggested-action");
        open_button.add_css_class("pill");
        open_button.set_visible(false);
        content_box.append(&open_button);

        let cancel_button = gtk::Button::with_label("Cancel");
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
        self.reset_ui();
        self.start_device_flow(Rc::new(on_success));
        self.dialog.present(parent);
    }

    fn reset_ui(&self) {
        *self.device_info.lock() = None;
        self.status_label.set_text("Initializing authentication...");
        self.code_box.set_visible(false);
        self.user_code.set_text("");
        self.open_button.set_visible(false);
        self.copy_button.set_visible(false);
        self.spinner.stop();
        self.spinner.set_visible(false);
    }

    fn start_device_flow(&self, on_success: Rc<dyn Fn()>) {
        let (sender, receiver) =
            glib::MainContext::default().channel::<AuthMessage>(glib::Priority::default());

        let start_sender = sender.clone();
        runtime_handle().spawn(async move {
            info!("Starting device flow authentication");
            let scopes = ["repo", "workflow"];
            let message = match start_device_flow(Config::github_client_id(), &scopes).await {
                Ok(flow_info) => AuthMessage::FlowReady(flow_info),
                Err(err) => AuthMessage::FlowError(err.to_string()),
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

        receiver.attach(None, move |message| match message {
            AuthMessage::FlowReady(info) => {
                *device_info_clone.lock() = Some(info.clone());
                user_code_clone.set_text(&info.user_code);
                code_box_clone.set_visible(true);
                open_button_clone.set_visible(true);
                copy_button_clone.set_visible(true);
                spinner_clone.set_visible(true);
                spinner_clone.start();
                status_clone.set_text("Open GitHub in your browser and enter the code.");

                let poll_info = info.clone();
                let sender_for_polling = poll_sender.clone();

                runtime_handle().spawn(async move {
                    let interval_secs = poll_info.interval.max(1) as u64;
                    let interval = Duration::from_secs(interval_secs);
                    let max_attempts =
                        (poll_info.expires_in / poll_info.interval.max(1)).max(1) as usize;
                    let mut attempts = 0usize;

                    loop {
                        if attempts >= max_attempts {
                            let _ = sender_for_polling
                                .send(AuthMessage::PollError("Authentication timeout".into()));
                            break;
                        }

                        tokio::time::sleep(interval).await;
                        attempts += 1;

                        let poll_result =
                            poll_device_token(Config::github_client_id(), &poll_info.device_code)
                                .await;

                        match poll_result {
                            Ok(token) => {
                                info!("Authentication successful");
                                if sender_for_polling
                                    .send(AuthMessage::PollSuccess(token))
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
                                let _ = sender_for_polling
                                    .send(AuthMessage::PollError("Authentication timeout".into()));
                                break;
                            }
                            Err(AuthError::AccessDenied) => {
                                let _ = sender_for_polling
                                    .send(AuthMessage::PollError("Access denied".into()));
                                break;
                            }
                            Err(AuthError::RequestFailed(err)) => {
                                let _ = sender_for_polling
                                    .send(AuthMessage::PollError(err.to_string()));
                                break;
                            }
                            Err(AuthError::Unknown(err)) => {
                                let _ = sender_for_polling.send(AuthMessage::PollError(err));
                                break;
                            }
                        }
                    }
                });

                status_clone.set_text("Waiting for authorization...");
                ControlFlow::Continue
            }
            AuthMessage::FlowError(err) => {
                error!("Failed to start device flow: {}", err);
                status_clone.set_text(&format!("Error: {}", err));
                spinner_clone.stop();
                spinner_clone.set_visible(false);
                ControlFlow::Break
            }
            AuthMessage::PollSuccess(token) => {
                spinner_clone.stop();
                spinner_clone.set_visible(false);
                match save_token_and_close(token, &dialog_clone) {
                    Ok(()) => {
                        status_clone.set_text("Signed in successfully");
                        completion();
                    }
                    Err(err) => {
                        error!("Failed to save token: {}", err);
                        status_clone.set_text(&format!("Error saving token: {}", err));
                    }
                }
                ControlFlow::Break
            }
            AuthMessage::PollError(err) => {
                error!("Polling failed: {}", err);
                spinner_clone.stop();
                spinner_clone.set_visible(false);
                status_clone.set_text(&format!("Error: {}", err));
                ControlFlow::Break
            }
        });
    }
}

fn save_token_and_close(
    token: AccessToken,
    dialog: &adw::Dialog,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = TokenStorage::new()?;
    storage.save_token(&token.token)?;
    if token.scope.is_empty() {
        info!("Token saved successfully (type: {})", token.token_type);
    } else {
        info!(
            "Token saved successfully (type: {}, scope: {})",
            token.token_type, token.scope
        );
    }

    dialog.close();

    Ok(())
}
