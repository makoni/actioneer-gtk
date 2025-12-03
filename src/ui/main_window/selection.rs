use super::MainWindow;
use crate::api::models::{RateLimitInfo, Repo};
use crate::ui::detail_placeholder::schedule_status_page_update;
use crate::ui::detail_view::{RepoDetailDeps, RepoDetailPane};
use crate::ui::utils::{MainContextChannelExt, update_rate_limit_label};
use gtk4::prelude::WidgetExt;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn};

#[derive(Debug, PartialEq, Eq)]
enum SelectionAction {
    None,
    Select(i64),
    Clear,
}

impl MainWindow {
    pub(super) fn ensure_detail_matches_selection(&self) {
        let action = {
            let selected_id = *self.selected_repo_id.lock();
            let active_id = self
                .active_detail
                .borrow()
                .as_ref()
                .map(|pane| pane.repo().id);
            determine_selection_action(selected_id, active_id)
        };

        match action {
            SelectionAction::None => {}
            SelectionAction::Select(repo_id) => {
                let repo = {
                    let repos = self.repos.lock();
                    repos.iter().find(|repo| repo.id == repo_id).cloned()
                };
                if let Some(repo) = repo {
                    let this = self.clone();
                    glib::idle_add_local_once(move || {
                        if *this.handling_selection.lock() {
                            return;
                        }
                        this.handle_repo_selection(Some(repo));
                    });
                } else {
                    let this = self.clone();
                    glib::idle_add_local_once(move || {
                        if *this.handling_selection.lock() {
                            return;
                        }
                        this.handle_repo_selection(None);
                    });
                }
            }
            SelectionAction::Clear => {
                let this = self.clone();
                glib::idle_add_local_once(move || {
                    if *this.handling_selection.lock() {
                        return;
                    }
                    this.handle_repo_selection(None);
                });
            }
        }
    }

    pub(super) fn handle_repo_selection(&self, repo: Option<Repo>) {
        *self.handling_selection.lock() = true;

        match repo {
            Some(repo) => {
                info!("Handling repo selection: {}", repo.full_name);
                let repo_id = repo.id;
                *self.selected_repo_id.lock() = Some(repo_id);
                if let Some(manager) = &self.preferences_manager {
                    let manager = manager.clone();
                    crate::runtime_handle().spawn(async move {
                        if let Err(err) = manager.set_last_selected_repo(Some(repo_id)).await {
                            warn!("Failed to persist selected repo: {}", err);
                        }
                    });
                }
                self.start_background_refresh(repo.clone());
                self.present_repo_detail(repo);
            }
            None => {
                info!("Deselecting repo");
                *self.selected_repo_id.lock() = None;
                if let Some(manager) = &self.preferences_manager {
                    let manager = manager.clone();
                    crate::runtime_handle().spawn(async move {
                        if let Err(err) = manager.set_last_selected_repo(None).await {
                            warn!("Failed to clear selected repo preference: {}", err);
                        }
                    });
                }
                self.stop_background_refresh();
                self.show_detail_placeholder();
            }
        }

        *self.handling_selection.lock() = false;
    }

    pub(super) fn present_repo_detail(&self, repo: Repo) {
        info!("Creating detail pane for: {}", repo.full_name);

        {
            let active = self.active_detail.borrow();
            if let Some(existing_pane) = active.as_ref()
                && existing_pane.repo().id == repo.id
            {
                info!("Pane already exists for this repo, skipping creation");
                return;
            }
        }

        let client_opt = {
            let guard = self.client.lock();
            guard.clone()
        };

        match client_opt {
            Some(client) => {
                let deps = RepoDetailDeps {
                    favorites_manager: self.favorites_manager.clone(),
                    preferences_manager: self.preferences_manager.clone(),
                    cache: self.cache.clone(),
                    favorites: self.favorites.clone(),
                    notification_manager: self.notification_manager.clone(),
                };

                let pane = RepoDetailPane::new(
                    self.window.clone(),
                    repo,
                    Arc::new(Mutex::new(client)),
                    deps,
                );
                let stack = self.detail_stack.clone();
                let active_detail = self.active_detail.clone();

                glib::idle_add_local_once(move || {
                    if let Some(existing_child) = stack.child_by_name("detail") {
                        stack.remove(&existing_child);
                    }

                    let widget = pane.widget();
                    stack.add_named(&widget, Some("detail"));
                    stack.set_visible_child_name("detail");
                    active_detail.borrow_mut().replace(pane);
                });
            }
            None => {
                warn!("Cannot show repository details without an authenticated client");
                self.show_detail_placeholder();
            }
        }
    }

    pub(super) fn show_detail_placeholder(&self) {
        let stack = self.detail_stack.clone();
        let active_detail = self.active_detail.clone();
        let status_page = self.detail_status_page.clone();

        glib::idle_add_local_once(move || {
            if let Some(detail_child) = stack.child_by_name("detail") {
                stack.remove(&detail_child);
            }
            stack.set_visible_child_name("placeholder");
            active_detail.borrow_mut().take();
        });

        schedule_status_page_update(status_page, None);
    }

    pub(super) fn start_background_refresh(&self, _repo: Repo) {
        self.stop_background_refresh();

        let preferences_manager = match &self.preferences_manager {
            Some(manager) => manager.clone(),
            None => return,
        };

        let client_arc = self.client.clone();
        let active_detail = self.active_detail.clone();
        let rate_limit_label = self.rate_limit_label.clone();

        let (sender, receiver) =
            glib::MainContext::default().channel::<()>(glib::Priority::default());

        receiver.attach(None, move |_| {
            if let Some(pane) = active_detail.borrow().as_ref() {
                pane.refresh_workflows_silent();
            }
            glib::ControlFlow::Continue
        });

        let (rate_sender, rate_receiver) =
            glib::MainContext::default().channel::<RateLimitInfo>(glib::Priority::default());

        let rate_label_clone = rate_limit_label.clone();
        rate_receiver.attach(None, move |info| {
            update_rate_limit_label(&rate_label_clone, Some(info));
            glib::ControlFlow::Continue
        });

        let handle = crate::runtime_handle().spawn(async move {
            loop {
                let interval = preferences_manager.get().await.refresh_interval;
                sleep(Duration::from_secs(interval)).await;

                let client_opt = {
                    let guard = client_arc.lock();
                    guard.clone()
                };

                if let Some(client) = client_opt {
                    if sender.send(()).is_err() {
                        break;
                    }

                    if let Some(rate_info) = client.rate_limit_info()
                        && rate_sender.send(rate_info).is_err()
                    {
                        break;
                    }
                } else {
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

    pub(super) fn show_header_loading(&self, loading: bool) {
        let header = self.header_bar.clone();
        let refresh_button = self.refresh_button.clone();
        let spinner_ref = self.header_spinner.clone();

        glib::idle_add_local_once(move || {
            if loading {
                info!("🔄 Showing header loading spinner");
                refresh_button.set_visible(false);

                if let Some(old_spinner) = spinner_ref.borrow_mut().take() {
                    header.remove(&old_spinner);
                }

                let spinner = gtk::Spinner::new();
                spinner.start();
                spinner.set_size_request(24, 24);
                spinner.set_tooltip_text(Some("Loading repositories..."));
                header.pack_start(&spinner);
                spinner.set_visible(true);

                *spinner_ref.borrow_mut() = Some(spinner);
            } else {
                info!("✅ Hiding header loading spinner");
                match spinner_ref.borrow_mut().take() {
                    Some(spinner) => {
                        info!("Removing spinner from header");
                        header.remove(&spinner);
                    }
                    None => {
                        debug!("No header spinner present when hiding header loader");
                    }
                }

                refresh_button.set_visible(true);
            }
        });
    }
}

fn determine_selection_action(selected_id: Option<i64>, active_id: Option<i64>) -> SelectionAction {
    match (selected_id, active_id) {
        (Some(sel), Some(active)) if sel == active => SelectionAction::None,
        (Some(sel), _) => SelectionAction::Select(sel),
        (None, Some(_)) => SelectionAction::Clear,
        _ => SelectionAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{SelectionAction, determine_selection_action};

    #[test]
    fn same_selected_and_active_returns_none() {
        assert_eq!(
            determine_selection_action(Some(1), Some(1)),
            SelectionAction::None
        );
    }

    #[test]
    fn new_selected_id_requests_selection() {
        assert_eq!(
            determine_selection_action(Some(2), Some(1)),
            SelectionAction::Select(2)
        );
        assert_eq!(
            determine_selection_action(Some(3), None),
            SelectionAction::Select(3)
        );
    }

    #[test]
    fn clear_when_active_without_selection() {
        assert_eq!(
            determine_selection_action(None, Some(1)),
            SelectionAction::Clear
        );
    }

    #[test]
    fn no_action_when_none_selected_and_active() {
        assert_eq!(
            determine_selection_action(None, None),
            SelectionAction::None
        );
    }
}
