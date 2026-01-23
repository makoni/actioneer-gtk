use crate::api::GitHubError;
use crate::api::models::{
    Branch, BranchCommit, Job, RateLimitInfo, Repo, RepoPermissions, User, Workflow, WorkflowRun,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) struct RepoKey {
    owner: String,
    name: String,
}

impl RepoKey {
    pub(super) fn new(owner: &str, name: &str) -> Self {
        Self {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) struct WorkflowKey {
    repo: RepoKey,
    workflow_id: i64,
}

impl WorkflowKey {
    pub(super) fn new(owner: &str, name: &str, workflow_id: i64) -> Self {
        Self {
            repo: RepoKey::new(owner, name),
            workflow_id,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DemoData {
    pub(super) repos: Vec<Repo>,
    pub(super) actions_enabled: HashSet<RepoKey>,
    pub(super) branches: HashMap<RepoKey, Vec<Branch>>,
    pub(super) workflows: HashMap<RepoKey, Vec<Workflow>>,
    pub(super) runs: HashMap<WorkflowKey, Vec<WorkflowRun>>,
    pub(super) jobs: HashMap<i64, Vec<Job>>, // run_id -> jobs
    pub(super) logs: HashMap<i64, String>,   // job_id -> logs
    pub(super) rate_limit: RateLimitInfo,
    pub(super) next_run_id: i64,
    pub(super) next_job_id: i64,
}

impl DemoData {
    pub(super) fn new() -> Self {
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

        jobs_map.insert(
            30_101,
            vec![
                Job {
                    id: 43_001,
                    run_id: 30_101,
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    started_at: Some("2025-10-28T07:20:05Z".to_string()),
                    completed_at: Some("2025-10-28T07:22:40Z".to_string()),
                    name: Some("Build".to_string()),
                    html_url: Some(
                        "https://github.com/demo-org/actioneer-demo-app/runs/43001".to_string(),
                    ),
                },
                Job {
                    id: 43_002,
                    run_id: 30_101,
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    started_at: Some("2025-10-28T07:22:45Z".to_string()),
                    completed_at: Some("2025-10-28T07:25:00Z".to_string()),
                    name: Some("Tests".to_string()),
                    html_url: Some(
                        "https://github.com/demo-org/actioneer-demo-app/runs/43002".to_string(),
                    ),
                },
            ],
        );

        jobs_map.insert(
            30_102,
            vec![Job {
                id: 43_011,
                run_id: 30_102,
                status: Some("completed".to_string()),
                conclusion: Some("failure".to_string()),
                started_at: Some("2025-10-27T18:12:10Z".to_string()),
                completed_at: Some("2025-10-27T18:18:00Z".to_string()),
                name: Some("Lint & unit tests".to_string()),
                html_url: Some(
                    "https://github.com/demo-org/actioneer-demo-app/runs/43011".to_string(),
                ),
            }],
        );

        jobs_map.insert(
            30_201,
            vec![Job {
                id: 43_021,
                run_id: 30_201,
                status: Some("queued".to_string()),
                conclusion: None,
                started_at: None,
                completed_at: None,
                name: Some("Publish artifacts".to_string()),
                html_url: Some(
                    "https://github.com/demo-org/actioneer-demo-app/runs/43021".to_string(),
                ),
            }],
        );

        let key_two = RepoKey::new(&repo_two.owner.login, &repo_two.name);
        let workflow_infra = Workflow {
            id: 22_001,
            name: "Infrastructure".to_string(),
            path: ".github/workflows/infra.yml".to_string(),
        };
        workflows_map.insert(key_two.clone(), vec![workflow_infra.clone()]);

        let runs_infra = vec![WorkflowRun {
            id: 31_001,
            run_number: Some(210),
            name: Some("Infrastructure".to_string()),
            display_title: Some("Infra • terraform plan".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("failure".to_string()),
            run_started_at: Some("2025-10-26T15:00:00Z".to_string()),
            event: Some("workflow_dispatch".to_string()),
            created_at: Some("2025-10-26T15:00:00Z".to_string()),
            updated_at: Some("2025-10-26T15:04:00Z".to_string()),
            html_url: Some(
                "https://github.com/demo-labs/workflow-lab/actions/runs/31001".to_string(),
            ),
        }];

        runs_map.insert(
            WorkflowKey::new(&repo_two.owner.login, &repo_two.name, workflow_infra.id),
            runs_infra.clone(),
        );

        jobs_map.insert(
            31_001,
            vec![Job {
                id: 44_001,
                run_id: 31_001,
                status: Some("completed".to_string()),
                conclusion: Some("failure".to_string()),
                started_at: Some("2025-10-26T15:00:10Z".to_string()),
                completed_at: Some("2025-10-26T15:04:00Z".to_string()),
                name: Some("Terraform plan".to_string()),
                html_url: Some("https://github.com/demo-labs/workflow-lab/runs/44001".to_string()),
            }],
        );

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
            "2026-01-24T09:14:03Z ##[group]Bootstrap runner\n2026-01-24T09:14:04Z [command] sudo sysctl -w net.core.somaxconn=1024\n2026-01-24T09:14:05Z ##[endgroup]\n2026-01-24T09:14:06Z ##[group]Gather telemetry\n2026-01-24T09:14:07Z Probing 12 devices for CPU, memory, and network stats...\n2026-01-24T09:14:08Z \u{1b}[32m✓\u{1b}[0m CPU OK, \u{1b}[33mWARN\u{1b}[0m jitter spikes detected\n2026-01-24T09:14:09Z Streaming telemetry to s3://demo-edge-diagnostics/tmp/42101.log\n2026-01-24T09:14:10Z Token: ***\n2026-01-24T09:14:11Z ##[endgroup]\n2026-01-24T09:14:12Z ::notice file=src/agents.rs,line=88,title=Telemetry::Collected 12/12 device snapshots".to_string(),
        );
        logs_map.insert(
            42_002,
            "2026-01-24T09:15:01Z ##[group]Aggregate report\n2026-01-24T09:15:02Z [command] python tools/aggregate.py --input s3://demo-edge-diagnostics/tmp/42101.log --output report.pdf\n2026-01-24T09:15:03Z \u{1b}[36mℹ\u{1b}[0m Loading metric baselines from cache\n2026-01-24T09:15:04Z ::debug::Cache hit: baseline-2024-11-05\n2026-01-24T09:15:05Z ::warning file=tools/aggregate.py,line=214,title=Outlier::Jitter spike beyond 2.5σ (ignored)\n2026-01-24T09:15:06Z Rendering PDF summary with 4 charts\n2026-01-24T09:15:07Z Uploading artifact edge-report.zip\n2026-01-24T09:15:08Z ##[endgroup]\n2026-01-24T09:15:09Z ::notice::Report uploaded to workflow artifacts".to_string(),
        );

        logs_map.insert(
            43_001,
            "2026-01-24T10:02:01Z ##[group]Checkout\n2026-01-24T10:02:02Z [command] git fetch --depth=1 origin main\n2026-01-24T10:02:03Z [command] git checkout 8d3c1f4\n2026-01-24T10:02:04Z ##[endgroup]\n2026-01-24T10:02:05Z ##[group]Toolchain\n2026-01-24T10:02:06Z [command] rustup override set stable\n2026-01-24T10:02:07Z [command] cargo fmt --check\n2026-01-24T10:02:08Z [command] cargo clippy --all-targets --all-features\n2026-01-24T10:02:09Z \u{1b}[32m✓\u{1b}[0m clippy warnings: 0\n2026-01-24T10:02:10Z ##[endgroup]\n2026-01-24T10:02:11Z ##[group]Build & test\n2026-01-24T10:02:12Z [command] cargo build --workspace\n2026-01-24T10:02:13Z build artifacts: target/debug (18 crates)\n2026-01-24T10:02:14Z [command] cargo test --workspace --tests\n2026-01-24T10:02:15Z tests passed: 82\n2026-01-24T10:02:16Z ##[endgroup]\n2026-01-24T10:02:17Z ##[group]Artifacts\n2026-01-24T10:02:18Z Compressing artifacts (xz)\n2026-01-24T10:02:19Z Uploading artifacts: debug binaries\n2026-01-24T10:02:20Z Uploading artifacts: coverage/lcov.info\n2026-01-24T10:02:21Z Signing manifest (test key)\n2026-01-24T10:02:22Z Token: ***\n2026-01-24T10:02:23Z ##[endgroup]\n2026-01-24T10:02:24Z ::notice::Pipeline complete".to_string(),
        );
        logs_map.insert(
            43_002,
            "2026-01-24T10:05:31Z ##[group]Test matrix\n2026-01-24T10:05:32Z [command] cargo test --workspace\n2026-01-24T10:05:33Z Running auth tests (12)\n2026-01-24T10:05:34Z Running cache tests (18)\n2026-01-24T10:05:35Z Running UI helpers tests (20)\n2026-01-24T10:05:36Z ::debug::Shard 2/4 finished in 12s\n2026-01-24T10:05:37Z \u{1b}[32m✓\u{1b}[0m integration smoke tests (4)\n2026-01-24T10:05:38Z ##[endgroup]\n2026-01-24T10:05:39Z ##[group]Coverage\n2026-01-24T10:05:40Z Collecting coverage data\n2026-01-24T10:05:41Z Coverage: lines 86%, branches 79%\n2026-01-24T10:05:42Z Uploading junit.xml artifact\n2026-01-24T10:05:43Z Uploading coverage/lcov.info artifact\n2026-01-24T10:05:44Z ##[endgroup]\n2026-01-24T10:05:45Z ::notice title=Summary::Test phase complete".to_string(),
        );
        logs_map.insert(
            43_011,
            "2026-01-24T10:11:01Z ##[group]Lint\n2026-01-24T10:11:02Z [command] cargo fmt --check\n2026-01-24T10:11:03Z [command] cargo clippy --all-targets --all-features\n2026-01-24T10:11:04Z ::warning file=src/auth/device.rs,line=12,title=Clippy::unused import: std::time::Instant\n2026-01-24T10:11:05Z ::error file=src/auth/device.rs,line=12,title=Clippy::lint failure\n2026-01-24T10:11:06Z ##[endgroup]\n2026-01-24T10:11:07Z \u{1b}[31mError:\u{1b}[0m clippy failed, skipping build\n2026-01-24T10:11:08Z Running minimal unit tests for context\n2026-01-24T10:11:09Z Tests executed: auth (12), storage (4), cache (4)\n2026-01-24T10:11:10Z Uploading junit.xml (partial)\n2026-01-24T10:11:11Z ::notice title=Summary::Lint job complete (failed)".to_string(),
        );
        logs_map.insert(
            43_021,
            "2026-01-24T10:18:01Z Waiting for runner to pick up publish job...\n2026-01-24T10:18:08Z [command] gh release create v1.0.2 ./dist/*.zip --notes-file RELEASE.md\n2026-01-24T10:18:12Z Build artifacts ready for release."
                .to_string(),
        );
        logs_map.insert(
            44_001,
            "2026-01-24T11:02:45Z ##[group]Terraform plan\n2026-01-24T11:02:46Z [command] terraform plan -out=tfplan\n2026-01-24T11:02:47Z \u{1b}[33mWarning:\u{1b}[0m drift detected in IAM policy attachments\n2026-01-24T11:02:48Z \u{1b}[31mError:\u{1b}[0m requires manual approval in prod account\n2026-01-24T11:02:49Z ##[endgroup]\n2026-01-24T11:02:50Z ::error title=Infra::Terraform plan failed".to_string(),
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

    pub(super) fn clone_repos(&self) -> Vec<Repo> {
        self.repos.clone()
    }

    pub(super) fn clone_rate_limit(&self) -> RateLimitInfo {
        self.rate_limit.clone()
    }

    pub(super) fn actions_enabled(&self, owner: &str, repo: &str) -> bool {
        let key = Self::repo_key(owner, repo);
        self.actions_enabled.contains(&key)
    }

    pub(super) fn clone_branches(&self, owner: &str, repo: &str) -> Vec<Branch> {
        let key = Self::repo_key(owner, repo);
        self.branches.get(&key).cloned().unwrap_or_default()
    }

    pub(super) fn clone_workflows(&self, owner: &str, repo: &str) -> Vec<Workflow> {
        let key = Self::repo_key(owner, repo);
        self.workflows.get(&key).cloned().unwrap_or_default()
    }

    pub(super) fn clone_runs(&self, owner: &str, repo: &str, workflow_id: i64) -> Vec<WorkflowRun> {
        let key = Self::workflow_key(owner, repo, workflow_id);
        self.runs.get(&key).cloned().unwrap_or_default()
    }

    pub(super) fn clone_jobs(&self, run_id: i64) -> Vec<Job> {
        self.jobs.get(&run_id).cloned().unwrap_or_default()
    }

    pub(super) fn clone_logs(&self, job_id: i64) -> Option<String> {
        self.logs.get(&job_id).cloned()
    }

    pub(super) fn add_manual_run(
        &mut self,
        owner: &str,
        name: &str,
        workflow_id: i64,
        reference: &str,
    ) -> GitHubErrorResult<()> {
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
            "2026-01-24T12:01:01Z ##[group]Manual dispatch\n2026-01-24T12:01:02Z [command] gh workflow run --ref demo-branch\n2026-01-24T12:01:03Z Recording branch in demo state\n2026-01-24T12:01:04Z Token: ***\n2026-01-24T12:01:05Z ##[endgroup]\n2026-01-24T12:01:06Z ::notice title=Dispatch::Run will appear once started".to_string(),
        );
        self.branches
            .entry(Self::repo_key(owner, name))
            .or_default()
            .push(Branch {
                name: reference.to_string(),
                commit: BranchCommit {
                    sha: format!("{:x}", run_id),
                },
                protected: false,
            });

        Ok(())
    }

    pub(super) fn update_run_status(
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

    fn repo_key(owner: &str, name: &str) -> RepoKey {
        RepoKey::new(owner, name)
    }

    fn workflow_key(owner: &str, name: &str, workflow_id: i64) -> WorkflowKey {
        WorkflowKey::new(owner, name, workflow_id)
    }
}

pub(super) type GitHubErrorResult<T> = Result<T, GitHubError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_manual_run_inserts_job_and_run() {
        let mut data = DemoData::new();
        let owner = "demo-org";
        let repo = "actioneer-demo-app";
        let workflow_id = 21_001;

        assert!(
            data.clone_runs(owner, repo, workflow_id)
                .iter()
                .any(|run| run.display_title.as_deref() == Some("CI • main"))
        );

        data.add_manual_run(owner, repo, workflow_id, "demo-branch")
            .expect("manual run creation should succeed");

        let runs = data.clone_runs(owner, repo, workflow_id);
        assert!(
            runs.iter()
                .any(|run| run.head_branch.as_deref() == Some("demo-branch"))
        );
    }
}
