use crate::APP_ICON_NAME;
use crate::ui::utils::channel::{MainContextChannelExt, Sender as UiChannelSender};
use anyhow::anyhow;
use ashpd::desktop::Icon as PortalIcon;
use ashpd::desktop::notification::{
    Notification as PortalNotification, NotificationProxy, Priority as PortalPriority,
};
use gtk4::prelude::{ApplicationExt, IsA};
use gtk4::{gio, glib};
use std::env;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

const DEFAULT_ICON_NAME: &str = APP_ICON_NAME;
const DEFAULT_NOTIFICATION_ACTION: &str = "app.focus-main-window";

/// Notification manager for Linux using XDG Desktop Notifications
/// Similar to NotificationManager.swift in macOS version
#[derive(Clone)]
pub struct NotificationManager {
    app_id: String,
    icon_name: String,
    dispatcher: NotificationDispatcher,
    prefer_portal_default: bool,
    recent_notifications: Arc<StdMutex<std::collections::HashMap<String, Instant>>>,
}

impl NotificationManager {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self::with_icon(app_id, DEFAULT_ICON_NAME)
    }

    pub fn with_icon(app_id: impl Into<String>, icon_name: impl Into<String>) -> Self {
        let icon_name = icon_name.into();
        let app_id = app_id.into();

        let dispatcher = gio::Application::default()
            .map(|app| Self::create_application_dispatcher(&app))
            .unwrap_or(NotificationDispatcher::Fallback);
        Self::with_icon_internal(app_id, icon_name, dispatcher, false)
    }

    pub fn for_application<A: IsA<gio::Application>>(app: &A) -> Self {
        Self::with_icon_for_application(app, DEFAULT_ICON_NAME)
    }

    pub fn with_icon_for_application<A: IsA<gio::Application>>(
        app: &A,
        icon_name: impl Into<String>,
    ) -> Self {
        let app_ref: &gio::Application = app.as_ref();
        let app_id = app_ref
            .application_id()
            .map(|id| id.to_string())
            .unwrap_or_else(|| DEFAULT_ICON_NAME.to_string());

        let dispatcher = Self::create_application_dispatcher(app_ref);
        let prefer_portal_default = app_ref.flags().contains(gio::ApplicationFlags::NON_UNIQUE);

        Self::with_icon_internal(app_id, icon_name.into(), dispatcher, prefer_portal_default)
    }

    fn with_icon_internal(
        app_id: String,
        icon_name: String,
        dispatcher: NotificationDispatcher,
        prefer_portal_default: bool,
    ) -> Self {
        Self {
            app_id,
            icon_name,
            dispatcher,
            prefer_portal_default,
            recent_notifications: Arc::new(StdMutex::new(std::collections::HashMap::new())),
        }
    }

    fn create_application_dispatcher(app: &gio::Application) -> NotificationDispatcher {
        let (sender, receiver) = MainContextChannelExt::channel(
            &glib::MainContext::default(),
            glib::Priority::default(),
        );
        let app_clone = app.clone();

        receiver.attach(None, move |command| {
            let NotificationCommand {
                payload,
                completion,
            } = command;
            let result = NotificationManager::deliver_notification(&app_clone, payload);
            if completion.send(result).is_err() {
                warn!("Notification completion receiver dropped before delivery finished");
            }
            glib::ControlFlow::Continue
        });

        NotificationDispatcher::Channel(sender)
    }

    /// Send a notification for completed workflow
    pub async fn notify_workflow_completed(
        &self,
        workflow_name: &str,
        run_title: &str,
        status: &str,
        conclusion: Option<&str>,
    ) -> anyhow::Result<()> {
        if self.should_suppress_duplicate(workflow_name, run_title, conclusion) {
            debug!(
                workflow = workflow_name,
                run = run_title,
                "Suppressing duplicate workflow notification (recently sent)"
            );
            return Ok(());
        }

        let summary = format!("Workflow Completed: {}", workflow_name);
        let body = format!("{} - {}", run_title, self.conclusion_text(conclusion));

        let priority = if conclusion == Some("failure") {
            gio::NotificationPriority::High
        } else {
            gio::NotificationPriority::Normal
        };

        info!(
            workflow = workflow_name,
            run = run_title,
            conclusion = conclusion.unwrap_or("unknown"),
            status = status,
            "Dispatching workflow completion notification"
        );

        self.dispatch_notification(
            Some(
                self.make_notification_id("workflow", &format!("{}-{}", workflow_name, run_title)),
            ),
            summary,
            Some(body),
            priority,
        )
        .await?;

        debug!("Notification sent successfully");

        Ok(())
    }

    fn should_suppress_duplicate(
        &self,
        workflow_name: &str,
        run_title: &str,
        conclusion: Option<&str>,
    ) -> bool {
        const DEDUP_TTL: Duration = Duration::from_secs(120);

        let key = format!(
            "{}|{}|{}",
            workflow_name,
            run_title,
            conclusion.unwrap_or("unknown")
        );

        let now = Instant::now();

        if let Ok(mut guard) = self.recent_notifications.lock() {
            guard.retain(|_, ts| now.duration_since(*ts) <= DEDUP_TTL);

            if guard.contains_key(&key) {
                return true;
            }

            guard.insert(key, now);
        }

        false
    }

    /// Send a generic notification for manual testing or informational messages
    pub async fn notify_message(&self, title: &str, body: &str) -> anyhow::Result<()> {
        debug!("Dispatching manual notification: {}", title);

        self.dispatch_notification(
            Some(self.make_notification_id("message", title)),
            title.to_string(),
            Some(body.to_string()),
            gio::NotificationPriority::Normal,
        )
        .await
    }

    fn conclusion_text(&self, conclusion: Option<&str>) -> String {
        match conclusion {
            Some("success") => "Success ✓".to_string(),
            Some("failure") => "Failed ✗".to_string(),
            Some("cancelled") => "Cancelled".to_string(),
            Some("skipped") => "Skipped".to_string(),
            Some("timed_out") => "Timed Out".to_string(),
            Some("action_required") => "Action Required".to_string(),
            Some("neutral") => "Neutral".to_string(),
            Some(other) => other
                .replace('_', " ")
                .split_whitespace()
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().chain(chars).collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            None => "Completed".to_string(),
        }
    }

    async fn dispatch_notification(
        &self,
        identifier: Option<String>,
        title: String,
        body: Option<String>,
        priority: gio::NotificationPriority,
    ) -> anyhow::Result<()> {
        let payload = NotificationPayload {
            identifier,
            title,
            body,
            priority,
            icon_name: self.icon_name.clone(),
        };

        let force_native = env::var_os("ACTIONEER_FORCE_NATIVE_NOTIFICATIONS").is_some();
        let force_portal = env::var_os("ACTIONEER_FORCE_PORTAL_NOTIFICATIONS").is_some();
        let sandboxed = is_sandboxed();
        let portal_first = force_portal || sandboxed;
        let portal_allowed = portal_first || self.prefer_portal_default;

        info!(
            app_id = %self.app_id,
            sandboxed,
            force_portal,
            force_native,
            prefer_portal_default = self.prefer_portal_default,
            portal_first,
            portal_allowed,
            "Notification dispatch decision"
        );

        if portal_first && !force_native {
            debug!("Attempting portal notification first");

            match self.dispatch_via_portal(payload.clone()).await {
                Ok(()) => {
                    debug!("Portal notification dispatched successfully");
                    return Ok(());
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        "Portal notification dispatch failed, retrying via GApplication"
                    );
                }
            }
        }

        // Default path for non-sandboxed hosts or portal fallback
        match self.dispatch_via_main_context(payload.clone()).await {
            Ok(()) => {
                debug!("Notification sent via main context/native path");
                Ok(())
            }
            Err(native_err) => {
                warn!(error = %native_err, "Native notification failed");

                // If native fails and portal is allowed (not forced-native), try portal.
                if !force_native && portal_allowed {
                    match self.dispatch_via_portal(payload).await {
                        Ok(()) => {
                            debug!("Portal notification dispatched after native failure");
                            Ok(())
                        }
                        Err(portal_err) => Err(anyhow!(
                            "Both native and portal notifications failed: {native_err}; portal error: {portal_err}"
                        )),
                    }
                } else {
                    Err(native_err)
                }
            }
        }
    }

    async fn dispatch_via_main_context(&self, payload: NotificationPayload) -> anyhow::Result<()> {
        match &self.dispatcher {
            NotificationDispatcher::Channel(sender) => {
                let (completion_tx, completion_rx) = oneshot::channel();
                if sender
                    .send(NotificationCommand {
                        payload,
                        completion: completion_tx,
                    })
                    .is_err()
                {
                    error!("Notification channel closed before dispatch");
                    return Err(anyhow!("Notification dispatcher unavailable"));
                }

                match completion_rx.await {
                    Ok(result) => {
                        if let Err(ref err) = result {
                            error!("Failed to send GNOME notification: {}", err);
                        }
                        debug!("Notification dispatched via app channel");
                        result
                    }
                    Err(_) => {
                        error!("Notification dispatcher dropped before completion");
                        Err(anyhow!(
                            "Notification dispatcher dropped before sending result"
                        ))
                    }
                }
            }
            NotificationDispatcher::Fallback => {
                let (sender, receiver) = oneshot::channel();

                glib::MainContext::default().invoke(move || {
                    let result = (|| -> anyhow::Result<()> {
                        let application = gio::Application::default()
                            .ok_or_else(|| anyhow!("No active GApplication registered"))?;

                        Self::deliver_notification(&application, payload)
                    })();

                    if sender.send(result).is_err() {
                        warn!("Notification receiver dropped before completion");
                    }
                });

                match receiver.await {
                    Ok(result) => {
                        if let Err(ref err) = result {
                            error!("Failed to send GNOME notification: {}", err);
                        }
                        debug!("Notification dispatched via fallback main context");
                        result
                    }
                    Err(_) => {
                        error!("Notification dispatcher dropped before completion");
                        Err(anyhow!("Notification dispatcher dropped before sending"))
                    }
                }
            }
        }
    }

    async fn dispatch_via_portal(&self, payload: NotificationPayload) -> anyhow::Result<()> {
        // When running outside a sandbox, the portal drops notifications unless an app ID
        // is provided. Set the well-known ID unless the user already overrode it.
        if env::var_os("XDG_DESKTOP_PORTAL_FORCE_USE_THIS_APP_ID").is_none() {
            let app_id = resolve_portal_app_id(&self.app_id);
            // SAFETY: Setting process env var; scoped to current process.
            unsafe {
                env::set_var("XDG_DESKTOP_PORTAL_FORCE_USE_THIS_APP_ID", app_id);
            }
        }

        let NotificationPayload {
            identifier,
            title,
            body,
            priority,
            icon_name,
        } = payload;

        let proxy = NotificationProxy::new().await?;
        let mut notification = PortalNotification::new(&title)
            .priority(Some(Self::portal_priority(priority)))
            .default_action(Some(DEFAULT_NOTIFICATION_ACTION))
            .icon(PortalIcon::with_names([icon_name.as_str()]));

        if let Some(body_text) = body.as_deref() {
            notification = notification.body(Some(body_text));
        }

        let id = identifier.unwrap_or_else(|| self.make_notification_id("portal", &title));

        debug!(
            workflow = title.as_str(),
            "Dispatching notification via portal fallback"
        );

        proxy.add_notification(&id, notification).await?;

        Ok(())
    }

    fn deliver_notification(
        application: &gio::Application,
        payload: NotificationPayload,
    ) -> anyhow::Result<()> {
        // Drop out early if the GApplication did not register; this allows callers
        // to fall back to the portal path instead of silently succeeding.
        if !application.is_registered() {
            return Err(anyhow!(
                "GApplication is not registered; cannot send notification"
            ));
        }

        let NotificationPayload {
            identifier,
            title,
            body,
            priority,
            icon_name,
        } = payload;

        let notification = gio::Notification::new(&title);
        if let Some(ref body_text) = body {
            notification.set_body(Some(body_text));
        }
        notification.set_priority(priority);
        notification.set_default_action(DEFAULT_NOTIFICATION_ACTION);

        let icon = gio::ThemedIcon::new(&icon_name);
        notification.set_icon(&icon);

        if let Some(ref identifier) = identifier {
            application.send_notification(Some(identifier.as_str()), &notification);
        } else {
            application.send_notification(None, &notification);
        }

        info!("Notification delivered via GApplication");

        Ok(())
    }

    fn make_notification_id(&self, scope: &str, key: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        format!(
            "{}.{}.{}.{}",
            self.app_id,
            Self::sanitize_key(scope),
            Self::sanitize_key(key),
            timestamp
        )
    }

    fn portal_priority(priority: gio::NotificationPriority) -> PortalPriority {
        match priority {
            gio::NotificationPriority::Urgent => PortalPriority::Urgent,
            gio::NotificationPriority::High => PortalPriority::High,
            gio::NotificationPriority::Low => PortalPriority::Low,
            _ => PortalPriority::Normal,
        }
    }

    fn sanitize_key(value: &str) -> String {
        value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect()
    }
}

#[derive(Clone)]
struct NotificationPayload {
    identifier: Option<String>,
    title: String,
    body: Option<String>,
    priority: gio::NotificationPriority,
    icon_name: String,
}

#[derive(Clone)]
enum NotificationDispatcher {
    Channel(UiChannelSender<NotificationCommand>),
    Fallback,
}

struct NotificationCommand {
    payload: NotificationPayload,
    completion: oneshot::Sender<anyhow::Result<()>>,
}

fn is_sandboxed() -> bool {
    env::var_os("FLATPAK_ID").is_some()
        || env::var_os("SNAP").is_some()
        || env::var_os("APPIMAGE").is_some()
}

fn resolve_portal_app_id(default_app_id: &str) -> String {
    // If running inside Snap or the Snap desktop entry is present, use its desktop ID so
    // portal grants notification permission. Otherwise, fall back to the provided app ID.
    let snap_desktop = "/var/lib/snapd/desktop/applications/actioneer_actioneer.desktop";
    if env::var_os("SNAP").is_some() || std::path::Path::new(snap_desktop).exists() {
        return "actioneer_actioneer".to_string();
    }

    default_app_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conclusion_text() {
        let manager = NotificationManager::new("test");

        assert_eq!(manager.conclusion_text(Some("success")), "Success ✓");
        assert_eq!(manager.conclusion_text(Some("failure")), "Failed ✗");
        assert_eq!(manager.conclusion_text(Some("cancelled")), "Cancelled");
        assert_eq!(manager.conclusion_text(None), "Completed");
    }
}
