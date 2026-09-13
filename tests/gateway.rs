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
