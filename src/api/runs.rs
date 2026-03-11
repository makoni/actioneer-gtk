/// Workflow run operations
use super::error::GitHubError;
use super::http::{GITHUB_API_BASE, ResponseHandler, add_auth_header};
use crate::api::models::{WorkflowRun, WorkflowRunsResponse};
use reqwest::Client;
use tracing::{debug, info};

const REPOSITORY_RUNS_PAGE_SIZE: usize = 100;

/// List workflow runs
pub async fn list_runs(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
    workflow_id: i64,
) -> Result<Vec<WorkflowRun>, GitHubError> {
    info!("Fetching runs for workflow {}", workflow_id);
    debug!(
        owner,
        repo, workflow_id, "Requesting runs without HTTP ETag caching to avoid stale results"
    );

    let request = client
        .get(format!(
            "{}/repos/{}/{}/actions/workflows/{}/runs",
            GITHUB_API_BASE, owner, repo, workflow_id
        ))
        .query(&[("per_page", "50")]);

    let request = add_auth_header(request, token);
    // Runs are highly dynamic; always fetch fresh data rather than relying on
    // cached ETags.
    let request = response_handler.apply_cache_headers(request, None);
    let response = request.send().await?;
    let runs_response: WorkflowRunsResponse =
        response_handler.handle_response(response, None).await?;
    Ok(runs_response.workflow_runs)
}

/// List recent workflow runs for a repository
pub async fn list_repository_runs(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<Vec<WorkflowRun>, GitHubError> {
    info!("Fetching repository runs for {}/{}", owner, repo);

    let request = client
        .get(format!(
            "{}/repos/{}/{}/actions/runs",
            GITHUB_API_BASE, owner, repo
        ))
        .query(&[("per_page", REPOSITORY_RUNS_PAGE_SIZE.to_string())]);

    let request = add_auth_header(request, token);
    let request = response_handler.apply_cache_headers(request, None);
    let response = request.send().await?;
    let runs_response: WorkflowRunsResponse =
        response_handler.handle_response(response, None).await?;
    Ok(runs_response.workflow_runs)
}

/// Rerun a workflow
pub async fn rerun_workflow(
    client: &Client,
    token: &Option<String>,
    owner: &str,
    repo: &str,
    run_id: i64,
) -> Result<(), GitHubError> {
    info!("Rerunning workflow run {}", run_id);

    let request = client.post(format!(
        "{}/repos/{}/{}/actions/runs/{}/rerun",
        GITHUB_API_BASE, owner, repo, run_id
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(GitHubError::ApiError(format!(
            "Failed to rerun workflow: {}",
            response.status()
        )))
    }
}

/// Rerun failed jobs in a workflow run
pub async fn rerun_failed_jobs(
    client: &Client,
    token: &Option<String>,
    owner: &str,
    repo: &str,
    run_id: i64,
) -> Result<(), GitHubError> {
    info!("Rerunning failed jobs for run {}", run_id);

    let request = client.post(format!(
        "{}/repos/{}/{}/actions/runs/{}/rerun-failed-jobs",
        GITHUB_API_BASE, owner, repo, run_id
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(GitHubError::ApiError(format!(
            "Failed to rerun failed jobs: {}",
            response.status()
        )))
    }
}

/// Cancel a workflow run
pub async fn cancel_run(
    client: &Client,
    token: &Option<String>,
    owner: &str,
    repo: &str,
    run_id: i64,
) -> Result<(), GitHubError> {
    use reqwest::StatusCode;

    info!("Cancelling run {}", run_id);

    let request = client.post(format!(
        "{}/repos/{}/{}/actions/runs/{}/cancel",
        GITHUB_API_BASE, owner, repo, run_id
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    if response.status() == StatusCode::ACCEPTED {
        info!("Run cancelled successfully");
        Ok(())
    } else {
        Err(GitHubError::ApiError(format!(
            "Failed to cancel run: {}",
            response.status()
        )))
    }
}
