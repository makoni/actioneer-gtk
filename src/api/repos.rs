/// Repository operations
use super::error::GitHubError;
use super::http::{GITHUB_API_BASE, ResponseHandler, add_auth_header};
use crate::api::models::{Branch, Repo};
use reqwest::Client;
use tracing::info;

/// Fetch all repositories for the authenticated user
pub async fn list_repos(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
) -> Result<Vec<Repo>, GitHubError> {
    info!("Fetching user repositories");

    let mut page = 1;
    let mut all_repos = Vec::new();

    loop {
        let cache_key = format!(
            "repos:user:page={page}:affiliation=owner,collaborator,organization_member:sort=updated"
        );
        let request = client
            .get(format!("{}/user/repos", GITHUB_API_BASE))
            .query(&[
                ("per_page", "100"),
                ("page", &page.to_string()),
                ("affiliation", "owner,collaborator,organization_member"),
                ("sort", "updated"),
            ]);

        let request = add_auth_header(request, token);
        let request = response_handler.apply_cache_headers(request, Some(cache_key.as_str()));
        let response = request.send().await?;
        let repos_page: Vec<Repo> = response_handler
            .handle_response(response, Some(cache_key.as_str()))
            .await?;
        let count = repos_page.len();
        all_repos.extend(repos_page);

        if count < 100 {
            break;
        }

        page += 1;
    }

    Ok(all_repos)
}

/// Check if GitHub Actions is enabled for a repository
pub async fn is_actions_enabled(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<bool, GitHubError> {
    use reqwest::StatusCode;
    use tracing::warn;

    info!("Checking actions permissions for {}/{}", owner, repo);

    let request = client.get(format!(
        "{}/repos/{}/{}/actions/permissions",
        GITHUB_API_BASE, owner, repo
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    let status = response.status();
    let headers = response.headers().clone();
    response_handler.update_rate_limit(&headers);

    match status {
        StatusCode::OK => {
            #[derive(serde::Deserialize)]
            struct PermissionsResponse {
                enabled: bool,
            }

            let body_bytes = response.bytes().await?;
            let body = serde_json::from_slice::<PermissionsResponse>(body_bytes.as_ref()).map_err(
                |error| {
                    GitHubError::ApiError(format!(
                        "Failed to parse permissions response: {}",
                        error
                    ))
                },
            )?;
            Ok(body.enabled)
        }
        StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => {
            // Lack of access means we default to true to keep repo visible
            warn!(
                "Cannot determine actions permissions for {}/{} (status: {}), assuming enabled",
                owner, repo, status
            );
            Ok(true)
        }
        StatusCode::UNAUTHORIZED => Err(GitHubError::AuthenticationFailed),
        _ => {
            let text = response.text().await.unwrap_or_default();
            Err(GitHubError::ApiError(format!(
                "Status {}: {}",
                status, text
            )))
        }
    }
}

/// List branches for a repository
pub async fn list_branches(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<Vec<Branch>, GitHubError> {
    info!("Fetching branches for {}/{}", owner, repo);
    let cache_key = format!("branches:{owner}/{repo}:per_page=100");

    let request = client
        .get(format!(
            "{}/repos/{}/{}/branches",
            GITHUB_API_BASE, owner, repo
        ))
        .query(&[("per_page", "100")]);

    let request = add_auth_header(request, token);
    let request = response_handler.apply_cache_headers(request, Some(cache_key.as_str()));
    let response = request.send().await?;

    let branches: Vec<Branch> = response_handler
        .handle_response(response, Some(cache_key.as_str()))
        .await?;

    Ok(branches)
}
