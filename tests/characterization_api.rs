//! Characterization of the live `GitHubClient` against a fake API server.
//!
//! Step 2.3 of the architecture refactor deletes the 14 demo-intercept branches
//! from `api/client.rs`, one in every public method. This file pins what those
//! methods do on the HTTP path *before* the branches are removed, so the strip
//! is provably behaviour-preserving.
//!
//! It works identically on both sides of that step: demo state is a
//! process-global that only `demo::enable()` populates, and no integration test
//! calls it, so the client already takes the HTTP path here.
//!
//! `ResponseHandler`-level behaviour (ETag revalidation, 401/403 mapping) is
//! already covered by the in-crate tests in `src/api/http.rs`; this file covers
//! the public client surface those unit tests do not reach.

use actioneer::services::api::{GitHubClient, GitHubError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn repo_json(id: i64, owner: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "full_name": format!("{owner}/{name}"),
        "owner": { "login": owner },
        "private": false,
        "permissions": { "admin": true, "push": true, "pull": true },
        "default_branch": "main"
    })
}

// The premise test that used to live here — asserting demo mode was inactive so
// the client really took the HTTP path — is gone with step 2.3. The client has
// no demo branches left to take: the choice is made once, at the gateway. There
// is nothing left to assert that the type system does not already guarantee.

#[tokio::test]
async fn list_repos_parses_the_repository_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            repo_json(1, "octo", "hello-world"),
            repo_json(2, "octo", "spoon-knife"),
        ])))
        .mount(&server)
        .await;

    let client = GitHubClient::with_base_url(&server.uri(), Some("t".into())).expect("client");
    let repos = client.list_repos().await.expect("list_repos");

    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0].full_name, "octo/hello-world");
    assert_eq!(repos[0].owner.login, "octo");
    assert_eq!(repos[1].name, "spoon-knife");
}

#[tokio::test]
async fn list_repos_maps_404_to_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = GitHubClient::with_base_url(&server.uri(), Some("t".into())).expect("client");
    let err = client.list_repos().await.expect_err("404 must be an error");

    assert!(
        matches!(err, GitHubError::NotFound),
        "expected NotFound, got {err:?}"
    );
}

#[tokio::test]
async fn rate_limit_headers_reach_rate_limit_info() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-remaining", "4987")
                .insert_header("x-ratelimit-reset", "1800000000")
                .set_body_json(serde_json::json!([])),
        )
        .mount(&server)
        .await;

    let client = GitHubClient::with_base_url(&server.uri(), Some("t".into())).expect("client");
    assert!(
        client.rate_limit_info().is_none(),
        "no rate limit is known before the first request"
    );

    client.list_repos().await.expect("list_repos");

    let info = client
        .rate_limit_info()
        .expect("the response headers populate the rate limit");
    assert_eq!(info.limit, 5000);
    assert_eq!(info.remaining, 4987);
}

#[tokio::test]
async fn list_workflows_parses_the_workflow_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello-world/actions/workflows"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 2,
            "workflows": [
                { "id": 11, "name": "CI", "path": ".github/workflows/ci.yml" },
                { "id": 12, "name": "Release", "path": ".github/workflows/release.yml" }
            ]
        })))
        .mount(&server)
        .await;

    let client = GitHubClient::with_base_url(&server.uri(), Some("t".into())).expect("client");
    let workflows = client
        .list_workflows("octo", "hello-world")
        .await
        .expect("list_workflows");

    assert_eq!(workflows.len(), 2);
    assert_eq!((workflows[0].id, workflows[0].name.as_str()), (11, "CI"));
    assert_eq!(workflows[1].path, ".github/workflows/release.yml");
}

#[tokio::test]
async fn job_logs_are_returned_as_text() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/octo/hello-world/actions/jobs/99/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_string("line one\nline two\n"))
        .mount(&server)
        .await;

    let client = GitHubClient::with_base_url(&server.uri(), Some("t".into())).expect("client");
    let logs = client
        .get_job_logs("octo", "hello-world", 99)
        .await
        .expect("get_job_logs");

    assert!(logs.contains("line one"), "got: {logs:?}");
}
