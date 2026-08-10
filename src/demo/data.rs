use crate::api::GitHubError;
use crate::api::models::{
    Branch, BranchCommit, Job, JobStep, RateLimitInfo, Repo, RepoPermissions, User, Workflow,
    WorkflowRun,
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
        let workflow_appimage = Workflow {
            id: 21_003,
            name: "AppImage CI".to_string(),
            path: ".github/workflows/appimage.yml".to_string(),
        };
        let workflow_snap = Workflow {
            id: 21_004,
            name: "Snap CI".to_string(),
            path: ".github/workflows/snap.yml".to_string(),
        };
        let workflow_lockfile = Workflow {
            id: 21_005,
            name: "Lockfile Sync".to_string(),
            path: ".github/workflows/lockfile-sync.yml".to_string(),
        };
        let workflow_publish = Workflow {
            id: 21_006,
            name: "Publish Release".to_string(),
            path: ".github/workflows/publish.yml".to_string(),
        };
        let workflow_copilot_review = Workflow {
            id: 21_007,
            name: "Copilot code review".to_string(),
            path: ".github/workflows/copilot-review.yml".to_string(),
        };
        let workflow_copilot_agent = Workflow {
            id: 21_008,
            name: "Copilot coding agent".to_string(),
            path: ".github/workflows/copilot-agent.yml".to_string(),
        };
        workflows_map.insert(
            key_one.clone(),
            vec![
                workflow_ci.clone(),
                workflow_release.clone(),
                workflow_appimage.clone(),
                workflow_snap.clone(),
                workflow_lockfile.clone(),
                workflow_publish.clone(),
                workflow_copilot_review.clone(),
                workflow_copilot_agent.clone(),
            ],
        );

        let repo_one_full = repo_one.full_name.clone();
        let runs_ci = vec![
            demo_run(
                &repo_one_full,
                30_108,
                134,
                "CI",
                "CI • main",
                "main",
                "in_progress",
                None,
                75,
                None,
                "push",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_107,
                133,
                "CI",
                "CI • feature/refactor",
                "feature/refactor",
                "in_progress",
                None,
                480,
                None,
                "pull_request",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_106,
                132,
                "CI",
                "CI • main",
                "main",
                "queued",
                None,
                720,
                None,
                "push",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_105,
                131,
                "CI",
                "CI • release/hotfix",
                "release/hotfix",
                "queued",
                None,
                900,
                None,
                "workflow_dispatch",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_103,
                130,
                "CI",
                "CI • main",
                "main",
                "completed",
                Some("success"),
                7_200,
                Some(220),
                "push",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_104,
                129,
                "CI",
                "CI • feature/login",
                "feature/login",
                "completed",
                Some("cancelled"),
                10_800,
                Some(70),
                "pull_request",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_101,
                128,
                "CI",
                "CI • main",
                "main",
                "completed",
                Some("success"),
                172_800,
                Some(300),
                "push",
                "makoni",
            ),
            demo_run(
                &repo_one_full,
                30_102,
                127,
                "CI",
                "CI • feature/login",
                "feature/login",
                "completed",
                Some("failure"),
                259_200,
                Some(360),
                "pull_request",
                "makoni",
            ),
        ];

        let runs_release = vec![demo_run(
            &repo_one_full,
            30_201,
            46,
            "Release",
            "Release • v1.0.0",
            "main",
            "completed",
            Some("success"),
            7_200,
            Some(95),
            "workflow_dispatch",
            "makoni",
        )];

        let runs_appimage = vec![demo_run(
            &repo_one_full,
            30_211,
            128,
            "AppImage CI",
            "Bundle AppImage",
            "main",
            "completed",
            Some("success"),
            10_800,
            Some(245),
            "push",
            "makoni",
        )];

        let runs_snap = vec![demo_run(
            &repo_one_full,
            30_221,
            21,
            "Snap CI",
            "Build snap package",
            "main",
            "completed",
            Some("success"),
            93_600,
            Some(400),
            "push",
            "makoni",
        )];

        let runs_lockfile = vec![demo_run(
            &repo_one_full,
            30_231,
            12,
            "Lockfile Sync",
            "Sync lockfiles",
            "main",
            "completed",
            Some("failure"),
            259_200,
            Some(45),
            "schedule",
            "dependabot",
        )];

        let runs_publish = vec![demo_run(
            &repo_one_full,
            30_241,
            9,
            "Publish Release",
            "Publish v1.4.0",
            "v1.4.0",
            "completed",
            Some("success"),
            432_000,
            Some(600),
            "release",
            "makoni",
        )];

        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_ci.id),
            runs_ci.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_release.id),
            runs_release.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_appimage.id),
            runs_appimage.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_snap.id),
            runs_snap.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_lockfile.id),
            runs_lockfile.clone(),
        );
        runs_map.insert(
            WorkflowKey::new(&repo_one.owner.login, &repo_one.name, workflow_publish.id),
            runs_publish.clone(),
        );

        // Jobs for the running CI run #134: one finished job, one in progress.
        jobs_map.insert(
            30_108,
            vec![
                demo_job(
                    &repo_one_full,
                    43_081,
                    30_108,
                    "prepare",
                    "completed",
                    Some("success"),
                    75,
                    Some(18),
                    vec![
                        demo_step(1, "Set up job", "completed", Some("success"), 75, Some(1)),
                        demo_step(
                            2,
                            "Run actions/checkout@v6.0.2",
                            "completed",
                            Some("success"),
                            74,
                            Some(2),
                        ),
                        demo_step(
                            3,
                            "Install flatpak-builder",
                            "completed",
                            Some("success"),
                            72,
                            Some(14),
                        ),
                        demo_step(4, "Complete job", "completed", Some("success"), 58, Some(0)),
                    ],
                ),
                demo_job(
                    &repo_one_full,
                    43_082,
                    30_108,
                    "bundle",
                    "in_progress",
                    None,
                    55,
                    None,
                    vec![
                        demo_step(1, "Set up job", "completed", Some("success"), 55, Some(1)),
                        demo_step(2, "Build flatpak bundle", "in_progress", None, 48, None),
                        demo_step(3, "Upload artifact", "queued", None, 0, None),
                        demo_step(4, "Complete job", "queued", None, 0, None),
                    ],
                ),
            ],
        );

        jobs_map.insert(
            30_107,
            vec![demo_job(
                &repo_one_full,
                43_071,
                30_107,
                "Integration tests",
                "in_progress",
                None,
                480,
                None,
                vec![
                    demo_step(1, "Set up job", "completed", Some("success"), 480, Some(2)),
                    demo_step(2, "Run integration suite", "in_progress", None, 470, None),
                ],
            )],
        );

        jobs_map.insert(
            30_106,
            vec![demo_job(
                &repo_one_full,
                43_061,
                30_106,
                "Queue tests",
                "queued",
                None,
                0,
                None,
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_105,
            vec![demo_job(
                &repo_one_full,
                43_051,
                30_105,
                "Queue build",
                "queued",
                None,
                0,
                None,
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_103,
            vec![demo_job(
                &repo_one_full,
                43_031,
                30_103,
                "Build & package",
                "completed",
                Some("success"),
                7_200,
                Some(215),
                vec![
                    demo_step(
                        1,
                        "Set up job",
                        "completed",
                        Some("success"),
                        7_200,
                        Some(2),
                    ),
                    demo_step(
                        2,
                        "Run actions/checkout@v6.0.2",
                        "completed",
                        Some("success"),
                        7_198,
                        Some(2),
                    ),
                    demo_step(
                        3,
                        "cargo build --release",
                        "completed",
                        Some("success"),
                        7_196,
                        Some(180),
                    ),
                    demo_step(
                        4,
                        "Package artifacts",
                        "completed",
                        Some("success"),
                        7_016,
                        Some(28),
                    ),
                    demo_step(
                        5,
                        "Complete job",
                        "completed",
                        Some("success"),
                        6_988,
                        Some(0),
                    ),
                ],
            )],
        );

        jobs_map.insert(
            30_104,
            vec![demo_job(
                &repo_one_full,
                43_041,
                30_104,
                "Test suite",
                "completed",
                Some("cancelled"),
                10_800,
                Some(60),
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_101,
            vec![
                demo_job(
                    &repo_one_full,
                    43_001,
                    30_101,
                    "Build",
                    "completed",
                    Some("success"),
                    172_800,
                    Some(155),
                    Vec::new(),
                ),
                demo_job(
                    &repo_one_full,
                    43_002,
                    30_101,
                    "Tests",
                    "completed",
                    Some("success"),
                    172_645,
                    Some(135),
                    Vec::new(),
                ),
            ],
        );

        jobs_map.insert(
            30_102,
            vec![demo_job(
                &repo_one_full,
                43_011,
                30_102,
                "Lint & unit tests",
                "completed",
                Some("failure"),
                259_200,
                Some(350),
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_201,
            vec![demo_job(
                &repo_one_full,
                43_021,
                30_201,
                "Publish artifacts",
                "completed",
                Some("success"),
                7_200,
                Some(90),
                vec![
                    demo_step(
                        1,
                        "Set up job",
                        "completed",
                        Some("success"),
                        7_200,
                        Some(2),
                    ),
                    demo_step(
                        2,
                        "Upload release assets",
                        "completed",
                        Some("success"),
                        7_198,
                        Some(80),
                    ),
                    demo_step(
                        3,
                        "Complete job",
                        "completed",
                        Some("success"),
                        7_118,
                        Some(0),
                    ),
                ],
            )],
        );

        jobs_map.insert(
            30_211,
            vec![demo_job(
                &repo_one_full,
                43_211,
                30_211,
                "Build AppImage",
                "completed",
                Some("success"),
                10_800,
                Some(240),
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_221,
            vec![demo_job(
                &repo_one_full,
                43_221,
                30_221,
                "Build snap",
                "completed",
                Some("success"),
                93_600,
                Some(395),
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_231,
            vec![demo_job(
                &repo_one_full,
                43_231,
                30_231,
                "Sync lockfiles",
                "completed",
                Some("failure"),
                259_200,
                Some(40),
                Vec::new(),
            )],
        );

        jobs_map.insert(
            30_241,
            vec![demo_job(
                &repo_one_full,
                43_241,
                30_241,
                "Publish release",
                "completed",
                Some("success"),
                432_000,
                Some(590),
                Vec::new(),
            )],
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
            workflow_id: None,
            name: Some("Infrastructure".to_string()),
            display_title: Some("Infra • terraform plan".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("failure".to_string()),
            run_started_at: Some("2025-10-26T15:00:00Z".to_string()),
            event: Some("workflow_dispatch".to_string()),
            created_at: Some("2025-10-26T15:00:00Z".to_string()),
            updated_at: Some("2025-10-26T15:04:00Z".to_string()),
            actor: None,

            head_commit: None,

            triggering_actor: None,

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
                steps: Vec::new(),
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
            workflow_id: None,
            name: Some("Edge Diagnostics".to_string()),
            display_title: Some("Edge Diagnostics • nightly".to_string()),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            run_started_at: Some("2025-10-27T02:00:00Z".to_string()),
            event: Some("schedule".to_string()),
            created_at: Some("2025-10-27T02:00:00Z".to_string()),
            updated_at: Some("2025-10-27T02:06:45Z".to_string()),
            actor: None,

            head_commit: None,

            triggering_actor: None,

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
                    steps: Vec::new(),
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
                    steps: Vec::new(),
                    html_url: Some(
                        "https://github.com/demo-team/device-edge/runs/42002".to_string(),
                    ),
                },
            ],
        );

        logs_map.insert(42_001, include_str!("logs/job-42001.log").to_string());
        logs_map.insert(42_002, include_str!("logs/job-42002.log").to_string());

        logs_map.insert(43_001, include_str!("logs/job-43001.log").to_string());
        logs_map.insert(43_002, include_str!("logs/job-43002.log").to_string());
        logs_map.insert(43_011, include_str!("logs/job-43011.log").to_string());
        logs_map.insert(43_031, include_str!("logs/job-43031.log").to_string());
        logs_map.insert(43_041, include_str!("logs/job-43041.log").to_string());
        logs_map.insert(43_051, include_str!("logs/job-43051.log").to_string());
        logs_map.insert(43_061, include_str!("logs/job-43061.log").to_string());
        logs_map.insert(43_071, include_str!("logs/job-43071.log").to_string());
        logs_map.insert(43_081, include_str!("logs/job-43081.log").to_string());
        logs_map.insert(43_021, include_str!("logs/job-43021.log").to_string());
        logs_map.insert(44_001, include_str!("logs/job-44001.log").to_string());
        logs_map.insert(43_082, include_str!("logs/job-43081.log").to_string());
        logs_map.insert(43_211, include_str!("logs/job-43001.log").to_string());
        logs_map.insert(43_221, include_str!("logs/job-43002.log").to_string());
        logs_map.insert(43_231, include_str!("logs/job-43011.log").to_string());
        logs_map.insert(43_241, include_str!("logs/job-43021.log").to_string());

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

    pub(super) fn clone_repo_runs(&self, owner: &str, repo: &str) -> Vec<WorkflowRun> {
        let repo_key = Self::repo_key(owner, repo);
        let mut runs = self
            .runs
            .iter()
            .filter(|(workflow_key, _)| workflow_key.repo == repo_key)
            .flat_map(|(workflow_key, workflow_runs)| {
                workflow_runs.iter().cloned().map(move |mut run| {
                    run.workflow_id = Some(workflow_key.workflow_id);
                    run
                })
            })
            .collect::<Vec<_>>();

        runs.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });

        runs
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
    ) -> GitHubErrorResult<WorkflowRun> {
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
            workflow_id: None,
            name: Some("Manual Dispatch".to_string()),
            display_title: Some(format!("{} • {}", reference, reference)),
            head_branch: Some(reference.to_string()),
            head_commit: None,
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
            actor: None,
            triggering_actor: None,
        };

        runs.insert(0, new_run.clone());

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
            steps: Vec::new(),
            html_url: None,
        };
        self.jobs.insert(run_id, vec![job]);
        self.logs
            .insert(job_id, include_str!("logs/manual-dispatch.log").to_string());
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

        Ok(new_run)
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

/// RFC 3339 timestamp `seconds` in the past (keeps demo data looking fresh).
fn iso_ago(seconds: i64) -> String {
    (Utc::now() - chrono::Duration::seconds(seconds)).to_rfc3339()
}

#[allow(clippy::too_many_arguments)]
fn demo_run(
    repo_full: &str,
    id: i64,
    number: i64,
    workflow_name: &str,
    title: &str,
    branch: &str,
    status: &str,
    conclusion: Option<&str>,
    started_secs_ago: i64,
    duration_secs: Option<i64>,
    event: &str,
    actor: &str,
) -> WorkflowRun {
    let queued = run_started_at_is_missing(status);
    let started = if queued {
        None
    } else {
        Some(iso_ago(started_secs_ago))
    };
    let updated = match (queued, duration_secs) {
        (true, _) => Some(iso_ago(started_secs_ago.saturating_sub(15).max(0))),
        (false, Some(duration)) => Some(iso_ago((started_secs_ago - duration).max(0))),
        (false, None) => Some(iso_ago(0)),
    };

    WorkflowRun {
        id,
        run_number: Some(number),
        workflow_id: None,
        name: Some(workflow_name.to_string()),
        display_title: Some(title.to_string()),
        head_branch: Some(branch.to_string()),
        head_commit: None,
        status: Some(status.to_string()),
        conclusion: conclusion.map(str::to_string),
        run_started_at: started.clone(),
        event: Some(event.to_string()),
        created_at: Some(started.unwrap_or_else(|| iso_ago(started_secs_ago))),
        updated_at: updated,
        html_url: Some(format!(
            "https://github.com/{}/actions/runs/{}",
            repo_full, id
        )),
        actor: Some(User {
            login: actor.to_string(),
        }),
        triggering_actor: None,
    }
}

fn run_started_at_is_missing(status: &str) -> bool {
    matches!(status, "queued" | "waiting")
}

#[allow(clippy::too_many_arguments)]
fn demo_job(
    repo_full: &str,
    id: i64,
    run_id: i64,
    name: &str,
    status: &str,
    conclusion: Option<&str>,
    started_secs_ago: i64,
    duration_secs: Option<i64>,
    steps: Vec<JobStep>,
) -> Job {
    let queued = run_started_at_is_missing(status);
    Job {
        id,
        run_id,
        status: Some(status.to_string()),
        conclusion: conclusion.map(str::to_string),
        started_at: if queued {
            None
        } else {
            Some(iso_ago(started_secs_ago))
        },
        completed_at: duration_secs.map(|d| iso_ago((started_secs_ago - d).max(0))),
        name: Some(name.to_string()),
        steps,
        html_url: Some(format!("https://github.com/{}/runs/{}", repo_full, id)),
    }
}

fn demo_step(
    number: i64,
    name: &str,
    status: &str,
    conclusion: Option<&str>,
    started_secs_ago: i64,
    duration_secs: Option<i64>,
) -> JobStep {
    let queued = run_started_at_is_missing(status);
    JobStep {
        name: Some(name.to_string()),
        status: Some(status.to_string()),
        conclusion: conclusion.map(str::to_string),
        number: Some(number),
        started_at: if queued {
            None
        } else {
            Some(iso_ago(started_secs_ago))
        },
        completed_at: duration_secs.map(|d| iso_ago((started_secs_ago - d).max(0))),
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

        let created_run = data
            .add_manual_run(owner, repo, workflow_id, "demo-branch")
            .expect("manual run creation should succeed");

        let runs = data.clone_runs(owner, repo, workflow_id);
        assert_eq!(created_run.head_branch.as_deref(), Some("demo-branch"));
        assert!(
            runs.iter()
                .any(|run| run.head_branch.as_deref() == Some("demo-branch"))
        );
    }
}
