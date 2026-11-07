use super::MainWindow;
use crate::domain::counts::{
    RepoActionsState, determine_actions_state, selected_repo_for_status_refresh,
};
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::GitHubError;
use crate::services::api::models::{RateLimitInfo, Repo};
use crate::ui::main_window::offline::render_rate_limit;
use crate::ui::sidebar::{RepoListRenderContext, rebuild_repo_list};
use crate::ui::tasks::repo_status;
use gtk4::glib;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tracing::{error, info};

impl MainWindow {
    pub(super) fn load_repositories(&self) {
        let client_opt = {
            let guard = self.client.lock();
            guard.clone()
        };

        if let Some(client) = client_opt {
            // Connectivity flapping (VPN, docker, Wi-Fi roaming) can ask for a
            // reload many times in a row; one request at a time is enough.
            if self.offline.repos_loading.swap(true, Ordering::AcqRel) {
                info!("Repository list already loading; reloading once it returns");
                self.offline.reload_pending.store(true, Ordering::Release);
                return;
            }
            info!("Starting to load repositories...");
            let session = self.offline.current_session();

            self.show_header_loading(true);

            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<(Result<Vec<Repo>, GitHubError>, Option<RateLimitInfo>)>(
                        glib::Priority::default(),
                    );
            let this = self.clone();

            receiver.attach(None, move |(repos_result, rate_info)| {
                // A sign-out or another sign-in since: the answer belongs to a
                // session that is over, and the flag was reset with it.
                if this.offline.current_session() != session {
                    return glib::ControlFlow::Break;
                }
                this.offline.repos_loading.store(false, Ordering::Release);
                this.show_header_loading(false);
                if this.offline.reload_pending.swap(false, Ordering::AcqRel) {
                    // A reload was asked for meanwhile. A successful answer is
                    // still shown; either way the reload runs after it.
                    if let Ok(repos) = repos_result {
                        this.refresh_repository_view(repos, rate_info);
                    }
                    this.load_repositories();
                    return glib::ControlFlow::Break;
                }

                match repos_result {
                    Ok(repos) => {
                        info!("✅ Loaded {} repositories, updating UI", repos.len());
                        this.refresh_repository_view(repos, rate_info);
                    }
                    Err(e) => {
                        error!("Failed to load repositories: {}", e);
                        if matches!(e, GitHubError::AuthenticationFailed) {
                            this.handle_auth_failure();
                        }
                        if let Some(info) = rate_info {
                            this.update_rate_limit_display(Some(info));
                        }
                    }
                }

                glib::ControlFlow::Break
            });

            crate::runtime::handle().spawn(async move {
                let repos_result = client.list_repos().await;
                let rate_info = client.rate_limit_info();

                let _ = sender.send((repos_result, rate_info));
            });
        } else {
            error!("No client available to load repositories");
        }
    }

    pub(super) fn refresh_repository_view(
        &self,
        repos: Vec<Repo>,
        rate_info: Option<RateLimitInfo>,
    ) {
        self.offline.note_live_repos();
        {
            let mut repos_guard = self.repos.lock();
            *repos_guard = repos.clone();
        }

        apply_live_actions_states(
            &mut self.actions_states.lock(),
            &self.actions_checked_at.lock(),
            &repos,
        );

        {
            let mut checked = self.actions_checked_at.lock();
            checked.retain(|repo_id, _| repos.iter().any(|repo| repo.id == *repo_id));
        }

        {
            let mut counts = self.workflow_counts.lock();
            counts.retain(|repo_id, _| repos.iter().any(|repo| repo.id == *repo_id));
            for repo in &repos {
                counts.entry(repo.id).or_default();
            }
        }

        {
            let mut info_guard = self.rate_limit_info.lock();
            *info_guard = rate_info.clone();
        }

        if let Some(selected_repo) =
            selected_repo_for_status_refresh(&repos, *self.selected_repo_id.lock())
        {
            self.refresh_repo_status_summaries(std::slice::from_ref(&selected_repo));
        }
        self.schedule_repo_list_refresh();
        self.update_rate_limit_display(rate_info);
        self.ensure_detail_matches_selection();
    }

    pub(super) fn refresh_repo_status_summaries(&self, repos: &[Repo]) {
        let client_opt = {
            let guard = self.client.lock();
            guard.clone()
        };

        let client = match client_opt {
            Some(client) => client,
            None => return,
        };

        let now = Instant::now();
        let mut checked_map = self.actions_checked_at.lock();
        let mut due_repos = Vec::new();

        for repo in repos {
            let last_checked = checked_map.get(&repo.id).copied();
            let recently_checked = last_checked
                .map(|timestamp| now.duration_since(timestamp) < super::REPO_STATUS_TTL)
                .unwrap_or(false);

            if recently_checked {
                continue;
            }

            checked_map.insert(repo.id, now);
            due_repos.push(repo.clone());
        }

        drop(checked_map);

        if due_repos.is_empty() {
            return;
        }

        let actions_state = self.actions_states.clone();
        let workflow_state = self.workflow_counts.clone();
        let checked_state = self.actions_checked_at.clone();
        let this = self.clone();

        repo_status::spawn_repo_status_tasks(
            due_repos,
            client,
            actions_state,
            workflow_state,
            checked_state,
            move || this.schedule_repo_list_refresh(),
        );
    }

    pub(super) fn update_rate_limit_display(&self, info: Option<RateLimitInfo>) {
        let label = self.rate_limit_label.clone();
        let client = self.client.clone();
        // Connectivity is read when the idle runs, not now: a display queued
        // before GitHub went unreachable must not paint over "Offline".
        glib::idle_add_local_once(move || render_rate_limit(&label, &client, info.clone()));
    }

    pub(super) fn schedule_repo_list_refresh(&self) {
        let repos_snapshot = self.repos.lock().clone();
        let actions_snapshot = self.actions_states.lock().clone();
        let workflow_snapshot = self.workflow_counts.lock().clone();
        let favorites_arc = self.favorites.clone();
        let favorites_manager = self.favorites_manager.clone();
        let store = self.repo_store.clone();
        let window = self.clone();

        glib::idle_add_local_once(move || {
            let context = RepoListRenderContext {
                repos: repos_snapshot,
                actions_snapshot,
                workflow_snapshot,
                favorites_state: favorites_arc,
                favorites_manager,
            };

            rebuild_repo_list(store, context);
            window.restore_repo_selection_now();
        });
    }
}

/// Live permissions replace whatever was derived before — possibly from the
/// cache — unless the actions-permissions endpoint has already given a firmer
/// answer for that repository. Repositories no longer listed are dropped.
fn apply_live_actions_states(
    actions: &mut HashMap<i64, RepoActionsState>,
    checked: &HashMap<i64, Instant>,
    repos: &[Repo],
) {
    actions.retain(|repo_id, _| repos.iter().any(|repo| repo.id == *repo_id));
    for repo in repos {
        let state = determine_actions_state(repo.permissions.as_ref());
        if checked.contains_key(&repo.id) {
            actions.entry(repo.id).or_insert(state);
        } else {
            actions.insert(repo.id, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::api::models::User;

    fn repo(id: i64, name: &str) -> Repo {
        Repo {
            id,
            name: name.into(),
            full_name: format!("mak/{name}"),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        }
    }

    #[test]
    fn live_permissions_replace_unconfirmed_states_only() {
        use crate::services::api::models::RepoPermissions;
        let mut with_push = repo(1, "one");
        with_push.permissions = Some(RepoPermissions {
            admin: false,
            push: true,
            pull: true,
        });
        let mut confirmed = repo(2, "two");
        confirmed.permissions = with_push.permissions.clone();

        // Both were painted from the cache as Unknown; 2 has since been checked
        // against the actions-permissions endpoint and found disabled. 3 is gone.
        let mut actions = HashMap::from([
            (1, RepoActionsState::Unknown),
            (2, RepoActionsState::Disabled),
            (3, RepoActionsState::Enabled),
        ]);
        let checked = HashMap::from([(2, Instant::now())]);

        apply_live_actions_states(&mut actions, &checked, &[with_push, confirmed]);

        assert_eq!(actions.get(&1), Some(&RepoActionsState::Enabled));
        assert_eq!(
            actions.get(&2),
            Some(&RepoActionsState::Disabled),
            "a checked answer is firmer than the listed permissions"
        );
        assert!(!actions.contains_key(&3));
    }

    #[test]
    fn selected_repo_status_refresh_targets_only_selected_repo() {
        let repos = vec![repo(1, "one"), repo(2, "two")];
        assert_eq!(
            selected_repo_for_status_refresh(&repos, Some(2))
                .as_ref()
                .map(|repo| repo.id),
            Some(2)
        );
        assert!(selected_repo_for_status_refresh(&repos, Some(99)).is_none());
        assert!(selected_repo_for_status_refresh(&repos, None).is_none());
    }
}
