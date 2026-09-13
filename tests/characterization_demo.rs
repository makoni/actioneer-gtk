//! Characterization of the demo data surface.
//!
//! Phase 2 replaced the process-global demo switch with `GitHubGateway::demo()`.
//! This file was written *before* that move and its call sites were re-pointed
//! by it; **every expected value is unchanged**, which is what makes the move
//! provably behaviour-preserving. `git log -p` on this file shows call-site
//! edits and one deliberate exception, marked at the bottom.
//!
//! Still a single test: it no longer has to be, now that each gateway owns its
//! own fixtures rather than sharing a process global, but keeping the shape
//! keeps the diff readable.

mod common;

use common::*;

#[tokio::test]
async fn demo_surface_is_stable() {
    // ---- a demo gateway seeds itself --------------------------------------
    let gateway = actioneer::gateway::GitHubGateway::demo();
    assert!(gateway.is_demo(), "a demo gateway reports itself as demo");
    let (repos, seeded_rate) = gateway.demo_seed().expect("a demo gateway always seeds");
    assert!(seeded_rate.is_some(), "the seed carries the rate limit");

    let names: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    assert_eq!(names, DEMO_REPOS, "demo repository set and order");

    // ---- per-repository reads ---------------------------------------------
    for (index, repo) in repos.iter().enumerate() {
        let (owner, name) = (repo.owner.login.as_str(), repo.name.as_str());

        let workflows = gateway
            .list_workflows(owner, name)
            .await
            .unwrap_or_else(|_| panic!("workflows for {owner}/{name}"));
        assert_eq!(
            workflows.len(),
            DEMO_WORKFLOW_COUNTS[index],
            "workflow count for {owner}/{name}"
        );

        let runs = gateway
            .list_repository_runs(owner, name)
            .await
            .unwrap_or_else(|_| panic!("runs for {owner}/{name}"));
        assert_eq!(
            runs.len(),
            DEMO_REPO_RUN_COUNTS[index],
            "run count for {owner}/{name}"
        );

        let branches = gateway
            .list_branches(owner, name)
            .await
            .unwrap_or_else(|_| panic!("branches for {owner}/{name}"));
        let branch_names: Vec<String> = branches.iter().map(|b| b.name.clone()).collect();
        assert_eq!(branch_names, DEMO_BRANCHES, "branches for {owner}/{name}");

        assert_eq!(
            gateway.is_actions_enabled(owner, name).await.ok(),
            Some(true),
            "actions enabled for {owner}/{name}"
        );
    }

    // ---- the first repository's workflows, by id and name -----------------
    let (owner, name) = DEMO_REPO_MAIN;
    let workflows = gateway
        .list_workflows(owner, name)
        .await
        .expect("main repo workflows");
    let pairs: Vec<(i64, String)> = workflows.iter().map(|w| (w.id, w.name.clone())).collect();
    let expected: Vec<(i64, String)> = DEMO_MAIN_WORKFLOWS
        .iter()
        .map(|(id, n)| (*id, (*n).to_string()))
        .collect();
    assert_eq!(pairs, expected, "main repo workflow ids and names");

    // Runs filtered by workflow are a subset of the repository's runs.
    let ci_runs = gateway
        .list_runs(owner, name, DEMO_MAIN_WORKFLOWS[0].0)
        .await
        .expect("runs for the CI workflow");
    assert!(!ci_runs.is_empty(), "the CI workflow has runs");
    assert!(
        ci_runs.len() <= DEMO_REPO_RUN_COUNTS[0],
        "per-workflow runs are a subset of repository runs"
    );

    // ---- the newest run, its jobs and their logs --------------------------
    let newest = gateway
        .list_repository_runs(owner, name)
        .await
        .expect("main repo runs")[0]
        .clone();
    assert_eq!(newest.id, DEMO_NEWEST_RUN.0, "newest run id");
    assert_eq!(
        newest.status.as_deref(),
        Some(DEMO_NEWEST_RUN.1),
        "newest run status"
    );

    let jobs = gateway
        .list_jobs(owner, name, newest.id)
        .await
        .expect("jobs of the newest run");
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

    let logs = gateway
        .get_job_logs(owner, name, jobs[0].id)
        .await
        .expect("logs of the first job");
    assert!(!logs.is_empty(), "job logs are not empty");
    assert!(
        logs.contains(DEMO_LOG_MARKER),
        "job logs carry the {DEMO_LOG_MARKER:?} marker, got: {logs:?}"
    );

    // ---- the canonical failed and successful runs -------------------------
    let (lab_owner, lab_name) = DEMO_REPO_LAB;
    let lab_run = &gateway
        .list_repository_runs(lab_owner, lab_name)
        .await
        .expect("lab runs")[0];
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
    let edge_run = &gateway
        .list_repository_runs(edge_owner, edge_name)
        .await
        .expect("edge runs")[0];
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
    let rate = gateway.rate_limit_info().expect("demo rate limit");
    assert_eq!(
        (rate.limit, rate.remaining),
        DEMO_RATE_LIMIT,
        "demo rate limit is a full budget"
    );
    assert_eq!(
        gateway
            .get_workflow_dispatch_inputs(owner, name, "ci.yml", None)
            .await
            .expect("dispatch inputs")
            .len(),
        DEMO_DISPATCH_INPUT_COUNT,
        "workflow_dispatch input count"
    );

    // ---- mutators: the demo backend must share state across callers -------
    // Phase 2 turns the global into a `DemoBackend`. If that backend ends up
    // owning its data by value and being cloned per caller, these mutations
    // would land on a copy nobody reads. These four assertions are what catches
    // that.
    let dispatched = gateway
        .dispatch_workflow(
            owner,
            name,
            &DEMO_MAIN_WORKFLOWS[0].0.to_string(),
            DEMO_BRANCHES[0],
            None,
        )
        .await
        .expect("dispatch_workflow succeeds in demo mode")
        .expect("demo dispatch returns the new run");
    let after_dispatch = gateway
        .list_repository_runs(owner, name)
        .await
        .expect("runs after dispatch");
    assert_eq!(
        after_dispatch.len(),
        DEMO_REPO_RUN_COUNTS[0] + 1,
        "dispatch_workflow adds a run visible to the next read"
    );
    assert_eq!(
        after_dispatch[0].id, dispatched.id,
        "the dispatched run is the newest one"
    );

    gateway
        .cancel_run(edge_owner, edge_name, DEMO_SUCCESS_RUN.0)
        .await
        .expect("cancel_run succeeds in demo mode");
    let cancelled = &gateway
        .list_repository_runs(edge_owner, edge_name)
        .await
        .expect("edge runs")[0];
    assert_eq!(
        (cancelled.status.as_deref(), cancelled.conclusion.as_deref()),
        (Some("completed"), Some("cancelled")),
        "cancel_run is visible to the next read"
    );

    gateway
        .rerun_workflow(lab_owner, lab_name, DEMO_FAILED_RUN.0)
        .await
        .expect("rerun_workflow succeeds in demo mode");
    let requeued = &gateway
        .list_repository_runs(lab_owner, lab_name)
        .await
        .expect("lab runs")[0];
    assert_eq!(
        requeued.status.as_deref(),
        Some("queued"),
        "rerun_workflow requeues the run"
    );

    gateway
        .rerun_failed_jobs(lab_owner, lab_name, DEMO_FAILED_RUN.0)
        .await
        .expect("rerun_failed_jobs succeeds in demo mode");
    let restarted = &gateway
        .list_repository_runs(lab_owner, lab_name)
        .await
        .expect("lab runs")[0];
    assert_eq!(
        restarted.status.as_deref(),
        Some("in_progress"),
        "rerun_failed_jobs moves the run to in_progress"
    );

    // ---- leaving demo mode ------------------------------------------------
    // The one assertion this phase *did* change, because the mechanism it named
    // is gone: there is no global to switch off. Demo mode now ends by dropping
    // the demo gateway, and a live gateway is simply not a demo one.
    let live = actioneer::gateway::GitHubGateway::live(None).expect("a live gateway builds");
    assert!(!live.is_demo(), "a live gateway is not demo");
    assert!(
        live.demo_seed().is_none(),
        "only a demo gateway can seed without I/O"
    );

    // Each demo gateway owns its own fixtures: a fresh one is unaffected by the
    // mutations above. That is the property the Arc-shared backend must keep.
    let fresh = actioneer::gateway::GitHubGateway::demo();
    let fresh_runs = fresh
        .list_repository_runs(owner, name)
        .await
        .expect("runs from a fresh gateway");
    assert_eq!(
        fresh_runs.len(),
        DEMO_REPO_RUN_COUNTS[0],
        "a fresh demo gateway starts from the pristine fixtures"
    );
}
