//! The demo data source, as an alternate backend behind the gateway.
//!
//! This replaces the process-global `OnceLock<Mutex<Option<DemoData>>>` the
//! module used to hide behind free functions. Demo mode is now "a `DemoBackend`
//! was constructed and installed", not "a global flag is set".
//!
//! The fixtures live behind `Arc<Mutex<_>>` and the struct is `Clone`. That is
//! load-bearing, not incidental: every UI call site reads the gateway out of its
//! slot with `slot.lock().clone()`, because a `parking_lot::MutexGuard` cannot
//! be held across an `.await` in a `Send` future. If the backend owned its data
//! by value, each clone would mutate a copy nobody reads and
//! `dispatch_workflow`, `cancel_run` and the re-runs would silently stop
//! working. `tests/characterization_demo.rs` covers exactly that.

use super::data::DemoData;
use crate::api::GitHubError;
use crate::api::models::{
    Branch, Job, RateLimitInfo, Repo, Workflow, WorkflowDispatchInput, WorkflowDispatchInputType,
    WorkflowDispatchInputValue, WorkflowRun,
};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct DemoBackend {
    data: Arc<Mutex<DemoData>>,
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoBackend {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(DemoData::new())),
        }
    }

    /// The repositories and rate limit to seed the UI with, synchronously.
    ///
    /// `enter_demo_mode` runs on the GTK main thread and populates the sidebar
    /// in place; it has nothing to await on. This is the one sync entry point,
    /// and it mirrors what `demo::enable()` + `demo::rate_limit_info()`
    /// returned together.
    pub fn seed(&self) -> (Vec<Repo>, Option<RateLimitInfo>) {
        let data = self.data.lock();
        (data.clone_repos(), Some(data.clone_rate_limit()))
    }

    pub fn rate_limit_info(&self) -> Option<RateLimitInfo> {
        Some(self.data.lock().clone_rate_limit())
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>, GitHubError> {
        Ok(self.data.lock().clone_repos())
    }

    pub async fn is_actions_enabled(&self, owner: &str, repo: &str) -> Result<bool, GitHubError> {
        Ok(self.data.lock().actions_enabled(owner, repo))
    }

    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>, GitHubError> {
        Ok(self.data.lock().clone_branches(owner, repo))
    }

    pub async fn list_workflows(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<Workflow>, GitHubError> {
        Ok(self.data.lock().clone_workflows(owner, repo))
    }

    pub async fn dispatch_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        ref_name: &str,
        _inputs: Option<serde_json::Value>,
    ) -> Result<Option<WorkflowRun>, GitHubError> {
        // Demo inputs are ignored; the run is simulated immediately.
        let Ok(id) = workflow_id.parse::<i64>() else {
            return Err(GitHubError::NotFound);
        };
        let mut data = self.data.lock();
        data.add_manual_run(owner, repo, id, ref_name).map(Some)
    }

    pub async fn get_workflow_dispatch_inputs(
        &self,
        _owner: &str,
        _repo: &str,
        _workflow_path: &str,
        _reference: Option<&str>,
    ) -> Result<Vec<WorkflowDispatchInput>, GitHubError> {
        Ok(vec![
            WorkflowDispatchInput {
                name: "environment".to_string(),
                description: Some("Target environment for the demo run".to_string()),
                required: true,
                input_type: WorkflowDispatchInputType::Choice,
                default_value: Some(WorkflowDispatchInputValue::String("staging".to_string())),
                options: vec!["staging".to_string(), "production".to_string()],
            },
            WorkflowDispatchInput {
                name: "dry_run".to_string(),
                description: Some("Run in simulation mode".to_string()),
                required: false,
                input_type: WorkflowDispatchInputType::Boolean,
                default_value: Some(WorkflowDispatchInputValue::Boolean(true)),
                options: Vec::new(),
            },
        ])
    }

    pub async fn list_runs(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: i64,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        Ok(self.data.lock().clone_runs(owner, repo, workflow_id))
    }

    pub async fn list_repository_runs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        Ok(self.data.lock().clone_repo_runs(owner, repo))
    }

    pub async fn rerun_workflow(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        self.data
            .lock()
            .update_run_status(owner, repo, run_id, "queued", None)
    }

    pub async fn rerun_failed_jobs(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        self.data
            .lock()
            .update_run_status(owner, repo, run_id, "in_progress", None)
    }

    pub async fn cancel_run(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        self.data
            .lock()
            .update_run_status(owner, repo, run_id, "completed", Some("cancelled"))
    }

    pub async fn list_jobs(
        &self,
        _owner: &str,
        _repo: &str,
        run_id: i64,
    ) -> Result<Vec<Job>, GitHubError> {
        Ok(self.data.lock().clone_jobs(run_id))
    }

    pub async fn get_job_logs(
        &self,
        _owner: &str,
        _repo: &str,
        job_id: i64,
    ) -> Result<String, GitHubError> {
        self.data
            .lock()
            .clone_logs(job_id)
            .ok_or(GitHubError::NotFound)
    }
}
