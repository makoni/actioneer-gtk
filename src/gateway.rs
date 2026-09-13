//! The data source the UI talks to: live GitHub, or the demo fixtures.
//!
//! A neutral top-level home on purpose — the gateway depends on **both** `api`
//! and `demo`, so it cannot live inside `api/`; that is the layering violation
//! this phase removes. Phase 4 moves it to `services/gateway.rs`.
//!
//! A sum type rather than a trait. With exactly two backends it costs one match
//! arm per operation and avoids the trade a trait would force here: `async fn`
//! in traits is stable but not `dyn`-compatible without boxing, and RPITIT
//! drops the `Send` bound these futures need — they go to
//! `runtime::handle().spawn`. A **third** data source is the trigger to switch
//! to a trait design.
//!
//! The known cost, accepted: each operation is written in three places
//! (`GitHubClient`, `GitHubGateway`, `DemoBackend`).
//!
//! `Clone` is load-bearing. Every UI call site reads the gateway out of its slot
//! with `slot.lock().clone()`, because a `parking_lot::MutexGuard` cannot be
//! held across an `.await` in a `Send` future. Both variants share their state
//! through `Arc`, so a clone is a handle, not a copy.

use crate::api::models::{
    Branch, Job, RateLimitInfo, Repo, Workflow, WorkflowDispatchInput, WorkflowRun,
};
use crate::api::{GitHubClient, GitHubError};
use crate::demo::DemoBackend;
use anyhow::Result;

#[derive(Clone)]
enum Inner {
    Live(GitHubClient),
    Demo(DemoBackend),
}

#[derive(Clone)]
pub struct GitHubGateway {
    inner: Inner,
}

impl GitHubGateway {
    /// The real GitHub REST API.
    pub fn live(token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: Inner::Live(GitHubClient::new(token)?),
        })
    }

    /// A live gateway against a different API root.
    ///
    /// Exists for integration tests, which need a controllable server: the
    /// alternative would be an environment variable, and that is process-global
    /// and would leak between the parallel tests inside one binary.
    #[doc(hidden)]
    pub fn live_with_base_url(base: &str, token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: Inner::Live(GitHubClient::with_base_url(base, token)?),
        })
    }

    /// The demo fixtures. Infallible — there is nothing to configure.
    pub fn demo() -> Self {
        Self {
            inner: Inner::Demo(DemoBackend::new()),
        }
    }

    /// True when this gateway serves demo fixtures rather than live data.
    pub fn is_demo(&self) -> bool {
        matches!(self.inner, Inner::Demo(_))
    }

    /// The repositories and rate limit to seed the UI with, synchronously.
    ///
    /// Only demo mode can answer without I/O; a live gateway returns `None` and
    /// the caller loads through the async path as usual.
    pub fn demo_seed(&self) -> Option<(Vec<Repo>, Option<RateLimitInfo>)> {
        match &self.inner {
            Inner::Live(_) => None,
            Inner::Demo(backend) => Some(backend.seed()),
        }
    }

    pub fn rate_limit_info(&self) -> Option<RateLimitInfo> {
        match &self.inner {
            Inner::Live(client) => client.rate_limit_info(),
            Inner::Demo(backend) => backend.rate_limit_info(),
        }
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_repos().await,
            Inner::Demo(backend) => backend.list_repos().await,
        }
    }

    pub async fn is_actions_enabled(&self, owner: &str, repo: &str) -> Result<bool, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.is_actions_enabled(owner, repo).await,
            Inner::Demo(backend) => backend.is_actions_enabled(owner, repo).await,
        }
    }

    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_branches(owner, repo).await,
            Inner::Demo(backend) => backend.list_branches(owner, repo).await,
        }
    }

    pub async fn list_workflows(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<Workflow>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_workflows(owner, repo).await,
            Inner::Demo(backend) => backend.list_workflows(owner, repo).await,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        ref_name: &str,
        inputs: Option<serde_json::Value>,
    ) -> Result<Option<WorkflowRun>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => {
                client
                    .dispatch_workflow(owner, repo, workflow_id, ref_name, inputs)
                    .await
            }
            Inner::Demo(backend) => {
                backend
                    .dispatch_workflow(owner, repo, workflow_id, ref_name, inputs)
                    .await
            }
        }
    }

    pub async fn get_workflow_dispatch_inputs(
        &self,
        owner: &str,
        repo: &str,
        workflow_path: &str,
        reference: Option<&str>,
    ) -> Result<Vec<WorkflowDispatchInput>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => {
                client
                    .get_workflow_dispatch_inputs(owner, repo, workflow_path, reference)
                    .await
            }
            Inner::Demo(backend) => {
                backend
                    .get_workflow_dispatch_inputs(owner, repo, workflow_path, reference)
                    .await
            }
        }
    }

    pub async fn list_runs(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: i64,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_runs(owner, repo, workflow_id).await,
            Inner::Demo(backend) => backend.list_runs(owner, repo, workflow_id).await,
        }
    }

    pub async fn list_repository_runs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_repository_runs(owner, repo).await,
            Inner::Demo(backend) => backend.list_repository_runs(owner, repo).await,
        }
    }

    pub async fn rerun_workflow(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.rerun_workflow(owner, repo, run_id).await,
            Inner::Demo(backend) => backend.rerun_workflow(owner, repo, run_id).await,
        }
    }

    pub async fn rerun_failed_jobs(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.rerun_failed_jobs(owner, repo, run_id).await,
            Inner::Demo(backend) => backend.rerun_failed_jobs(owner, repo, run_id).await,
        }
    }

    pub async fn cancel_run(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<(), GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.cancel_run(owner, repo, run_id).await,
            Inner::Demo(backend) => backend.cancel_run(owner, repo, run_id).await,
        }
    }

    pub async fn list_jobs(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<Vec<Job>, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.list_jobs(owner, repo, run_id).await,
            Inner::Demo(backend) => backend.list_jobs(owner, repo, run_id).await,
        }
    }

    pub async fn get_job_logs(
        &self,
        owner: &str,
        repo: &str,
        job_id: i64,
    ) -> Result<String, GitHubError> {
        match &self.inner {
            Inner::Live(client) => client.get_job_logs(owner, repo, job_id).await,
            Inner::Demo(backend) => backend.get_job_logs(owner, repo, job_id).await,
        }
    }
}
