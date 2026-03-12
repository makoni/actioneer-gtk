use super::MainWindow;
use crate::api::GitHubError;
use crate::api::models::{RateLimitInfo, Repo, RepoPermissions};
use crate::ui::sidebar::{RepoListRenderContext, rebuild_repo_list};
use crate::ui::state::RepoActionsState;
use crate::ui::tasks::repo_status;
use crate::ui::utils::{MainContextChannelExt, update_rate_limit_label};
use gtk4::glib;
use std::time::Instant;
use tracing::{error, info};

impl MainWindow {
    pub(super) fn load_repositories(&self) {
        let client_opt = {
            let guard = self.client.lock();
            guard.clone()
        };

        if let Some(client) = client_opt {
            info!("Starting to load repositories...");

            self.show_header_loading(true);

            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<(Result<Vec<Repo>, GitHubError>, Option<RateLimitInfo>)>(
                        glib::Priority::default(),
                    );
            let this = self.clone();

            receiver.attach(None, move |(repos_result, rate_info)| {
                this.show_header_loading(false);

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

            crate::runtime_handle().spawn(async move {
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
        {
            let mut repos_guard = self.repos.lock();
            *repos_guard = repos.clone();
        }

        {
            let mut actions = self.actions_states.lock();
            actions.retain(|repo_id, _| repos.iter().any(|repo| repo.id == *repo_id));
            for repo in &repos {
                let state = determine_actions_state(repo.permissions.as_ref());
                actions.entry(repo.id).or_insert(state);
            }
        }

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
        glib::idle_add_local_once(move || update_rate_limit_label(&label, info.clone()));
    }

    pub(super) fn schedule_repo_list_refresh(&self) {
        let repos_snapshot = self.repos.lock().clone();
        let favorites_snapshot = self.favorites.lock().clone();
        let actions_snapshot = self.actions_states.lock().clone();
        let workflow_snapshot = self.workflow_counts.lock().clone();
        let selected = *self.selected_repo_id.lock();
        let favorites_arc = self.favorites.clone();
        let favorites_manager = self.favorites_manager.clone();
        let store = self.repo_store.clone();
        let filter_model = self.repo_filter_model.clone();
        let selection = self.repo_selection.clone();

        glib::idle_add_local_once(move || {
            let context = RepoListRenderContext {
                repos: repos_snapshot,
                favorites_snapshot,
                actions_snapshot,
                workflow_snapshot,
                favorites_state: favorites_arc,
                favorites_manager,
            };

            rebuild_repo_list(store.clone(), context);
            super::repo_list::restore_sidebar_selection(&selection, &filter_model, selected);
        });
    }
}

fn determine_actions_state(permissions: Option<&RepoPermissions>) -> RepoActionsState {
    match permissions {
        Some(perms) if perms.push || perms.admin => RepoActionsState::Enabled,
        Some(_) => RepoActionsState::Disabled,
        None => RepoActionsState::Unknown,
    }
}

fn selected_repo_for_status_refresh(repos: &[Repo], selected_repo_id: Option<i64>) -> Option<Repo> {
    let selected_repo_id = selected_repo_id?;
    repos
        .iter()
        .find(|repo| repo.id == selected_repo_id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::User;

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
    fn determine_actions_state_prefers_push_or_admin() {
        let perms = RepoPermissions {
            admin: false,
            push: true,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&perms)),
            RepoActionsState::Enabled
        );

        let admin_perms = RepoPermissions {
            admin: true,
            push: false,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&admin_perms)),
            RepoActionsState::Enabled
        );
    }

    #[test]
    fn determine_actions_state_disables_pull_only() {
        let perms = RepoPermissions {
            admin: false,
            push: false,
            pull: true,
        };
        assert_eq!(
            determine_actions_state(Some(&perms)),
            RepoActionsState::Disabled
        );
    }

    #[test]
    fn determine_actions_state_unknown_without_permissions() {
        assert_eq!(determine_actions_state(None), RepoActionsState::Unknown);
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
