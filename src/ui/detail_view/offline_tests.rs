//! The pane's paint from the cache (#103): what it shows, and what it must not
//! set off.

use super::*;
use crate::services::api::GitHubError;
use crate::services::api::models::{User, WorkflowRun};
use crate::services::cache::DataCache;
use crate::ui::detail_view::workflow_refresh::should_toast_load_failure;
use crate::ui::test_helpers::run_gtk_test;

/// Nothing listens here, so the live load fails at once, as offline.
const UNREACHABLE: &str = "http://127.0.0.1:1";

fn repo() -> Repo {
    Repo {
        id: 7,
        name: "hello".into(),
        full_name: "octo/hello".into(),
        owner: User {
            login: "octo".into(),
        },
        is_private: true,
        permissions: None,
        default_branch: Some("main".into()),
    }
}

fn workflow(id: i64) -> Workflow {
    Workflow {
        id,
        name: format!("wf-{id}"),
        path: format!(".github/workflows/{id}.yml"),
    }
}

fn run(id: i64, workflow_id: i64, status: &str) -> WorkflowRun {
    WorkflowRun {
        id,
        run_number: Some(id),
        workflow_id: Some(workflow_id),
        name: Some(format!("wf-{workflow_id}")),
        display_title: Some("Build".into()),
        head_branch: Some("main".into()),
        head_commit: None,
        status: Some(status.into()),
        conclusion: (status == "completed").then(|| "success".into()),
        run_started_at: Some("2026-11-01T09:00:00Z".into()),
        event: Some("push".into()),
        created_at: Some("2026-11-01T09:00:00Z".into()),
        updated_at: Some("2026-11-01T09:05:00Z".into()),
        html_url: None,
        actor: None,
        triggering_actor: None,
    }
}

/// A live gateway that cannot reach GitHub, over a cache holding repository 7
/// with two workflows: 70 last finished green, 71 only ever seen running.
fn cached_unreachable_gateway(age_secs: u64) -> GitHubGateway {
    let cache = Arc::new(DataCache::new());
    let gateway = GitHubGateway::live_cached_with_base_url(UNREACHABLE, "token".into(), &cache)
        .expect("gateway builds");
    let owner = cache.bind_owner("token");
    cache.store_repos(&owner, &[repo()]);
    cache.store_workflows(&owner, "octo", "hello", &[workflow(70), workflow(71)]);
    cache.store_repository_runs(
        &owner,
        "octo",
        "hello",
        &[run(2, 71, "in_progress"), run(1, 70, "completed")],
    );
    cache.backdate_for_test(age_secs);
    gateway
}

fn settle() {
    for _ in 0..100 {
        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn pane_over(gateway: GitHubGateway) -> RepoDetailPane {
    let app = adw::Application::builder()
        .application_id("me.spaceinbox.actioneer.PaneOfflineTest")
        .build();
    RepoDetailPane::new(
        adw::ApplicationWindow::new(&app),
        repo(),
        Arc::new(parking_lot::Mutex::new(gateway)),
        RepoDetailDeps {
            favorites_manager: None,
            preferences_manager: None,
            favorites: Arc::new(parking_lot::Mutex::new(HashSet::new())),
            notification_manager: None,
        },
        false,
    )
}

#[test]
#[ignore = "requires GTK display"]
fn the_pane_paints_cached_workflows_before_the_network_answers() {
    run_gtk_test("pane_paints_cached_workflows", || {
        crate::runtime::init_test_runtime();
        let _guard = crate::kernel::i18n::i18n_test_guard();
        let pane = pane_over(cached_unreachable_gateway(3 * 3600));

        // Synchronously, before the main loop has run once.
        assert_eq!(pane.workflow_store.n_items(), 2, "both cached workflows");
        let subtitle = pane.header.subtitle_label().text().to_string();
        let three_hours = tr("{count}h ago").replace("{count}", "3");
        assert!(
            subtitle.contains(&three_hours),
            "the cached age is shown, not \"Just now\": {subtitle:?}"
        );
        assert!(pane.header.latest_run(70).is_some(), "last finished run");
        assert!(
            pane.header.latest_run(71).is_none(),
            "a run cached while running is not shown as running"
        );

        // No side effects: nothing polls, nothing seeds notifications.
        assert!(pane.workflows_with_active_runs.lock().is_empty());
        assert!(pane.run_digests.lock().is_empty());
        assert!(pane.workflows_last_loaded.lock().is_empty());

        // The live load fails; the cached rows stay, now marked offline.
        settle();
        assert!(pane.client.lock().is_offline());
        // The failure reached the main loop only after the gateway saw it.
        settle();
        assert_eq!(pane.workflow_store.n_items(), 2);
        assert!(
            pane.header
                .subtitle_label()
                .text()
                .contains(tr("Offline").as_str()),
            "the subline says the data could not be refreshed"
        );
        assert!(
            pane.header.subtitle_label().text().contains(&three_hours),
            "a failed refresh does not count as one"
        );
        assert!(pane.client.lock().is_offline());

        pane.deactivate();
        settle();
    });
}

#[test]
#[ignore = "requires GTK display"]
fn without_a_cache_the_pane_starts_empty() {
    run_gtk_test("pane_without_cache_starts_empty", || {
        crate::runtime::init_test_runtime();
        let gateway = GitHubGateway::live_with_base_url(UNREACHABLE, Some("token".into()))
            .expect("gateway builds");
        let pane = pane_over(gateway);
        assert_eq!(pane.workflow_store.n_items(), 0);
        pane.deactivate();
        settle();
    });
}

#[test]
fn an_offline_failure_over_cached_rows_is_not_toasted() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let offline: GitHubError = runtime
        .block_on(reqwest::get(UNREACHABLE))
        .expect_err("nothing listens")
        .into();
    assert!(offline.is_offline());

    assert!(!should_toast_load_failure(&offline, true));
    assert!(
        should_toast_load_failure(&offline, false),
        "nothing on screen"
    );
    let api = GitHubError::ApiError("boom".into());
    assert!(
        should_toast_load_failure(&api, true),
        "GitHub answered with an error"
    );
}
