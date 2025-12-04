/// Workflow operations
use super::error::GitHubError;
use super::http::{GITHUB_API_BASE, ResponseHandler, add_auth_header};
use crate::api::models::{Workflow, WorkflowsResponse};
use reqwest::{Client, StatusCode};
use tracing::info;

/// List workflows for a repository
pub async fn list_workflows(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<Vec<Workflow>, GitHubError> {
    info!("Fetching workflows for {}/{}", owner, repo);

    let request = client.get(format!(
        "{}/repos/{}/{}/actions/workflows",
        GITHUB_API_BASE, owner, repo
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;
    let workflows_response: WorkflowsResponse = response_handler.handle_response(response).await?;
    Ok(workflows_response.workflows)
}

/// Dispatch a workflow
pub async fn dispatch_workflow(
    client: &Client,
    token: &Option<String>,
    owner: &str,
    repo: &str,
    workflow_id: &str,
    ref_name: &str,
    inputs: Option<serde_json::Value>,
) -> Result<(), GitHubError> {
    info!("Dispatching workflow {} on ref {}", workflow_id, ref_name);

    let mut body = serde_json::json!({
        "ref": ref_name
    });

    if let Some(inputs) = inputs {
        body["inputs"] = inputs;
    }

    let request = client
        .post(format!(
            "{}/repos/{}/{}/actions/workflows/{}/dispatches",
            GITHUB_API_BASE, owner, repo, workflow_id
        ))
        .json(&body);

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    if response.status() == StatusCode::NO_CONTENT {
        info!("Workflow dispatched successfully");
        Ok(())
    } else {
        // For error cases, try to parse as JSON error
        Err(GitHubError::ApiError(format!(
            "Failed to dispatch workflow: {}",
            response.status()
        )))
    }
}
