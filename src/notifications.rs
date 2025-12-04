use crate::APP_ICON_NAME;
use anyhow::anyhow;
use gtk4::prelude::ApplicationExt;
use gtk4::{gio, glib};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

const DEFAULT_ICON_NAME: &str = APP_ICON_NAME;

/// Notification manager for Linux using XDG Desktop Notifications
/// Similar to NotificationManager.swift in macOS version
#[derive(Clone)]
pub struct NotificationManager {
    app_id: String,
    icon_name: String,
}

impl NotificationManager {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self::with_icon(app_id, DEFAULT_ICON_NAME)
    }

    pub fn with_icon(app_id: impl Into<String>, icon_name: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            icon_name: icon_name.into(),
        }
    }

    /// Send a notification for completed workflow
    pub async fn notify_workflow_completed(
        &self,
        workflow_name: &str,
        run_title: &str,
        status: &str,
        conclusion: Option<&str>,
    ) -> anyhow::Result<()> {
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

        Self::dispatch_via_main_context(payload).await
    }

    async fn dispatch_via_main_context(payload: NotificationPayload) -> anyhow::Result<()> {
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
                result
            }
            Err(_) => {
                error!("Notification dispatcher dropped before completion");
                Err(anyhow!("Notification dispatcher dropped before sending"))
            }
        }
    }

    fn deliver_notification(
        application: &gio::Application,
        payload: NotificationPayload,
    ) -> anyhow::Result<()> {
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

        let icon = gio::ThemedIcon::new(&icon_name);
        notification.set_icon(&icon);

        if let Some(ref identifier) = identifier {
            application.send_notification(Some(identifier.as_str()), &notification);
        } else {
            application.send_notification(None, &notification);
        }

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
