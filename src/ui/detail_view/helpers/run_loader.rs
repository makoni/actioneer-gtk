use super::runs::LoadRunsParams;
use super::runs::load_workflow_runs;
use gtk4::glib;
use parking_lot::Mutex;
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Centralized dispatcher for workflow run loads to coalesce background refreshes
/// and prevent duplicate requests.
#[derive(Clone)]
pub(crate) struct RunLoadService {
    pending: Rc<Mutex<VecDeque<LoadRunsParams>>>,
    scheduled: Rc<Cell<bool>>,
    debounce: Duration,
    workflows_last_loaded: Arc<Mutex<HashMap<i64, Instant>>>,
    workflows_loading: Arc<Mutex<HashSet<i64>>>,
}

impl RunLoadService {
    pub(crate) fn new(
        workflows_last_loaded: Arc<Mutex<HashMap<i64, Instant>>>,
        workflows_loading: Arc<Mutex<HashSet<i64>>>,
    ) -> Self {
        Self {
            pending: Rc::new(Mutex::new(VecDeque::new())),
            scheduled: Rc::new(Cell::new(false)),
            debounce: Duration::from_millis(1200),
            workflows_last_loaded,
            workflows_loading,
        }
    }

    /// Queue a run load request; coalesces by workflow ID and debounces background calls.
    pub(crate) fn request(&self, params: LoadRunsParams) {
        let workflow_id = params.workflow_id;
        {
            let mut queue = self.pending.lock();
            // Replace any pending request for the same workflow with the latest parameters.
            if let Some(pos) = queue.iter().position(|p| p.workflow_id == workflow_id) {
                info!(
                    workflow_id,
                    "Replacing pending run-load request with latest parameters"
                );
                queue.remove(pos);
            }
            queue.push_back(params);
            info!(workflow_id, queued = queue.len(), "Queued run-load request");
        }

        self.schedule_process();
    }

    fn schedule_process(&self) {
        if self.scheduled.replace(true) {
            return;
        }

        let service = self.clone();
        glib::idle_add_local_once(move || {
            service.process_queue();
        });
    }

    fn process_queue(&self) {
        self.scheduled.set(false);

        let mut ready: Vec<LoadRunsParams> = Vec::new();
        let mut needs_retry = false;
        let now = Instant::now();

        {
            let mut queue = self.pending.lock();
            let mut to_remove: Vec<usize> = Vec::new();

            for (idx, params) in queue.iter().enumerate() {
                let workflow_id = params.workflow_id;

                if params.background {
                    let last_loaded_guard = self.workflows_last_loaded.lock();
                    if let Some(previous) = last_loaded_guard.get(&workflow_id)
                        && now.duration_since(*previous) < self.debounce
                    {
                        info!(workflow_id, "Skipped run load (debounced in service)");
                        needs_retry = true;
                        continue;
                    }
                }

                if self.workflows_loading.lock().contains(&workflow_id) {
                    info!(workflow_id, "Skipped run load (already in flight)");
                    needs_retry = true;
                    continue;
                }

                to_remove.push(idx);
            }

            // Remove in reverse order so indices stay valid
            for idx in to_remove.into_iter().rev() {
                if let Some(params) = queue.remove(idx) {
                    ready.push(params);
                }
            }

            needs_retry |= !queue.is_empty();
        }

        if !ready.is_empty() {
            for mut params in ready.into_iter() {
                // Refresh expanded run state at dispatch time so queued requests don't
                // reopen or collapse rows based on stale queued state.
                let latest_expanded: std::collections::HashSet<i64> =
                    params.run_list.expanded_run_ids();
                params.expanded_run_ids = latest_expanded.into_iter().collect();

                debug!(
                    workflow_id = params.workflow_id,
                    background = params.background,
                    "Dispatching workflow run load via RunLoadService"
                );
                info!(
                    workflow_id = params.workflow_id,
                    background = params.background,
                    "Dispatching run-load request"
                );
                load_workflow_runs(params);
            }
        }

        if needs_retry {
            let service = self.clone();
            glib::timeout_add_local_once(self.debounce, move || {
                service.process_queue();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Repo, User};
    use crate::services::gateway::GitHubGateway;
    use crate::ui::detail_view::helpers::runs::list::test_run_list_model;
    use crate::ui::test_helpers::run_gtk_test;
    use libadwaita as adw;

    fn params(workflow_id: i64, background: bool) -> LoadRunsParams {
        let app = adw::Application::builder()
            .application_id("me.spaceinbox.actioneer.RunLoaderTest")
            .build();
        LoadRunsParams {
            client: Arc::new(Mutex::new(GitHubGateway::demo())),
            owner: "demo-org".into(),
            repo: "actioneer-demo-app".into(),
            repo_model: Repo {
                id: 1,
                name: "actioneer-demo-app".into(),
                full_name: "demo-org/actioneer-demo-app".into(),
                owner: User {
                    login: "demo-org".into(),
                },
                is_private: false,
                permissions: None,
                default_branch: Some("main".into()),
            },
            workflow_id,
            workflow_name: format!("Workflow {workflow_id}"),
            run_list: test_run_list_model(),
            parent_window: adw::ApplicationWindow::new(&app),
            expander: gtk4::Expander::new(None),
            toast_overlay: adw::ToastOverlay::new(),
            job_contexts: Default::default(),
            expanded_run_ids: Vec::new(),
            workflows_with_active: Arc::new(Mutex::new(HashSet::new())),
            workflows_last_loaded: Arc::new(Mutex::new(HashMap::new())),
            workflows_loading: Arc::new(Mutex::new(HashSet::new())),
            background,
            run_digests: Default::default(),
            notification_manager: None,
            preferences_manager: None,
            run_filters: Default::default(),
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_second_request_for_the_same_workflow_replaces_the_first() {
        run_gtk_test("run_loader_coalesces", || {
            let service = RunLoadService::new(
                Arc::new(Mutex::new(HashMap::new())),
                Arc::new(Mutex::new(HashSet::new())),
            );

            // Expanding a row twice in quick succession, or a refresh landing on
            // top of a manual expand, must not queue the work twice — and the
            // survivor has to be the *second* set of parameters, because the
            // first describes a state the user has already moved on from.
            service.request(params(11, false));
            service.request(params(11, true));
            assert_eq!(service.pending.lock().len(), 1, "coalesced by workflow id");
            assert!(
                service.pending.lock()[0].background,
                "the later parameters replaced the earlier ones"
            );

            service.request(params(22, false));
            assert_eq!(
                service.pending.lock().len(),
                2,
                "a different workflow queues separately"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_workflow_already_in_flight_is_left_queued() {
        run_gtk_test("run_loader_in_flight_guard", || {
            // The dispatch at the end of this test spawns onto the runtime.
            crate::runtime::init_test_runtime();
            let loading = Arc::new(Mutex::new(HashSet::from([11_i64])));
            let service =
                RunLoadService::new(Arc::new(Mutex::new(HashMap::new())), loading.clone());

            service.request(params(11, false));
            service.process_queue();

            // Still queued: dispatching now would run two loads for the same
            // workflow and let the slower one overwrite the newer result.
            assert_eq!(
                service.pending.lock().len(),
                1,
                "an in-flight workflow stays queued for the retry"
            );

            loading.lock().clear();
            service.process_queue();
            assert!(
                service.pending.lock().is_empty(),
                "once the flight ends the request is dispatched"
            );
        });
    }
}
