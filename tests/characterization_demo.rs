//! Characterization of the demo data surface.
//!
//! Phase 2 of the architecture refactor replaces the process-global demo switch
//! with a `GitHubGateway::demo()` backend. That is a behaviour-preserving move
//! the compiler cannot check, so this file pins what the demo surface returns
//! *before* the move. Afterwards the calls below are re-pointed at the gateway
//! and **the expected values must not change** — a changed assertion means the
//! refactor changed behaviour.
//!
//! Everything lives in one `#[test]` on purpose: demo state is a process-global
//! `OnceLock<Mutex<..>>`, and libtest runs tests in parallel, so two tests
//! toggling `enable()`/`disable()` would race.

mod common;

use common::*;

#[test]
fn demo_surface_is_stable() {
    // ---- enable -----------------------------------------------------------
    let repos = actioneer::demo::enable();
    assert!(
        actioneer::demo::is_active(),
        "enable() must activate demo mode"
    );

    let names: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    assert_eq!(names, DEMO_REPOS, "demo repository set and order");

    // ---- per-repository reads ---------------------------------------------
    for (index, repo) in repos.iter().enumerate() {
        let (owner, name) = (repo.owner.login.as_str(), repo.name.as_str());

        let workflows = actioneer::demo::list_workflows(owner, name)
            .unwrap_or_else(|| panic!("workflows for {owner}/{name}"));
        assert_eq!(
            workflows.len(),
            DEMO_WORKFLOW_COUNTS[index],
            "workflow count for {owner}/{name}"
        );

        let runs = actioneer::demo::list_repository_runs(owner, name)
            .unwrap_or_else(|| panic!("runs for {owner}/{name}"));
        assert_eq!(
            runs.len(),
            DEMO_REPO_RUN_COUNTS[index],
            "run count for {owner}/{name}"
        );

        let branches = actioneer::demo::list_branches(owner, name)
            .unwrap_or_else(|| panic!("branches for {owner}/{name}"));
        let branch_names: Vec<String> = branches.iter().map(|b| b.name.clone()).collect();
        assert_eq!(branch_names, DEMO_BRANCHES, "branches for {owner}/{name}");

        assert_eq!(
            actioneer::demo::is_actions_enabled(owner, name),
            Some(true),
            "actions enabled for {owner}/{name}"
        );
    }

    // ---- the first repository's workflows, by id and name -----------------
    let (owner, name) = DEMO_REPO_MAIN;
    let workflows = actioneer::demo::list_workflows(owner, name).expect("main repo workflows");
    let pairs: Vec<(i64, String)> = workflows.iter().map(|w| (w.id, w.name.clone())).collect();
    let expected: Vec<(i64, String)> = DEMO_MAIN_WORKFLOWS
        .iter()
        .map(|(id, n)| (*id, (*n).to_string()))
        .collect();
    assert_eq!(pairs, expected, "main repo workflow ids and names");

    // Runs filtered by workflow are a subset of the repository's runs.
    let ci_runs = actioneer::demo::list_runs(owner, name, DEMO_MAIN_WORKFLOWS[0].0)
        .expect("runs for the CI workflow");
    assert!(!ci_runs.is_empty(), "the CI workflow has runs");
    assert!(
        ci_runs.len() <= DEMO_REPO_RUN_COUNTS[0],
        "per-workflow runs are a subset of repository runs"
    );

    // ---- the newest run, its jobs and their logs --------------------------
    let newest =
        actioneer::demo::list_repository_runs(owner, name).expect("main repo runs")[0].clone();
    assert_eq!(newest.id, DEMO_NEWEST_RUN.0, "newest run id");
    assert_eq!(
        newest.status.as_deref(),
        Some(DEMO_NEWEST_RUN.1),
        "newest run status"
    );

    let jobs = actioneer::demo::list_jobs(owner, name, newest.id).expect("jobs of the newest run");
    let job_shapes: Vec<(i64, String, String)> = jobs
        .iter()
        .map(|j| {
            (
                j.id,
                j.name.clone().unwrap_or_default(),
                j.status.clone().unwrap_or_default(),
            )
        })
        .collect();
    let expected_jobs: Vec<(i64, String, String)> = DEMO_NEWEST_RUN_JOBS
        .iter()
        .map(|(id, n, s)| (*id, (*n).to_string(), (*s).to_string()))
        .collect();
    assert_eq!(job_shapes, expected_jobs, "jobs of the newest run");

    let logs = actioneer::demo::job_logs(jobs[0].id).expect("logs of the first job");
    assert!(!logs.is_empty(), "job logs are not empty");
    assert!(
        logs.contains(DEMO_LOG_MARKER),
        "job logs carry the {DEMO_LOG_MARKER:?} marker, got: {logs:?}"
    );

    // ---- the canonical failed and successful runs -------------------------
    let (lab_owner, lab_name) = DEMO_REPO_LAB;
    let lab_run = &actioneer::demo::list_repository_runs(lab_owner, lab_name).expect("lab runs")[0];
    assert_eq!(
        (
            lab_run.id,
            lab_run.status.as_deref(),
            lab_run.conclusion.as_deref()
        ),
        (
            DEMO_FAILED_RUN.0,
            Some(DEMO_FAILED_RUN.1),
            Some(DEMO_FAILED_RUN.2)
        ),
        "the lab repository's run is the canonical failure"
    );

    let (edge_owner, edge_name) = DEMO_REPO_EDGE;
    let edge_run =
        &actioneer::demo::list_repository_runs(edge_owner, edge_name).expect("edge runs")[0];
    assert_eq!(
        (
            edge_run.id,
            edge_run.status.as_deref(),
            edge_run.conclusion.as_deref()
        ),
        (
            DEMO_SUCCESS_RUN.0,
            Some(DEMO_SUCCESS_RUN.1),
            Some(DEMO_SUCCESS_RUN.2)
        ),
        "the edge repository's run is the canonical success"
    );

    // ---- rate limit and dispatch inputs -----------------------------------
    let rate = actioneer::demo::rate_limit_info().expect("demo rate limit");
    assert_eq!(
        (rate.limit, rate.remaining),
        DEMO_RATE_LIMIT,
        "demo rate limit is a full budget"
    );
    assert_eq!(
        actioneer::demo::workflow_dispatch_inputs().len(),
        DEMO_DISPATCH_INPUT_COUNT,
        "workflow_dispatch input count"
    );

    // ---- mutators: the demo backend must share state across callers -------
    // Phase 2 turns the global into a `DemoBackend`. If that backend ends up
    // owning its data by value and being cloned per caller, these mutations
    // would land on a copy nobody reads. These four assertions are what catches
    // that.
    let dispatched =
        actioneer::demo::dispatch_workflow(owner, name, DEMO_MAIN_WORKFLOWS[0].0, DEMO_BRANCHES[0])
            .expect("dispatch_workflow succeeds in demo mode");
    let after_dispatch =
        actioneer::demo::list_repository_runs(owner, name).expect("runs after dispatch");
    assert_eq!(
        after_dispatch.len(),
        DEMO_REPO_RUN_COUNTS[0] + 1,
        "dispatch_workflow adds a run visible to the next read"
    );
    assert_eq!(
        after_dispatch[0].id, dispatched.id,
        "the dispatched run is the newest one"
    );

    actioneer::demo::cancel_run(edge_owner, edge_name, DEMO_SUCCESS_RUN.0)
        .expect("cancel_run succeeds in demo mode");
    let cancelled =
        &actioneer::demo::list_repository_runs(edge_owner, edge_name).expect("edge runs")[0];
    assert_eq!(
        (cancelled.status.as_deref(), cancelled.conclusion.as_deref()),
        (Some("completed"), Some("cancelled")),
        "cancel_run is visible to the next read"
    );

    actioneer::demo::rerun_workflow(lab_owner, lab_name, DEMO_FAILED_RUN.0)
        .expect("rerun_workflow succeeds in demo mode");
    let requeued =
        &actioneer::demo::list_repository_runs(lab_owner, lab_name).expect("lab runs")[0];
    assert_eq!(
        requeued.status.as_deref(),
        Some("queued"),
        "rerun_workflow requeues the run"
    );

    actioneer::demo::rerun_failed_jobs(lab_owner, lab_name, DEMO_FAILED_RUN.0)
        .expect("rerun_failed_jobs succeeds in demo mode");
    let restarted =
        &actioneer::demo::list_repository_runs(lab_owner, lab_name).expect("lab runs")[0];
    assert_eq!(
        restarted.status.as_deref(),
        Some("in_progress"),
        "rerun_failed_jobs moves the run to in_progress"
    );

    // ---- disable ----------------------------------------------------------
    actioneer::demo::disable();
    assert!(
        !actioneer::demo::is_active(),
        "disable() must deactivate demo mode"
    );
    assert!(
        actioneer::demo::list_repos().is_none(),
        "no demo data is served once demo mode is off"
    );
}
