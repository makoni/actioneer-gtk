/// Background task: Check repository status (Actions enabled, workflow counts)
use super::super::state::{RepoActionsState, WorkflowStatusCounts};
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::models::Repo;
use crate::services::gateway::GitHubGateway;
use crate::ui::sidebar::gather_workflow_status_counts;
use gtk4::glib;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

const MAX_REPOS_FOR_STATUS: usize = 20;

/// Spawn tasks to check repository status for all repos
/// Runs checks in parallel (5 concurrent) on tokio runtime
pub fn spawn_repo_status_tasks<F>(
    repos: Vec<Repo>,
    client: GitHubGateway,
    actions_state: Arc<Mutex<HashMap<i64, RepoActionsState>>>,
    workflow_state: Arc<Mutex<HashMap<i64, WorkflowStatusCounts>>>,
    checked_state: Arc<Mutex<HashMap<i64, Instant>>>,
    on_complete: F,
) where
    F: FnOnce() + 'static,
{
    if repos.is_empty() {
        glib::idle_add_local_once(on_complete);
        return;
    }

    let (sender, receiver) = glib::MainContext::default().channel::<()>(glib::Priority::default());
    let mut callback = Some(on_complete);

    receiver.attach(None, move |_| {
        if let Some(done) = callback.take() {
            done();
        }
        glib::ControlFlow::Break
    });

    let repos: Vec<Repo> = repos.into_iter().take(MAX_REPOS_FOR_STATUS).collect();

    crate::runtime::handle().spawn(async move {
        use futures::stream::{self, StreamExt};

        stream::iter(repos)
            .for_each_concurrent(5, move |repo| {
                let client = client.clone();
                let actions_state = actions_state.clone();
                let workflow_state = workflow_state.clone();
                let checked_state = checked_state.clone();

                async move {
                    let owner = repo.owner.login.clone();
                    let repo_name = repo.name.clone();
                    let repo_id = repo.id;

                    // Check actions enabled
                    match client.is_actions_enabled(&owner, &repo_name).await {
                        Ok(enabled) => {
                            let mut actions = actions_state.lock();
                            actions.insert(
                                repo_id,
                                if enabled {
                                    RepoActionsState::Enabled
                                } else {
                                    RepoActionsState::Disabled
                                },
                            );
                        }
                        Err(err) => {
                            warn!(
                                "Failed to fetch actions status for {}/{}: {}",
                                owner, repo_name, err
                            );
                        }
                    }

                    // Get workflow counts
                    match gather_workflow_status_counts(&client, &owner, &repo_name).await {
                        Ok(counts) => {
                            let mut workflows = workflow_state.lock();
                            workflows.insert(repo_id, counts);
                        }
                        Err(err) => {
                            warn!(
                                "Failed to fetch workflow status for {}/{}: {}",
                                owner, repo_name, err
                            );
                        }
                    }

                    let mut checked = checked_state.lock();
                    checked.insert(repo_id, Instant::now());
                }
            })
            .await;

        let _ = sender.send(());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gateway::GitHubGateway;
    use crate::ui::test_helpers::run_gtk_test;
    use std::cell::Cell;
    use std::rc::Rc;

    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..200 {
            if !context.iteration(false) {
                break;
            }
        }
    }

    type ActionsState = Arc<Mutex<HashMap<i64, RepoActionsState>>>;
    type WorkflowState = Arc<Mutex<HashMap<i64, WorkflowStatusCounts>>>;
    type CheckedState = Arc<Mutex<HashMap<i64, Instant>>>;

    fn states() -> (ActionsState, WorkflowState, CheckedState) {
        (
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        )
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn an_empty_repository_list_still_reports_completion() {
        run_gtk_test("repo_status_empty_completes", || {
            crate::runtime::init_test_runtime();
            let (actions, workflows, checked) = states();
            let done = Rc::new(Cell::new(false));
            let flag = done.clone();

            // The early return takes its own path to `on_complete`; a caller
            // that never hears back leaves a spinner running forever.
            spawn_repo_status_tasks(
                Vec::new(),
                GitHubGateway::demo(),
                actions.clone(),
                workflows,
                checked,
                move || flag.set(true),
            );
            pump();

            assert!(done.get(), "on_complete must fire for an empty list");
            assert!(actions.lock().is_empty(), "nothing to record");
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_fan_out_is_capped_at_twenty_repositories() {
        // A guard on the constant rather than on behaviour: the cap is what
        // keeps a large account from opening one request per repository, and it
        // is easy to raise by accident.
        assert_eq!(MAX_REPOS_FOR_STATUS, 20);
    }
}
