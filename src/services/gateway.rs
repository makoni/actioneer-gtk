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
//!
//! A live gateway may carry the [`DataCache`], bound to the token it was built
//! with. What it fetches is written through to the cache here, in
//! one place, rather than at the dozen UI call sites; and because only the live
//! variant has a cache, demo data can neither be stored nor read back.

use crate::demo::DemoBackend;
use crate::services::api::models::{
    Branch, Job, RateLimitInfo, Repo, Workflow, WorkflowDispatchInput, WorkflowRun,
};
use crate::services::api::{GitHubClient, GitHubError};
use crate::services::cache::{CacheOwner, Cached, DataCache};
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
enum Inner {
    Live(GitHubClient, Option<CacheBinding>),
    Demo(DemoBackend),
}

#[derive(Clone)]
struct CacheBinding {
    cache: Arc<DataCache>,
    owner: CacheOwner,
}

impl CacheBinding {
    fn new(cache: &Arc<DataCache>, token: &str) -> Self {
        Self {
            cache: cache.clone(),
            owner: cache.bind_owner(token),
        }
    }
}

#[derive(Clone)]
pub struct GitHubGateway {
    inner: Inner,
}

impl GitHubGateway {
    /// The real GitHub REST API.
    pub fn live(token: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: Inner::Live(GitHubClient::new(token)?, None),
        })
    }

    /// The real GitHub REST API, writing what it fetches through to `cache`
    /// under the account behind `token`.
    pub fn live_cached(token: String, cache: &Arc<DataCache>) -> Result<Self> {
        // The client first: binding retires the previous owner, which must not
        // happen for a gateway that is never installed.
        let client = GitHubClient::new(Some(token.clone()))?;
        Ok(Self {
            inner: Inner::Live(client, Some(CacheBinding::new(cache, &token))),
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
            inner: Inner::Live(GitHubClient::with_base_url(base, token)?, None),
        })
    }

    /// [`live_cached`](Self::live_cached) against a different API root, for
    /// the same reason as [`live_with_base_url`](Self::live_with_base_url).
    #[doc(hidden)]
    pub fn live_cached_with_base_url(
        base: &str,
        token: String,
        cache: &Arc<DataCache>,
    ) -> Result<Self> {
        let client = GitHubClient::with_base_url(base, Some(token.clone()))?;
        Ok(Self {
            inner: Inner::Live(client, Some(CacheBinding::new(cache, &token))),
        })
    }

    fn cache(&self) -> Option<&CacheBinding> {
        match &self.inner {
            Inner::Live(_, binding) => binding.as_ref(),
            Inner::Demo(_) => None,
        }
    }

    /// The repository list as last fetched, for painting before the network
    /// answers. `None` for demo, an uncached gateway, or nothing stored yet.
    pub fn cached_repos(&self) -> Option<Cached<Vec<Repo>>> {
        let binding = self.cache()?;
        binding.cache.repos(&binding.owner)
    }

    /// A repository's workflows as last fetched.
    pub fn cached_workflows(&self, repo_id: i64) -> Option<Cached<Vec<Workflow>>> {
        let binding = self.cache()?;
        binding.cache.workflows(&binding.owner, repo_id)
    }

    /// The most recent run of each of a repository's workflows, as last
    /// fetched. A hint only: a run shown here as in progress may long be done.
    pub fn cached_latest_runs(&self, repo_id: i64) -> Option<Cached<Vec<WorkflowRun>>> {
        let binding = self.cache()?;
        binding.cache.latest_runs(&binding.owner, repo_id)
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
            Inner::Live(..) => None,
            Inner::Demo(backend) => Some(backend.seed()),
        }
    }

    pub fn rate_limit_info(&self) -> Option<RateLimitInfo> {
        match &self.inner {
            Inner::Live(client, _) => client.rate_limit_info(),
            Inner::Demo(backend) => backend.rate_limit_info(),
        }
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>, GitHubError> {
        match &self.inner {
            Inner::Live(client, binding) => {
                let repos = client.list_repos().await?;
                if let Some(binding) = binding {
                    binding.cache.store_repos(&binding.owner, &repos);
                }
                Ok(repos)
            }
            Inner::Demo(backend) => backend.list_repos().await,
        }
    }

    pub async fn is_actions_enabled(&self, owner: &str, repo: &str) -> Result<bool, GitHubError> {
        match &self.inner {
            Inner::Live(client, _) => client.is_actions_enabled(owner, repo).await,
            Inner::Demo(backend) => backend.is_actions_enabled(owner, repo).await,
        }
    }

    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>, GitHubError> {
        match &self.inner {
            Inner::Live(client, _) => client.list_branches(owner, repo).await,
            Inner::Demo(backend) => backend.list_branches(owner, repo).await,
        }
    }

    pub async fn list_workflows(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<Workflow>, GitHubError> {
        match &self.inner {
            Inner::Live(client, binding) => {
                let workflows = client.list_workflows(owner, repo).await?;
                if let Some(binding) = binding {
                    binding
                        .cache
                        .store_workflows(&binding.owner, owner, repo, &workflows);
                }
                Ok(workflows)
            }
            Inner::Demo(backend) => backend.list_workflows(owner, repo).await,
        }
    }

    pub async fn dispatch_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        ref_name: &str,
        inputs: Option<serde_json::Value>,
    ) -> Result<Option<WorkflowRun>, GitHubError> {
        match &self.inner {
            Inner::Live(client, _) => {
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
            Inner::Live(client, _) => {
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
            Inner::Live(client, binding) => {
                let runs = client.list_runs(owner, repo, workflow_id).await?;
                if let Some(binding) = binding {
                    binding.cache.note_runs(&binding.owner, &runs);
                }
                Ok(runs)
            }
            Inner::Demo(backend) => backend.list_runs(owner, repo, workflow_id).await,
        }
    }

    pub async fn list_repository_runs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<WorkflowRun>, GitHubError> {
        match &self.inner {
            Inner::Live(client, binding) => {
                let runs = client.list_repository_runs(owner, repo).await?;
                if let Some(binding) = binding {
                    binding
                        .cache
                        .store_repository_runs(&binding.owner, owner, repo, &runs);
                }
                Ok(runs)
            }
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
            Inner::Live(client, binding) => {
                let result = client.rerun_workflow(owner, repo, run_id).await;
                // Forgotten even when the call fails: a timeout can hide a
                // re-run that went through, and the cost is only a refetch.
                if let Some(binding) = binding {
                    binding.cache.forget_run(&binding.owner, run_id);
                }
                result
            }
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
            Inner::Live(client, binding) => {
                let result = client.rerun_failed_jobs(owner, repo, run_id).await;
                if let Some(binding) = binding {
                    binding.cache.forget_run(&binding.owner, run_id);
                }
                result
            }
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
            Inner::Live(client, binding) => {
                let result = client.cancel_run(owner, repo, run_id).await;
                if let Some(binding) = binding {
                    binding.cache.forget_run(&binding.owner, run_id);
                }
                result
            }
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
            Inner::Live(client, _) => client.list_jobs(owner, repo, run_id).await,
            Inner::Demo(backend) => backend.list_jobs(owner, repo, run_id).await,
        }
    }

    /// [`list_jobs`](Self::list_jobs) for a run row in the detail pane, which
    /// may answer from the cache.
    ///
    /// It does so only when the run's latest version seen in a *live* run list
    /// is completed and unchanged since the jobs were stored, and every job had
    /// finished — a completed run's jobs cannot change until it is re-run, and
    /// a re-run moves the run's `updated_at`. Callers that follow a run as it
    /// changes (the log window, the actions after a re-run) use `list_jobs`.
    pub async fn list_jobs_for_run(
        &self,
        owner: &str,
        repo: &str,
        run_id: i64,
    ) -> Result<Vec<Job>, GitHubError> {
        match &self.inner {
            Inner::Live(client, Some(binding)) => {
                if let Some(jobs) = binding.cache.completed_jobs(&binding.owner, run_id) {
                    return Ok(jobs.as_ref().clone());
                }
                let fetch = binding.cache.begin_jobs_fetch(&binding.owner, run_id);
                let jobs = client.list_jobs(owner, repo, run_id).await?;
                binding.cache.store_jobs(&binding.owner, fetch, &jobs);
                Ok(jobs)
            }
            _ => self.list_jobs(owner, repo, run_id).await,
        }
    }

    pub async fn get_job_logs(
        &self,
        owner: &str,
        repo: &str,
        job_id: i64,
    ) -> Result<String, GitHubError> {
        match &self.inner {
            Inner::Live(client, _) => client.get_job_logs(owner, repo, job_id).await,
            Inner::Demo(backend) => backend.get_job_logs(owner, repo, job_id).await,
        }
    }
}
