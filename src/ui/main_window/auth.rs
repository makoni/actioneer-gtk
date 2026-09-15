use super::MainWindow;
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::tokens::TokenStorage;
use crate::ui::auth_window::AuthWindow;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use tracing::{error, info, warn};

fn load_token_if_present_blocking() -> Result<Option<String>, String> {
    let storage = TokenStorage::new().map_err(|err| err.to_string())?;
    if !storage.has_token() {
        return Ok(None);
    }
    storage.get_token().map(Some).map_err(|err| err.to_string())
}

fn load_token_blocking() -> Result<String, String> {
    let storage = TokenStorage::new().map_err(|err| err.to_string())?;
    storage.get_token().map_err(|err| err.to_string())
}

fn delete_token_blocking() -> Result<(), String> {
    let storage = TokenStorage::new().map_err(|err| err.to_string())?;
    storage.delete_token().map_err(|err| err.to_string())
}

impl MainWindow {
    /// Builds the sign-out confirmation dialog (without wiring the response
    /// handler), so its structure can be asserted in tests.
    fn build_sign_out_dialog() -> adw::AlertDialog {
        let dialog = adw::AlertDialog::new(
            None,
            Some(
                tr("Are you sure you want to sign out?\n\nYou will need to sign in again to continue.")
                    .as_str(),
            ),
        );
        dialog.add_response("cancel", tr("No").as_str());
        dialog.add_response("signout", tr("Yes").as_str());
        dialog.set_response_appearance("signout", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog
    }

    pub(super) fn show_sign_out_dialog(&self) {
        let parent = self.window.clone();
        let this = self.clone();

        let dialog = Self::build_sign_out_dialog();

        dialog.connect_response(None, move |_dialog, response| {
            if response == "signout" {
                // Leave the signed-in UI immediately rather than waiting on the
                // keyring. Deleting the token can *block indefinitely* when the
                // secret service cannot be reached — no session keyring, a
                // locked one, a sandboxed environment — and the previous code
                // did the transition inside the reply handler, so the user
                // clicked "Yes", the dialog closed, and nothing else ever
                // happened. That was the demo-mode sign-out bug: demo has no
                // token to delete, yet its sign-out still hung on the keyring.
                info!("Signing out");
                this.enter_signed_out_state();

                // The deletion still runs, in the background, because a demo
                // session started while a token is stored must not leave that
                // token behind — the focus handler would sign the user straight
                // back in. Its outcome only gets logged: the session is already
                // over from the user's point of view.
                crate::runtime::handle().spawn(async move {
                    match tokio::task::spawn_blocking(delete_token_blocking).await {
                        Ok(Ok(())) => info!("Stored token deleted"),
                        Ok(Err(err)) => error!("Failed to delete token: {}", err),
                        Err(err) => error!("Sign-out task failed to join: {}", err),
                    }
                });
            }
        });

        dialog.present(Some(&parent));
    }

    pub(super) fn check_authentication(&self) {
        let this = self.clone();
        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Option<String>, String>>(glib::Priority::default());

        receiver.attach(None, move |result| {
            match result {
                Ok(Some(token)) => {
                    info!("Found existing token, initializing client");
                    if !this.initialize_client(token) {
                        this.enter_signed_out_state();
                    }
                }
                Ok(None) => {
                    info!("No token found, presenting welcome screen");
                    this.enter_signed_out_state();
                }
                Err(err) => {
                    error!("Failed to access token storage: {}", err);
                    this.enter_signed_out_state();
                }
            }
            glib::ControlFlow::Break
        });

        crate::runtime::handle().spawn(async move {
            let result = tokio::task::spawn_blocking(load_token_if_present_blocking)
                .await
                .map_err(|err| format!("Failed to join auth check task: {err}"))
                .and_then(|result| result);
            let _ = sender.send(result);
        });
    }

    pub(super) fn show_auth_window(&self) {
        let auth_window = AuthWindow::new();
        let this = self.clone();
        auth_window.present(Some(&self.window), move || {
            this.handle_sign_in_success();
        });
    }

    pub(super) fn handle_auth_failure(&self) {
        info!("Authentication failed; clearing token and returning to welcome");

        self.enter_signed_out_state();

        crate::runtime::handle().spawn(async move {
            let result = tokio::task::spawn_blocking(delete_token_blocking)
                .await
                .map_err(|err| format!("Failed to join auth-failure token cleanup task: {err}"))
                .and_then(|result| result);
            if let Err(err) = result {
                warn!("Failed to delete token after auth failure: {}", err);
            }
        });
    }

    pub(super) fn handle_sign_in_success(&self) {
        let this = self.clone();
        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<String, String>>(glib::Priority::default());

        receiver.attach(None, move |result| {
            match result {
                Ok(token) => {
                    if !this.initialize_client(token) {
                        this.enter_signed_out_state();
                    }
                }
                Err(err) => {
                    error!("Token unavailable after sign-in: {}", err);
                    this.enter_signed_out_state();
                }
            }
            glib::ControlFlow::Break
        });

        crate::runtime::handle().spawn(async move {
            let result = tokio::task::spawn_blocking(load_token_blocking)
                .await
                .map_err(|err| format!("Failed to join sign-in token load task: {err}"))
                .and_then(|result| result);
            let _ = sender.send(result);
        });
    }

    pub(super) fn setup_focus_handler(&self) {
        let this = self.clone();
        self.window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                return;
            }

            if this.is_demo_mode() {
                return;
            }

            let this_for_ui = this.clone();
            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<String, String>>(glib::Priority::default());

            receiver.attach(None, move |token_result| {
                match token_result {
                    Ok(token) => {
                        let needs_client = this_for_ui.client.lock().is_none();
                        if needs_client {
                            info!("Token available after auth, initializing client");
                            if !this_for_ui.initialize_client(token) {
                                this_for_ui.enter_signed_out_state();
                            }
                        }
                    }
                    Err(_) => {
                        let had_client = {
                            let mut guard = this_for_ui.client.lock();
                            let had = guard.is_some();
                            *guard = None;
                            had
                        };

                        if had_client {
                            info!("Token missing after focus, returning to welcome screen");
                        }

                        this_for_ui.enter_signed_out_state();
                    }
                }
                glib::ControlFlow::Break
            });

            crate::runtime::handle().spawn(async move {
                let result = tokio::task::spawn_blocking(load_token_blocking)
                    .await
                    .map_err(|err| format!("Failed to join focus token check task: {err}"))
                    .and_then(|result| result);
                let _ = sender.send(result);
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    #[test]
    #[ignore = "requires GTK display"]
    fn sign_out_dialog_has_expected_responses() {
        run_gtk_test("sign_out_dialog_has_expected_responses", || {
            let dialog = MainWindow::build_sign_out_dialog();

            // The migration to adw::AlertDialog must keep the two response ids the
            // handler relies on, with cancel as the safe default/close response and
            // the destructive appearance on the confirming action.
            assert!(dialog.has_response("cancel"));
            assert!(dialog.has_response("signout"));
            assert_eq!(dialog.default_response().as_deref(), Some("cancel"));
            assert_eq!(dialog.close_response().as_str(), "cancel");
            assert_eq!(
                dialog.response_appearance("signout"),
                adw::ResponseAppearance::Destructive
            );
        });
    }
}
