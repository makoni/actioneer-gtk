//! Startup from the cache, and the window's reaction to GitHub being
//! unreachable.
//!
//! On sign-in the sidebar is painted once from the data cache, before the
//! network answers, so the repositories appear at once and stay readable
//! offline. The gateway tracks whether GitHub can be reached; the window shows
//! "Offline" where the rate limit normally is, and reloads when it comes back.

use super::MainWindow;
use crate::domain::counts::determine_actions_state;
use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::models::{RateLimitInfo, Repo};
use crate::ui::main_window::rate_limit::update_rate_limit_label;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::info;

use crate::services::gateway::GitHubGateway;

/// The window's startup and connectivity bookkeeping.
#[derive(Clone, Default)]
pub(super) struct OfflineState {
    /// A repository list request is in flight; another one is not started.
    pub(super) repos_loading: Arc<AtomicBool>,
    /// A reload was asked for while one was in flight — after reconnecting,
    /// say, while a request from before still waits out its timeout. It runs
    /// when the one in flight returns.
    pub(super) reload_pending: Arc<AtomicBool>,
    /// A live repository list has landed since the last sign-in, so a cached
    /// paint arriving now would only paint over newer data.
    live_repos_loaded: Arc<AtomicBool>,
    /// Bumped on every sign-in and sign-out; a cached paint started in another
    /// session is dropped.
    session: Arc<AtomicU64>,
    /// Forwards the gateway's connectivity to this window. Aborted on sign-out
    /// and when the window goes away: the forwarding closure owns a clone of
    /// the window.
    connectivity_task: Rc<RefCell<Option<tokio::task::JoinHandle<()>>>>,
    /// Cached paints that reached the window, accepted or not — lets a test
    /// wait for a rejection instead of guessing how long one takes.
    #[cfg(test)]
    pub(super) cached_paint_attempts: Arc<AtomicU64>,
}

impl OfflineState {
    pub(super) fn begin_session(&self) {
        self.session.fetch_add(1, Ordering::AcqRel);
        self.live_repos_loaded.store(false, Ordering::Release);
        self.repos_loading.store(false, Ordering::Release);
        self.reload_pending.store(false, Ordering::Release);
    }

    pub(super) fn current_session(&self) -> u64 {
        self.session.load(Ordering::Acquire)
    }

    pub(super) fn note_live_repos(&self) {
        self.live_repos_loaded.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(super) fn is_watching(&self) -> bool {
        self.connectivity_task.borrow().is_some()
    }

    pub(super) fn stop_watching(&self) {
        if let Some(task) = self.connectivity_task.borrow_mut().take() {
            task.abort();
        }
    }
}

/// The rate-limit readout, or "Offline" while GitHub cannot be reached.
pub(super) fn render_rate_limit(
    label: &gtk::Label,
    client: &Arc<Mutex<Option<GitHubGateway>>>,
    info: Option<RateLimitInfo>,
) {
    let offline = client
        .lock()
        .as_ref()
        .is_some_and(|gateway| gateway.is_offline());
    if offline {
        label.set_text(tr("Offline").as_str());
    } else {
        update_rate_limit_label(label, info);
    }
}

impl MainWindow {
    /// Paints the sidebar from the cache, once per sign-in, if the network has
    /// not answered first.
    pub(super) fn paint_cached_repositories(&self) {
        let Some(client) = self.client.lock().clone() else {
            return;
        };
        let session = self.offline.session.load(Ordering::Acquire);
        let preferences = self.preferences_manager.clone();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<(Arc<Vec<Repo>>, Option<i64>)>(glib::Priority::default());
        let this = self.clone();
        receiver.attach(None, move |(repos, last_selected)| {
            this.apply_cached_repositories(session, &repos, last_selected);
            glib::ControlFlow::Break
        });

        crate::runtime::handle().spawn(async move {
            client.cache_ready().await;
            let Some(cached) = client.cached_repos() else {
                return;
            };
            // Read here rather than relied on: the selection to restore must
            // be known before the list is painted, or the first repository
            // would be opened — and saved as the last selection.
            let last_selected = match preferences {
                Some(manager) => manager.get().await.last_selected_repo_id,
                None => None,
            };
            let _ = sender.send((cached.value, last_selected));
        });
    }

    fn apply_cached_repositories(&self, session: u64, repos: &[Repo], last_selected: Option<i64>) {
        #[cfg(test)]
        self.offline
            .cached_paint_attempts
            .fetch_add(1, Ordering::AcqRel);
        if self.offline.session.load(Ordering::Acquire) != session
            || self.offline.live_repos_loaded.load(Ordering::Acquire)
            || !self.repos.lock().is_empty()
        {
            return;
        }
        info!("Showing {} cached repositories", repos.len());

        {
            let mut selected = self.selected_repo_id.lock();
            if selected.is_none() {
                *selected = last_selected;
            }
        }
        *self.repos.lock() = repos.to_vec();
        {
            let mut actions = self.actions_states.lock();
            for repo in repos {
                actions
                    .entry(repo.id)
                    .or_insert_with(|| determine_actions_state(repo.permissions.as_ref()));
            }
        }
        {
            let mut counts = self.workflow_counts.lock();
            for repo in repos {
                counts.entry(repo.id).or_default();
            }
        }
        // No status refresh here: it stamps `actions_checked_at` before its
        // request, and offline that would hold back the real check for minutes.
        self.schedule_repo_list_refresh();
    }

    /// Follows the gateway's connectivity: the label shows "Offline", and
    /// coming back online reloads the repositories and the open repository.
    pub(super) fn watch_connectivity(&self) {
        self.offline.stop_watching();
        let Some(mut connectivity) = self
            .client
            .lock()
            .as_ref()
            .and_then(|gateway| gateway.connectivity())
        else {
            return;
        };

        let (sender, receiver) =
            glib::MainContext::default().channel::<bool>(glib::Priority::default());
        let this = self.clone();
        receiver.attach(None, move |offline| {
            // This closure owns a clone of the window. The language-change
            // rebuild stops the forwarder explicitly; a window destroyed some
            // other way is let go of here, at the next change.
            let window: &gtk::Window = this.window.upcast_ref();
            if !gtk::Window::list_toplevels().contains(&window.clone().upcast()) {
                this.offline.stop_watching();
                return glib::ControlFlow::Break;
            }
            this.update_rate_limit_display(this.rate_limit_info.lock().clone());
            let active_pane = this.active_detail.borrow().clone();
            if offline {
                if let Some(pane) = active_pane {
                    pane.mark_offline();
                }
            } else {
                info!("GitHub is reachable again; reloading");
                this.load_repositories();
                if let Some(pane) = active_pane {
                    pane.reload();
                }
            }
            glib::ControlFlow::Continue
        });

        let task = crate::runtime::handle().spawn(async move {
            while connectivity.changed().await.is_ok() {
                let offline = *connectivity.borrow_and_update();
                if sender.send(offline).is_err() {
                    break;
                }
            }
        });
        *self.offline.connectivity_task.borrow_mut() = Some(task);
    }
}
