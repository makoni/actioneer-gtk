use super::error::GitHubError;
use super::http::ResponseHandler;
use super::{jobs, repos, runs, workflows};
use crate::api::models::*;
use anyhow::Result;
use reqwest::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue};
use std::env;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tracing::info;

const DEFAULT_GITHUB_API_VERSION: &str = "2026-03-10";
const GITHUB_API_VERSION_HEADER: &str = "x-github-api-version";
const GITHUB_ACCEPT_HEADER: &str = "application/vnd.github+json";
const GITHUB_API_VERSION_ENV: &str = "ACTIONEER_GITHUB_API_VERSION";

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    base_url: String,
    token: Option<String>,
    response_handler: Arc<ResponseHandler>,
}

impl GitHubClient {
    pub fn new(token: Option<String>) -> Result<Self> {
        Self::with_base_url(super::http::GITHUB_API_BASE, token)
    }

    /// Builds a client against a different API root.
    ///
    /// Exists so integration tests can point the client at a local `wiremock`
    /// server: every URL is built from this base. Not an environment variable
    /// on purpose — an env override is process-global and would leak between
    /// the parallel tests inside one test binary.
    pub fn with_base_url(base: &str, token: Option<String>) -> Result<Self> {
        let api_version = github_api_version_from_env();
        let client = Client::builder()
            .user_agent("Actioneer-Linux/0.1.0")
            .default_headers(github_default_headers(&api_version)?)
            .timeout(Duration::from_secs(30))
            .build()?;

        let rate_limit = Arc::new(StdMutex::new(None));
        let response_handler = Arc::new(ResponseHandler::new(rate_limit));

        info!(
            api_version = %api_version,
            "Configured GitHub REST API client"
        );

        Ok(Self {
            client,
            base_url: base.trim_end_matches('/').to_string(),
            token,
            response_handler,
        })
    }

    pub fn rate_limit_info(&self) -> Option<RateLimitInfo> {
        self.response_handler.get_rate_limit()
    }

    // Repository operations
    pub async fn list_repos(&self) -> Result<Vec<Repo>, GitHubError> {
        repos::list_repos(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
        )
        .await
    }

    pub async fn is_actions_enabled(&self, owner: &str, repo: &str) -> Result<bool, GitHubError> {
        repos::is_actions_enabled(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
        )
        .await
    }

    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>, GitHubError> {
        repos::list_branches(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
        )
        .await
    }

    // Workflow operations
    pub async fn list_workflows(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<Workflow>, GitHubError> {
        workflows::list_workflows(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
        )
        .await
    }

    pub async fn dispatch_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        ref_name: &str,
        inputs: Option<serde_json::Value>,
    ) -> Result<Option<WorkflowRun>, GitHubError> {
        workflows::dispatch_workflow(
            &self.client,
            &self.base_url,
            &self.token,
            owner,
            repo,
            workflow_id,
            ref_name,
            inputs,
        )
        .await
    }

    pub async fn get_workflow_dispatch_inputs(
        &self,
        owner: &str,
        repo: &str,
        workflow_path: &str,
        reference: Option<&str>,
    ) -> Result<Vec<WorkflowDispatchInput>, GitHubError> {
        workflows::get_workflow_dispatch_inputs(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
            workflow_path,
            reference,
        )
        .await
    }

    // Run operations
    pub async fn list_runs(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: i64,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        runs::list_runs(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
            workflow_id,
        )
        .await
    }

    pub async fn list_repository_runs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        runs::list_repository_runs(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
        )
        .await
    }

    pub async fn rerun_workflow(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        runs::rerun_workflow(
            &self.client,
            &self.base_url,
            &self.token,
            owner,
            repo,
            run_id,
        )
        .await
    }

    pub async fn rerun_failed_jobs(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        runs::rerun_failed_jobs(
            &self.client,
            &self.base_url,
            &self.token,
            owner,
            repo,
            run_id,
        )
        .await
    }

    pub async fn cancel_run(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        runs::cancel_run(
            &self.client,
            &self.base_url,
            &self.token,
            owner,
            repo,
            run_id,
        )
        .await
    }

    // Job operations
    pub async fn list_jobs(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<Vec<Job>, GitHubError> {
        jobs::list_jobs(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
            run_id,
        )
        .await
    }

    pub async fn get_job_logs(
        &self,
        owner: &str,
        repo: &str,
        job_id: i64,
    ) -> Result<String, GitHubError> {
        jobs::get_job_logs(
            &self.client,
            &self.base_url,
            &self.token,
            &self.response_handler,
            owner,
            repo,
            job_id,
        )
        .await
    }
}

fn github_api_version_from_env() -> String {
    env::var(GITHUB_API_VERSION_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_GITHUB_API_VERSION.to_string())
}

fn github_default_headers(api_version: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_ACCEPT_HEADER));
    headers.insert(
        HeaderName::from_static(GITHUB_API_VERSION_HEADER),
        HeaderValue::from_str(api_version)?,
    );
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_client_creation() {
        let client = GitHubClient::new(Some("test_token".to_string()));
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_without_token() {
        let client = GitHubClient::new(None);
        assert!(client.is_ok());
    }

    #[test]
    fn test_github_default_headers_include_accept_and_api_version() {
        let headers = github_default_headers("2026-03-10").expect("headers");

        assert_eq!(
            headers.get(ACCEPT),
            Some(&HeaderValue::from_static(GITHUB_ACCEPT_HEADER))
        );
        assert_eq!(
            headers.get(HeaderName::from_static(GITHUB_API_VERSION_HEADER)),
            Some(&HeaderValue::from_static("2026-03-10"))
        );
    }

    #[tokio::test]
    async fn test_client_requests_use_versioned_default_headers() {
        let server = MockServer::start().await;
        let client = Client::builder()
            .default_headers(github_default_headers("2026-03-10").expect("headers"))
            .build()
            .expect("client");

        Mock::given(method("GET"))
            .and(path("/headers"))
            .and(header("accept", GITHUB_ACCEPT_HEADER))
            .and(header(GITHUB_API_VERSION_HEADER, "2026-03-10"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        client
            .get(format!("{}/headers", server.uri()))
            .send()
            .await
            .expect("request");
    }
}
