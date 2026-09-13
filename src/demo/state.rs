use super::data::{DemoData, GitHubErrorResult};
use crate::api::GitHubError;
use crate::api::models::{
    Branch, Job, RateLimitInfo, Repo, Workflow, WorkflowDispatchInput, WorkflowDispatchInputType,
    WorkflowDispatchInputValue, WorkflowRun,
};
use parking_lot::Mutex;
use std::sync::OnceLock;

static DEMO_STATE: OnceLock<Mutex<Option<DemoData>>> = OnceLock::new();

fn store() -> &'static Mutex<Option<DemoData>> {
    DEMO_STATE.get_or_init(|| Mutex::new(None))
}

fn with_data<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&DemoData) -> R,
{
    let guard = store().lock();
    guard.as_ref().map(f)
}

fn with_data_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut DemoData) -> R,
{
    let mut guard = store().lock();
    guard.as_mut().map(f)
}

pub fn enable() -> Vec<Repo> {
    let mut guard = store().lock();
    let data = DemoData::new();
    let repos = data.clone_repos();
    *guard = Some(data);
    repos
}

pub fn disable() {
    let mut guard = store().lock();
    *guard = None;
}

pub fn is_active() -> bool {
    store().lock().is_some()
}

pub fn list_repos() -> Option<Vec<Repo>> {
    with_data(|data| data.clone_repos())
}

pub fn rate_limit_info() -> Option<RateLimitInfo> {
    with_data(|data| data.clone_rate_limit())
}

pub fn is_actions_enabled(owner: &str, repo: &str) -> Option<bool> {
    with_data(|data| data.actions_enabled(owner, repo))
}

pub fn list_branches(owner: &str, repo: &str) -> Option<Vec<Branch>> {
    with_data(|data| data.clone_branches(owner, repo))
}

pub fn list_workflows(owner: &str, repo: &str) -> Option<Vec<Workflow>> {
    with_data(|data| data.clone_workflows(owner, repo))
}

pub fn list_runs(owner: &str, repo: &str, workflow_id: i64) -> Option<Vec<WorkflowRun>> {
    with_data(|data| data.clone_runs(owner, repo, workflow_id))
}

pub fn list_repository_runs(owner: &str, repo: &str) -> Option<Vec<WorkflowRun>> {
    with_data(|data| data.clone_repo_runs(owner, repo))
}

pub fn list_jobs(_owner: &str, _repo: &str, run_id: i64) -> Option<Vec<Job>> {
    with_data(|data| data.clone_jobs(run_id))
}

pub fn job_logs(job_id: i64) -> Option<String> {
    with_data(|data| data.clone_logs(job_id)).flatten()
}

pub fn dispatch_workflow(
    owner: &str,
    repo: &str,
    workflow_id: i64,
    reference: &str,
) -> GitHubErrorResult<WorkflowRun> {
    with_data_mut(|data| data.add_manual_run(owner, repo, workflow_id, reference))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub fn workflow_dispatch_inputs() -> Vec<WorkflowDispatchInput> {
    vec![
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
    ]
}

pub fn rerun_workflow(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.update_run_status(owner, repo, run_id, "queued", None))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub fn rerun_failed_jobs(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.update_run_status(owner, repo, run_id, "in_progress", None))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub fn cancel_run(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| {
        data.update_run_status(owner, repo, run_id, "completed", Some("cancelled"))
    })
    .unwrap_or(Err(GitHubError::NotFound))
}
