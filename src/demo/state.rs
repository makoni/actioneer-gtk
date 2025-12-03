use super::data::{DemoData, GitHubErrorResult};
use crate::api::GitHubError;
use crate::api::models::{Branch, Job, RateLimitInfo, Repo, Workflow, WorkflowRun};
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

pub(crate) fn enable() -> Vec<Repo> {
    let mut guard = store().lock();
    let data = DemoData::new();
    let repos = data.clone_repos();
    *guard = Some(data);
    repos
}

pub(crate) fn disable() {
    let mut guard = store().lock();
    *guard = None;
}

pub(crate) fn is_active() -> bool {
    store().lock().is_some()
}

pub(crate) fn list_repos() -> Option<Vec<Repo>> {
    with_data(|data| data.clone_repos())
}

pub(crate) fn rate_limit_info() -> Option<RateLimitInfo> {
    with_data(|data| data.clone_rate_limit())
}

pub(crate) fn is_actions_enabled(owner: &str, repo: &str) -> Option<bool> {
    with_data(|data| data.actions_enabled(owner, repo))
}

pub(crate) fn list_branches(owner: &str, repo: &str) -> Option<Vec<Branch>> {
    with_data(|data| data.clone_branches(owner, repo))
}

pub(crate) fn list_workflows(owner: &str, repo: &str) -> Option<Vec<Workflow>> {
    with_data(|data| data.clone_workflows(owner, repo))
}

pub(crate) fn list_runs(owner: &str, repo: &str, workflow_id: i64) -> Option<Vec<WorkflowRun>> {
    with_data(|data| data.clone_runs(owner, repo, workflow_id))
}

pub(crate) fn list_jobs(_owner: &str, _repo: &str, run_id: i64) -> Option<Vec<Job>> {
    with_data(|data| data.clone_jobs(run_id))
}

pub(crate) fn job_logs(job_id: i64) -> Option<String> {
    with_data(|data| data.clone_logs(job_id)).flatten()
}

pub(crate) fn dispatch_workflow(
    owner: &str,
    repo: &str,
    workflow_id: i64,
    reference: &str,
) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.add_manual_run(owner, repo, workflow_id, reference))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub(crate) fn rerun_workflow(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.update_run_status(owner, repo, run_id, "queued", None))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub(crate) fn rerun_failed_jobs(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.update_run_status(owner, repo, run_id, "in_progress", None))
        .unwrap_or(Err(GitHubError::NotFound))
}

pub(crate) fn cancel_run(owner: &str, repo: &str, run_id: i64) -> GitHubErrorResult<()> {
    with_data_mut(|data| {
        data.update_run_status(owner, repo, run_id, "completed", Some("cancelled"))
    })
    .unwrap_or(Err(GitHubError::NotFound))
}
