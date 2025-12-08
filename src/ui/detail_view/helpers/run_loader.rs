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
                // collapse rows the user expanded after the request was enqueued.
                let latest_expanded: std::collections::HashSet<i64> =
                    params.run_list.expanded_run_ids();
                if !latest_expanded.is_empty() {
                    params.expanded_run_ids = latest_expanded.into_iter().collect();
                }

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
