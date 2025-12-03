use crate::api::GitHubError;
use crate::api::models::{
    Branch, BranchCommit, Job, RateLimitInfo, Repo, RepoPermissions, User, Workflow, WorkflowRun,
};
use chrono::Utc;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RepoKey {
    owner: String,
    name: String,
}

impl RepoKey {
    fn new(owner: &str, name: &str) -> Self {
        Self {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct WorkflowKey {
    repo: RepoKey,
    workflow_id: i64,
}

impl WorkflowKey {
    fn new(owner: &str, name: &str, workflow_id: i64) -> Self {
        Self {
            repo: RepoKey::new(owner, name),
            workflow_id,
        }
    }
}

#[derive(Debug, Clone)]
struct DemoData {
    repos: Vec<Repo>,
    actions_enabled: HashSet<RepoKey>,
    branches: HashMap<RepoKey, Vec<Branch>>,
    workflows: HashMap<RepoKey, Vec<Workflow>>,
    runs: HashMap<WorkflowKey, Vec<WorkflowRun>>,
    jobs: HashMap<i64, Vec<Job>>, // run_id -> jobs
    logs: HashMap<i64, String>,   // job_id -> logs
    rate_limit: RateLimitInfo,
    next_run_id: i64,
    next_job_id: i64,
}

impl DemoData {
    fn new() -> Self {
        let mut actions_enabled = HashSet::new();
        let mut repositories = Vec::new();
        let mut branches = HashMap::new();
        let mut workflows_map = HashMap::new();
        let mut runs_map = HashMap::new();
        let mut jobs_map = HashMap::new();
        let mut logs_map = HashMap::new();

        let standard_permissions = Some(RepoPermissions {
            admin: true,
            push: true,
            pull: true,
        });

        let repo_one = Repo {
            id: 10_100,
            name: "actioneer-demo-app".to_string(),
            full_name: "demo-org/actioneer-demo-app".to_string(),
            owner: User {
                login: "demo-org".to_string(),
            },
            is_private: false,
            permissions: standard_permissions.clone(),
            default_branch: Some("main".to_string()),
        };

        let repo_two = Repo {
            id: 10_200,
            name: "workflow-lab".to_string(),
            full_name: "demo-labs/workflow-lab".to_string(),
            owner: User {
                login: "demo-labs".to_string(),
            },
            is_private: false,
            permissions: standard_permissions.clone(),
            default_branch: Some("main".to_string()),
        };

        let repo_three = Repo {
            id: 10_300,
            name: "device-edge".to_string(),
            full_name: "demo-team/device-edge".to_string(),
            owner: User {
                login: "demo-team".to_string(),
            },
            is_private: false,
            permissions: standard_permissions,
            default_branch: Some("main".to_string()),
        };

        repositories.extend([repo_one.clone(), repo_two.clone(), repo_three.clone()]);

        for repo in &repositories {
            actions_enabled.insert(RepoKey::new(&repo.owner.login, &repo.name));
            branches.insert(
                RepoKey::new(&repo.owner.login, &repo.name),
                vec![
                    Branch {
                        name: "main".to_string(),
                        commit: BranchCommit {
                            sha: "8d3c1f4".to_string(),
                        },
                        protected: true,
                    },
                    Branch {
                        name: "develop".to_string(),
                        commit: BranchCommit {
                            sha: "5e2a7b1".to_string(),
                        },
                        protected: false,
                    },
                ],
            );
        }

        // Repo one workflows and runs
        let key_one = RepoKey::new(&repo_one.owner.login, &repo_one.name);
        let workflow_ci = Workflow {
            id: 21_001,
            name: "CI".to_string(),
            path: ".github/workflows/ci.yml".to_string(),
        };
        let workflow_release = Workflow {
            id: 21_002,
            name: "Release".to_string(),
            path: ".github/workflows/release.yml".to_string(),
        };
        workflows_map.insert(
            key_one.clone(),
            vec![workflow_ci.clone(), workflow_release.clone()],
        );
        let runs_ci = vec![
            WorkflowRun {
                id: 30_101,
                run_number: Some(128),
                name: Some("CI".to_string()),
                display_title: Some("CI • main".to_string()),
                head_branch: Some("main".to_string()),
                status: Some("completed".to_string()),
                conclusion: Some("success".to_string()),
                run_started_at: Some("2025-10-28T07:20:00Z".to_string()),
                event: Some("push".to_string()),
                created_at: Some("2025-10-28T07:20:00Z".to_string()),
                updated_at: Some("2025-10-28T07:25:00Z".to_string()),
                html_url: Some(
                    "https://github.com/demo-org/actioneer-demo-app/actions/runs/30101".to_string(),
                ),
            },
            WorkflowRun {
                id: 30_102,
                run_number: Some(127),
                name: Some("CI".to_string()),
                display_title: Some("CI • feature/login".to_string()),
                head_branch: Some("feature/login".to_string()),
                status: Some("completed".to_string()),
                conclusion: Some("failure".to_string()),
                run_started_at: Some("2025-10-27T18:12:00Z".to_string()),
                event: Some("pull_request".to_string()),
                created_at: Some("2025-10-27T18:12:00Z".to_string()),
                updated_at: Some("2025-10-27T18:18:00Z".to_string()),
                html_url: Some(
                    "https://github.com/demo-org/actioneer-demo-app/actions/runs/30102".to_string(),
                ),
            },
        ];

        let runs_release = vec![WorkflowRun {
            id: 30_201,
            run_number: Some(46),
            name: Some("Release".to_string()),
            display_title: Some("Release • v1.0.0".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("queued".to_string()),
            conclusion: None,
            run_started_at: Some("2025-10-28T09:40:00Z".to_string()),
            event: Some("workflow_dispatch".to_string()),
            created_at: Some("2025-10-28T09:38:00Z".to_string()),
            updated_at: Some("2025-10-28T09:40:00Z".to_string()),
            html_url: Some(
                "https://github.com/demo-org/actioneer-demo-app/actions/runs/30201".to_string(),
            ),
        }];

        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_ci.id),
            runs_ci.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_release.id),
            runs_release.clone(),
        );

        let job_compile_success = Job {
            id: 40_001,
            run_id: 30_101,
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            started_at: Some("2025-10-28T07:20:05Z".to_string()),
            completed_at: Some("2025-10-28T07:22:30Z".to_string()),
            name: Some("Compile".to_string()),
            html_url: Some("https://github.com/demo-org/actioneer-demo-app/runs/40001".to_string()),
        };

        let job_tests_success = Job {
            id: 40_002,
            run_id: 30_101,
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            started_at: Some("2025-10-28T07:22:40Z".to_string()),
            completed_at: Some("2025-10-28T07:24:40Z".to_string()),
            name: Some("Tests".to_string()),
            html_url: Some("https://github.com/demo-org/actioneer-demo-app/runs/40002".to_string()),
        };

        let job_lint_failure = Job {
            id: 40_003,
            run_id: 30_102,
            status: Some("completed".to_string()),
            conclusion: Some("failure".to_string()),
            started_at: Some("2025-10-27T18:12:10Z".to_string()),
            completed_at: Some("2025-10-27T18:14:00Z".to_string()),
            name: Some("Lint".to_string()),
            html_url: Some("https://github.com/demo-org/actioneer-demo-app/runs/40003".to_string()),
        };

        let job_tests_partial = Job {
            id: 40_004,
            run_id: 30_102,
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            started_at: Some("2025-10-27T18:14:10Z".to_string()),
            completed_at: Some("2025-10-27T18:16:50Z".to_string()),
            name: Some("Tests".to_string()),
            html_url: Some("https://github.com/demo-org/actioneer-demo-app/runs/40004".to_string()),
        };

        jobs_map.insert(
            30_101,
            vec![job_compile_success.clone(), job_tests_success.clone()],
        );
        jobs_map.insert(
            30_102,
            vec![job_lint_failure.clone(), job_tests_partial.clone()],
        );
        jobs_map.insert(
            30_201,
            vec![Job {
                id: 40_005,
                run_id: 30_201,
                status: Some("queued".to_string()),
                conclusion: None,
                started_at: None,
                completed_at: None,
                name: Some("Publish".to_string()),
                html_url: None,
            }],
        );

        logs_map.insert(
            40_001,
            "Cloning repository...\nResolving crate graph (86 crates)...\nRunning cargo build --workspace --all-targets...\n   Compiling actioneer v1.0.0 (src/main.rs)\n   Compiling actioneer_ui v1.0.0 (src/ui/mod.rs)\n   Compiling actioneer_api v1.0.0 (src/api/mod.rs)\nFinished dev [unoptimized + debuginfo] target(s) in 1m 45s.\nUploading build artifacts to GitHub cache.".to_string(),
        );
        logs_map.insert(
            40_002,
            "Executing cargo test...\nStarting test runner with 8 parallel jobs...\nRunning ui::detail_view::tests::loads_runs... ok\nRunning api::client::tests::creates_client_without_token... ok\nRunning storage::token_storage::tests::round_trip_token... ok\nAll 142 tests passed in 1m 58s.".to_string(),
        );
        logs_map.insert(
            40_003,
            "Running cargo fmt --check...\nDiff detected in src/ui/main_window.rs at lines 120-145.\nwarning: rustfmt would make changes.\nHint: run `cargo fmt` locally before opening a pull request.".to_string(),
        );
        logs_map.insert(
            40_004,
            "Executing cargo test --package actioneer --lib...\nFreshening cached dependencies...\nRunning repo_detail::tests::shows_jobs_table... FAILED\nthread 'repo_detail::tests::shows_jobs_table' panicked at 'expected 4 rows, found 3', src/ui/detail_view.rs:287\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace.\nTest result: FAILED. 73 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out.".to_string(),
        );
        logs_map.insert(
            40_005,
            "Preparing release artifacts...\nReading release metadata from .github/workflows/release.yml...\nPacking flatpak bundle (com.example.Actioneer.flatpak)...\nCalculating SHA256: 7c9f6df2160db3ef4cf3b0d72d4e0a9a\nUploading artifacts to release draft...\nWaiting for maintainer approval before publishing.".to_string(),
        );

        // Repo two data
        let key_two = RepoKey::new(&repo_two.owner.login, &repo_two.name);
        let workflow_smoke = Workflow {
            id: 22_001,
            name: "Smoke Tests".to_string(),
            path: ".github/workflows/smoke.yml".to_string(),
        };
        let workflow_deploy = Workflow {
            id: 22_002,
            name: "Deploy".to_string(),
            path: ".github/workflows/deploy.yml".to_string(),
        };

        workflows_map.insert(
            key_two.clone(),
            vec![workflow_smoke.clone(), workflow_deploy.clone()],
        );

        let runs_smoke = vec![WorkflowRun {
            id: 31_101,
            run_number: Some(67),
            name: Some("Smoke Tests".to_string()),
            display_title: Some("Smoke Tests • main".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("in_progress".to_string()),
            conclusion: None,
            run_started_at: Some("2025-10-28T09:55:00Z".to_string()),
            event: Some("schedule".to_string()),
            created_at: Some("2025-10-28T09:55:00Z".to_string()),
            updated_at: Some("2025-10-28T09:57:00Z".to_string()),
            html_url: Some(
                "https://github.com/demo-labs/workflow-lab/actions/runs/31101".to_string(),
            ),
        }];

        let runs_deploy = vec![WorkflowRun {
            id: 31_201,
            run_number: Some(24),
            name: Some("Deploy".to_string()),
            display_title: Some("Deploy • production".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            run_started_at: Some("2025-10-26T14:10:00Z".to_string()),
            event: Some("workflow_dispatch".to_string()),
            created_at: Some("2025-10-26T14:10:00Z".to_string()),
            updated_at: Some("2025-10-26T14:18:30Z".to_string()),
            html_url: Some(
                "https://github.com/demo-labs/workflow-lab/actions/runs/31201".to_string(),
            ),
        }];

        runs_map.insert(
            WorkflowKey::new(&repo_two.owner.login, &repo_two.name, workflow_smoke.id),
            runs_smoke.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_two.owner.login, &repo_two.name, workflow_deploy.id),
            runs_deploy.clone(),
        );

        jobs_map.insert(
            31_101,
            vec![Job {
                id: 41_001,
                run_id: 31_101,
                status: Some("in_progress".to_string()),
                conclusion: None,
                started_at: Some("2025-10-28T09:55:05Z".to_string()),
                completed_at: None,
                name: Some("End-to-end tests".to_string()),
                html_url: Some("https://github.com/demo-labs/workflow-lab/runs/41001".to_string()),
            }],
        );
        jobs_map.insert(
            31_201,
            vec![Job {
                id: 41_201,
                run_id: 31_201,
                status: Some("completed".to_string()),
                conclusion: Some("success".to_string()),
                started_at: Some("2025-10-26T14:11:00Z".to_string()),
                completed_at: Some("2025-10-26T14:17:30Z".to_string()),
                name: Some("Deploy to production".to_string()),
                html_url: Some("https://github.com/demo-labs/workflow-lab/runs/41201".to_string()),
            }],
        );

        logs_map.insert(
            41_001,
            "Bootstrapping test environment...\nProvisioning ephemeral runner on ubuntu-24.04...\nExporting secrets to workflow context...\nRunning smoke tests against staging...\nScenario login_flow.rs .... ok\nScenario billing_flow.rs .... ok\nScenario notifications_flow.rs .... pending (retries left: 2).".to_string(),
        );
        logs_map.insert(
            41_201,
            "Connecting to SSH bastion...\nAuthenticating with deploy key actioneer-prod...\nRolling out version 1.18.2 to production...\nScaling web pods to 6 replicas...\nWaiting for health checks (elapsed: 04m12s)...\nDeployment completed in 6m 30s.".to_string(),
        );

        // Repo three data
        let key_three = RepoKey::new(&repo_three.owner.login, &repo_three.name);
        let workflow_edge = Workflow {
            id: 23_001,
            name: "Edge Diagnostics".to_string(),
            path: ".github/workflows/edge.yml".to_string(),
        };

        workflows_map.insert(key_three.clone(), vec![workflow_edge.clone()]);

        let runs_edge = vec![WorkflowRun {
            id: 32_101,
            run_number: Some(12),
            name: Some("Edge Diagnostics".to_string()),
            display_title: Some("Edge Diagnostics • nightly".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            run_started_at: Some("2025-10-27T02:00:00Z".to_string()),
            event: Some("schedule".to_string()),
            created_at: Some("2025-10-27T02:00:00Z".to_string()),
            updated_at: Some("2025-10-27T02:06:45Z".to_string()),
            html_url: Some(
                "https://github.com/demo-team/device-edge/actions/runs/32101".to_string(),
            ),
        }];

        runs_map.insert(
            WorkflowKey::new(&repo_three.owner.login, &repo_three.name, workflow_edge.id),
            runs_edge.clone(),
        );

        jobs_map.insert(
            32_101,
            vec![
                Job {
                    id: 42_001,
                    run_id: 32_101,
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    started_at: Some("2025-10-27T02:00:05Z".to_string()),
                    completed_at: Some("2025-10-27T02:03:30Z".to_string()),
                    name: Some("Collect metrics".to_string()),
                    html_url: Some(
                        "https://github.com/demo-team/device-edge/runs/42001".to_string(),
                    ),
                },
                Job {
                    id: 42_002,
                    run_id: 32_101,
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    started_at: Some("2025-10-27T02:03:35Z".to_string()),
                    completed_at: Some("2025-10-27T02:06:20Z".to_string()),
                    name: Some("Aggregate diagnostics".to_string()),
                    html_url: Some(
                        "https://github.com/demo-team/device-edge/runs/42002".to_string(),
                    ),
                },
            ],
        );

        logs_map.insert(
            42_001,
            "Gathering CPU metrics from remote agents...\nCollecting network stats from 12 devices (latency, jitter, packet loss)...\nStreaming telemetry to s3://demo-edge-diagnostics/tmp/42101.log...\nDiagnostics captured; archiving raw samples.".to_string(),
        );
        logs_map.insert(
            42_002,
            "Aggregating diagnostics into report...\nLoading metric baselines from cache...\nDetecting anomalies (threshold 2.5 sigma)... none found.\nRendering PDF summary with 4 charts...\nUploading report artifact edge-report.zip to workflow run.".to_string(),
        );

        let rate_limit = RateLimitInfo {
            limit: 5000,
            remaining: 5000,
            reset: (Utc::now() + chrono::Duration::minutes(60)).timestamp(),
        };

        Self {
            repos: repositories,
            actions_enabled,
            branches,
            workflows: workflows_map,
            runs: runs_map,
            jobs: jobs_map,
            logs: logs_map,
            rate_limit,
            next_run_id: 40_000,
            next_job_id: 50_000,
        }
    }

    fn repo_key(owner: &str, name: &str) -> RepoKey {
        RepoKey::new(owner, name)
    }

    fn workflow_key(owner: &str, name: &str, workflow_id: i64) -> WorkflowKey {
        WorkflowKey::new(owner, name, workflow_id)
    }

    fn add_manual_run(
        &mut self,
        owner: &str,
        name: &str,
        workflow_id: i64,
        reference: &str,
    ) -> GitHubErrorResult<()> {
        let repo_key = Self::repo_key(owner, name);
        let workflow_key = Self::workflow_key(owner, name, workflow_id);

        let runs = self
            .runs
            .get_mut(&workflow_key)
            .ok_or_else(|| GitHubError::NotFound)?;

        self.next_run_id += 1;
        let run_id = self.next_run_id;
        let now = Utc::now();
        let run_number = runs.first().and_then(|run| run.run_number).unwrap_or(100) + 1;

        let new_run = WorkflowRun {
            id: run_id,
            run_number: Some(run_number),
            name: Some("Manual Dispatch".to_string()),
            display_title: Some(format!("{} • {}", reference, reference)),
            head_branch: Some(reference.to_string()),
            status: Some("queued".to_string()),
            conclusion: None,
            run_started_at: Some(now.to_rfc3339()),
            event: Some("workflow_dispatch".to_string()),
            created_at: Some(now.to_rfc3339()),
            updated_at: Some(now.to_rfc3339()),
            html_url: Some(format!(
                "https://github.com/{}/{}/actions/runs/{}",
                owner, name, run_id
            )),
        };

        runs.insert(0, new_run);

        self.next_job_id += 1;
        let job_id = self.next_job_id;
        let job = Job {
            id: job_id,
            run_id,
            status: Some("queued".to_string()),
            conclusion: None,
            started_at: None,
            completed_at: None,
            name: Some("Queued job".to_string()),
            html_url: None,
        };
        self.jobs.insert(run_id, vec![job]);
        self.logs.insert(
            job_id,
            "This job was created by demo mode to simulate workflow dispatch.".to_string(),
        );

        // Ensure branch exists for manual references
        self.branches.entry(repo_key).or_default().push(Branch {
            name: reference.to_string(),
            commit: BranchCommit {
                sha: format!("{:x}", run_id),
            },
            protected: false,
        });

        Ok(())
    }

    fn update_run_status(
        &mut self,
        owner: &str,
        name: &str,
        run_id: i64,
        status: &str,
        conclusion: Option<&str>,
    ) -> GitHubErrorResult<()> {
        for runs in self
            .runs
            .values_mut()
            .filter(|runs| runs.iter().any(|run| run.id == run_id))
        {
            if let Some(run) = runs.iter_mut().find(|run| run.id == run_id) {
                run.status = Some(status.to_string());
                run.conclusion = conclusion.map(ToString::to_string);
                run.updated_at = Some(Utc::now().to_rfc3339());
                return Ok(());
            }
        }

        let key = Self::repo_key(owner, name);
        if !self.actions_enabled.contains(&key) {
            return Err(GitHubError::NotFound);
        }

        Err(GitHubError::NotFound)
    }
}

pub type GitHubErrorResult<T> = Result<T, GitHubError>;

static DEMO_STATE: OnceLock<Mutex<Option<DemoData>>> = OnceLock::new();

fn store() -> &'static Mutex<Option<DemoData>> {
    DEMO_STATE.get_or_init(|| Mutex::new(None))
}

pub fn enable() -> Vec<Repo> {
    let mut guard = store().lock();
    let data = DemoData::new();
    let repos = data.repos.clone();
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

pub fn list_repos() -> Option<Vec<Repo>> {
    with_data(|data| data.repos.clone())
}

pub fn rate_limit_info() -> Option<RateLimitInfo> {
    with_data(|data| data.rate_limit.clone())
}

pub fn is_actions_enabled(owner: &str, repo: &str) -> Option<bool> {
    with_data(|data| {
        let key = DemoData::repo_key(owner, repo);
        data.actions_enabled.contains(&key)
    })
}

pub fn list_branches(owner: &str, repo: &str) -> Option<Vec<Branch>> {
    with_data(|data| {
        let key = DemoData::repo_key(owner, repo);
        data.branches.get(&key).cloned().unwrap_or_default()
    })
}

pub fn list_workflows(owner: &str, repo: &str) -> Option<Vec<Workflow>> {
    with_data(|data| {
        let key = DemoData::repo_key(owner, repo);
        data.workflows.get(&key).cloned().unwrap_or_default()
    })
}

pub fn list_runs(owner: &str, repo: &str, workflow_id: i64) -> Option<Vec<WorkflowRun>> {
    with_data(|data| {
        let key = DemoData::workflow_key(owner, repo, workflow_id);
        data.runs.get(&key).cloned().unwrap_or_default()
    })
}

pub fn list_jobs(owner: &str, repo: &str, run_id: i64) -> Option<Vec<Job>> {
    with_data(|data| {
        let _ = DemoData::repo_key(owner, repo);
        data.jobs.get(&run_id).cloned().unwrap_or_default()
    })
}

pub fn job_logs(job_id: i64) -> Option<String> {
    with_data(|data| data.logs.get(&job_id).cloned()).flatten()
}

pub fn dispatch_workflow(
    owner: &str,
    repo: &str,
    workflow_id: i64,
    reference: &str,
) -> GitHubErrorResult<()> {
    with_data_mut(|data| data.add_manual_run(owner, repo, workflow_id, reference))
        .unwrap_or(Err(GitHubError::NotFound))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_enable_populates_repos() {
        let repos = enable();
        assert!(!repos.is_empty());
        assert!(is_active());
        disable();
        assert!(!is_active());
    }
}
