//! Frozen expectations shared by the integration tests.
//!
//! These values were read off `src/demo/data.rs` on 2026-09-13 by running the
//! demo surface and recording what it returned. They are **characterization
//! constants**: a later refactor may change how a test reaches them, but it may
//! not change the values. If a refactor makes one of these assertions fail, the
//! refactor changed behaviour — fix the code, not the constant.

// Each `tests/*.rs` binary compiles its own copy of this module and uses only a
// subset, so unused constants are expected here.
#![allow(dead_code)]

/// The demo backend serves three repositories, in this order.
pub const DEMO_REPOS: [&str; 3] = [
    "demo-org/actioneer-demo-app",
    "demo-labs/workflow-lab",
    "demo-team/device-edge",
];

pub const DEMO_REPO_MAIN: (&str, &str) = ("demo-org", "actioneer-demo-app");
pub const DEMO_REPO_LAB: (&str, &str) = ("demo-labs", "workflow-lab");
pub const DEMO_REPO_EDGE: (&str, &str) = ("demo-team", "device-edge");

/// Workflows per repository, in `DEMO_REPOS` order.
pub const DEMO_WORKFLOW_COUNTS: [usize; 3] = [8, 1, 1];

/// The first repository's workflows, in order.
pub const DEMO_MAIN_WORKFLOWS: [(i64, &str); 8] = [
    (21001, "CI"),
    (21002, "Release"),
    (21003, "AppImage CI"),
    (21004, "Snap CI"),
    (21005, "Lockfile Sync"),
    (21006, "Publish Release"),
    (21007, "Copilot code review"),
    (21008, "Copilot coding agent"),
];

/// Runs per repository, in `DEMO_REPOS` order.
pub const DEMO_REPO_RUN_COUNTS: [usize; 3] = [13, 1, 1];

/// The newest run of the first repository.
pub const DEMO_NEWEST_RUN: (i64, &str) = (30108, "in_progress");

/// The lab repository's single run is the canonical failed run.
pub const DEMO_FAILED_RUN: (i64, &str, &str) = (31001, "completed", "failure");

/// The edge repository's single run is the canonical successful run.
pub const DEMO_SUCCESS_RUN: (i64, &str, &str) = (32101, "completed", "success");

/// Jobs of `DEMO_NEWEST_RUN`.
pub const DEMO_NEWEST_RUN_JOBS: [(i64, &str, &str); 2] = [
    (43081, "prepare", "completed"),
    (43082, "bundle", "in_progress"),
];

/// Every demo repository exposes the same two branches.
pub const DEMO_BRANCHES: [&str; 2] = ["main", "develop"];

/// Job logs are ANSI-group-annotated text; every fixture carries this marker.
pub const DEMO_LOG_MARKER: &str = "##[group]";

/// The demo rate limit is a full, untouched budget.
pub const DEMO_RATE_LIMIT: (i64, i64) = (5000, 5000);

/// `workflow_dispatch` exposes this many inputs in demo mode.
pub const DEMO_DISPATCH_INPUT_COUNT: usize = 2;

/// A fixed instant for time-dependent assertions, so tests never call
/// `Utc::now()` inside an assertion.
pub const FIXED_INSTANT: &str = "2026-01-24T12:00:00Z";

/// A gateway serving the demo fixtures, for tests that need a working data
/// source without a network.
pub fn demo_gateway() -> actioneer::gateway::GitHubGateway {
    actioneer::gateway::GitHubGateway::demo()
}

/// A gateway pointed at a local fake API server.
pub fn live_gateway_at(base_url: &str) -> actioneer::gateway::GitHubGateway {
    // `GitHubGateway::live` always builds the real client, so tests that need a
    // controllable server go through the client's base-URL override and wrap it
    // the same way the gateway would.
    actioneer::gateway::GitHubGateway::live_with_base_url(base_url, Some("test-token".into()))
        .expect("a live gateway against a local server builds")
}
