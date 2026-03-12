use super::MainWindow;
use crate::api::models::Repo;
use crate::ui::utils::{MainContextChannelExt, update_rate_limit_label};
use gtk4::glib;
use tokio::time::{Duration, sleep};

fn refresh_delay_for_interval(interval: u64) -> Option<Duration> {
    (interval > 0).then(|| Duration::from_secs(interval))
}

impl MainWindow {
    pub(super) fn start_background_refresh(&self, _repo: Repo) {
        self.stop_background_refresh();
        let preferences_manager = match &self.preferences_manager {
            Some(manager) => manager.clone(),
            None => return,
        };
        let client_arc = self.client.clone();
        let rate_limit_label = self.rate_limit_label.clone();

        let (rate_sender, rate_receiver) =
            glib::MainContext::default().channel(glib::Priority::default());

        rate_receiver.attach(None, move |info| {
            update_rate_limit_label(&rate_limit_label, info);
            glib::ControlFlow::Continue
        });

        let handle = crate::runtime_handle().spawn(async move {
            let mut updates = preferences_manager.subscribe();
            loop {
                let interval = preferences_manager.get().await.refresh_interval;
                let Some(delay) = refresh_delay_for_interval(interval) else {
                    if updates.changed().await.is_err() {
                        break;
                    }
                    continue;
                };

                sleep(delay).await;

                let client_opt = {
                    let guard = client_arc.lock();
                    guard.clone()
                };

                let Some(client) = client_opt else {
                    break;
                };

                if rate_sender.send(client.rate_limit_info()).is_err() {
                    break;
                }
            }
        });

        *self.background_refresh_task.lock() = Some(handle);
    }

    pub(super) fn stop_background_refresh(&self) {
        if let Some(handle) = self.background_refresh_task.lock().take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::refresh_delay_for_interval;
    use std::time::Duration;

    #[test]
    fn refresh_delay_handles_disabled_interval() {
        assert_eq!(refresh_delay_for_interval(0), None);
        assert_eq!(
            refresh_delay_for_interval(10),
            Some(Duration::from_secs(10))
        );
    }
}
