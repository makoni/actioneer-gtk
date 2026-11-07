//! The gateway's dispatch: which backend answers, and that the two differ.
//!
//! `tests/characterization_demo.rs` already covers what the demo backend
//! *returns*. This file covers the sum type itself — that `live()` and `demo()`
//! route to different places, which is the whole point of Phase 2 and the one
//! thing neither the demo characterization nor the client characterization can
//! see on its own.

mod common;

use common::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_demo_gateway_answers_without_a_network() {
    let gateway = demo_gateway();
    assert!(gateway.is_demo());

    // No server is running anywhere in this test; if the demo variant ever
    // reached for HTTP this would fail rather than pass.
    let repos = gateway.list_repos().await.expect("demo repositories");
    let names: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    assert_eq!(names, DEMO_REPOS);
}

#[tokio::test]
async fn a_live_gateway_goes_to_the_server_it_was_given() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 99,
                "name": "only-on-the-server",
                "full_name": "octo/only-on-the-server",
                "owner": { "login": "octo" },
                "private": false,
                "permissions": { "admin": true, "push": true, "pull": true },
                "default_branch": "main"
            }])),
        )
        .mount(&server)
        .await;

    let gateway = live_gateway_at(&server.uri());
    assert!(!gateway.is_demo());

    let repos = gateway.list_repos().await.expect("live repositories");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].full_name, "octo/only-on-the-server");

    // The decisive assertion: the live gateway returned the server's data, not
    // the demo fixtures. Before Phase 2 the client branched on a global and
    // could have served either.
    assert!(
        !DEMO_REPOS.contains(&repos[0].full_name.as_str()),
        "a live gateway must not serve demo fixtures"
    );
}

#[tokio::test]
async fn only_a_demo_gateway_can_seed_synchronously() {
    assert!(
        demo_gateway().demo_seed().is_some(),
        "the demo gateway seeds the sidebar without I/O"
    );

    let server = MockServer::start().await;
    assert!(
        live_gateway_at(&server.uri()).demo_seed().is_none(),
        "a live gateway has nothing to seed from"
    );
}

#[tokio::test]
async fn two_demo_gateways_do_not_share_fixtures() {
    // Each gateway owns its backend, so one test's mutations cannot reach
    // another's — the property that replaced the old process-global switch.
    let a = demo_gateway();
    let b = demo_gateway();
    let (owner, name) = DEMO_REPO_MAIN;

    let before = b
        .list_repository_runs(owner, name)
        .await
        .expect("runs")
        .len();
    a.dispatch_workflow(owner, name, "21001", "main", None)
        .await
        .expect("dispatch succeeds");
    let after = b
        .list_repository_runs(owner, name)
        .await
        .expect("runs")
        .len();

    assert_eq!(before, after);
}

// ---- the data cache --------------------------------------------------------
//
// A live gateway built with a cache writes what it fetches through to it; the
// demo gateway has no cache to write to or read from.

fn repo_page() -> serde_json::Value {
    serde_json::json!([{
        "id": 7,
        "name": "hello",
        "full_name": "octo/hello",
        "owner": { "login": "octo" },
        "private": true,
        "permissions": { "admin": true, "push": true, "pull": true },
        "default_branch": "main"
    }])
}

fn run_json(id: i64, workflow_id: i64, status: &str, updated_at: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "run_number": id,
        "workflow_id": workflow_id,
        "name": "CI",
        "display_title": "CI",
        "head_branch": "main",
        "head_commit": null,
        "status": status,
        "conclusion": if status == "completed" { serde_json::json!("success") } else { serde_json::Value::Null },
        "run_started_at": null,
        "event": "push",
        "created_at": null,
        "updated_at": updated_at,
        "html_url": null
    })
}

fn jobs_json(run_id: i64, status: &str) -> serde_json::Value {
    serde_json::json!({
        "total_count": 1,
        "jobs": [{
            "id": run_id * 10,
            "run_id": run_id,
            "status": status,
            "conclusion": null,
            "started_at": null,
            "completed_at": null,
            "name": "build",
            "steps": [],
            "html_url": null
        }]
    })
}

fn cached_gateway_at(
    base_url: &str,
    cache: &std::sync::Arc<actioneer::services::cache::DataCache>,
) -> actioneer::services::gateway::GitHubGateway {
    actioneer::services::gateway::GitHubGateway::live_cached_with_base_url(
        base_url,
        "test-token".into(),
        cache,
    )
    .expect("a cached live gateway builds")
}

#[tokio::test]
async fn a_cached_gateway_writes_what_it_fetches_through() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repo_page()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "workflows": [{ "id": 70, "name": "CI", "path": ".github/workflows/ci.yml" }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [
                run_json(2, 70, "in_progress", "2026-11-01T10:00:00Z"),
                run_json(1, 70, "completed", "2026-11-01T09:00:00Z")
            ]
        })))
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    assert!(gateway.cached_repos().is_none(), "nothing fetched yet");

    gateway.list_repos().await.expect("repos");
    gateway
        .list_workflows("octo", "hello")
        .await
        .expect("workflows");
    gateway
        .list_repository_runs("octo", "hello")
        .await
        .expect("runs");

    let repos = gateway.cached_repos().expect("cached repos");
    assert_eq!(repos.value[0].full_name, "octo/hello");
    assert_eq!(
        gateway.cached_workflows(7).expect("workflows").value[0].id,
        70
    );
    let latest = gateway
        .cached_latest_completed_runs(7)
        .expect("latest runs");
    assert_eq!(
        latest.value.iter().map(|run| run.id).collect::<Vec<_>>(),
        vec![1],
        "one run per workflow, the newest completed one"
    );
}

#[tokio::test]
async fn a_failed_request_stores_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    assert!(gateway.list_repos().await.is_err());
    assert!(gateway.cached_repos().is_none());
}

#[tokio::test]
async fn the_demo_gateway_has_no_cache() {
    let gateway = demo_gateway();
    gateway.list_repos().await.expect("demo repositories");
    assert!(gateway.cached_repos().is_none());
}

#[tokio::test]
async fn jobs_of_a_settled_run_are_fetched_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows/70/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [run_json(5, 70, "completed", "2026-11-01T09:00:00Z")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/runs/5/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jobs_json(5, "completed")))
        .expect(1)
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    gateway.list_runs("octo", "hello", 70).await.expect("runs");

    for _ in 0..3 {
        let jobs = gateway
            .list_jobs_for_run("octo", "hello", 5)
            .await
            .expect("jobs");
        assert_eq!(jobs.len(), 1);
    }
    // `expect(1)` is verified when the server drops.
}

#[tokio::test]
async fn jobs_are_refetched_once_the_run_is_rerun() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/runs/5/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jobs_json(5, "completed")))
        .expect(2)
        .mount(&server)
        .await;

    let first_list = Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows/70/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [run_json(5, 70, "completed", "2026-11-01T09:00:00Z")]
        })))
        .mount_as_scoped(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    gateway.list_runs("octo", "hello", 70).await.expect("runs");
    gateway
        .list_jobs_for_run("octo", "hello", 5)
        .await
        .expect("jobs");
    drop(first_list);

    // Re-run and finished again: same id, new `updated_at`.
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows/70/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [run_json(5, 70, "completed", "2026-11-01T11:00:00Z")]
        })))
        .mount(&server)
        .await;
    gateway.list_runs("octo", "hello", 70).await.expect("runs");
    gateway
        .list_jobs_for_run("octo", "hello", 5)
        .await
        .expect("jobs");
}

#[tokio::test]
async fn plain_list_jobs_always_goes_to_the_network() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows/70/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [run_json(5, 70, "completed", "2026-11-01T09:00:00Z")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/runs/5/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jobs_json(5, "completed")))
        .expect(3)
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    gateway.list_runs("octo", "hello", 70).await.expect("runs");
    gateway
        .list_jobs_for_run("octo", "hello", 5)
        .await
        .expect("jobs");
    // The log window follows a run live; it must never be answered from cache.
    gateway.list_jobs("octo", "hello", 5).await.expect("jobs");
    gateway.list_jobs("octo", "hello", 5).await.expect("jobs");
}

#[tokio::test]
async fn a_rerun_through_the_gateway_drops_the_cached_jobs_at_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/workflows/70/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "workflow_runs": [run_json(5, 70, "completed", "2026-11-01T09:00:00Z")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/octo/hello/actions/runs/5/rerun"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello/actions/runs/5/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jobs_json(5, "completed")))
        .expect(2)
        .mount(&server)
        .await;

    let cache = std::sync::Arc::new(actioneer::services::cache::DataCache::new());
    let gateway = cached_gateway_at(&server.uri(), &cache);
    gateway.list_runs("octo", "hello", 70).await.expect("runs");
    gateway
        .list_jobs_for_run("octo", "hello", 5)
        .await
        .expect("jobs");

    // No run list has shown the re-run yet; the jobs must still be refetched.
    gateway
        .rerun_workflow("octo", "hello", 5)
        .await
        .expect("rerun");
    gateway
        .list_jobs_for_run("octo", "hello", 5)
        .await
        .expect("jobs");
}

// ---- connectivity ----------------------------------------------------------

/// A port nothing listens on: connecting fails at once, which is what an
/// unreachable GitHub looks like to the client.
const UNREACHABLE: &str = "http://127.0.0.1:1";

#[tokio::test]
async fn a_transport_failure_marks_the_gateway_offline() {
    let gateway = live_gateway_at(UNREACHABLE);
    let mut connectivity = gateway.connectivity().expect("a live gateway reports");
    assert!(!gateway.is_offline());

    let error = gateway.list_repos().await.expect_err("nothing listens");
    assert!(
        error.is_offline(),
        "a refused connection is offline: {error}"
    );
    assert!(gateway.is_offline());
    assert!(
        connectivity.has_changed().expect("sender alive"),
        "subscribers are told"
    );
    assert!(*connectivity.borrow_and_update());
}

#[tokio::test]
async fn an_http_error_is_not_offline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let gateway = live_gateway_at(&server.uri());
    // A long first pause, so it is the request below that clears the state.
    gateway.simulate_offline_for_test(std::time::Duration::from_secs(3600));
    let error = gateway.list_repos().await.expect_err("server error");
    assert!(!error.is_offline());
    assert!(!gateway.is_offline(), "GitHub answered, so it is reachable");
}

#[tokio::test]
async fn the_probe_notices_github_coming_back() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rate_limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let gateway = live_gateway_at(&server.uri());
    let mut connectivity = gateway.connectivity().expect("a live gateway reports");
    gateway.simulate_offline_for_test(std::time::Duration::from_millis(20));
    assert!(gateway.is_offline());

    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        connectivity.wait_for(|offline| !*offline),
    )
    .await
    .expect("the probe reaches the server")
    .expect("sender alive");
    assert!(!gateway.is_offline());
}

#[tokio::test]
async fn any_successful_request_clears_offline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let gateway = live_gateway_at(&server.uri());
    // A long first pause, so it is the request below that clears the state.
    gateway.simulate_offline_for_test(std::time::Duration::from_secs(3600));
    gateway.list_repos().await.expect("repos");
    assert!(!gateway.is_offline());
}

#[tokio::test]
async fn the_demo_gateway_is_never_offline() {
    let gateway = demo_gateway();
    assert!(!gateway.is_offline());
    assert!(gateway.connectivity().is_none());
}

#[tokio::test]
async fn a_connection_dropped_before_the_answer_is_offline() {
    // Accepts and hangs up at once: what a pooled connection does when the
    // network goes away under it. Not a connect error, and no I/O error in the
    // chain either — hyper reports the closed connection itself.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });

    let gateway = live_gateway_at(&format!("http://{address}"));
    let error = gateway.list_repos().await.expect_err("the server hangs up");
    assert!(error.is_offline(), "{error}");
    assert!(gateway.is_offline());
}
