//! Startup from the cache and the offline readout (#103).

use super::*;
use crate::services::api::models::User;
use crate::services::app_services::AppServices;
use crate::services::cache::DataCache;
use crate::ui::test_helpers::run_gtk_test;

const UNREACHABLE: &str = "http://127.0.0.1:1";

fn app() -> adw::Application {
    adw::Application::builder()
        .application_id("me.spaceinbox.actioneer.MainWindowOfflineTest")
        .build()
}

fn repo(id: i64, name: &str) -> Repo {
    Repo {
        id,
        name: name.into(),
        full_name: format!("octo/{name}"),
        owner: User {
            login: "octo".into(),
        },
        is_private: true,
        permissions: None,
        default_branch: Some("main".into()),
    }
}

/// A live gateway that cannot reach GitHub, over a cache holding two repos.
fn cached_unreachable_gateway() -> GitHubGateway {
    let cache = Arc::new(DataCache::new());
    let gateway = GitHubGateway::live_cached_with_base_url(UNREACHABLE, "token".into(), &cache)
        .expect("gateway builds");
    let owner = cache.bind_owner("token");
    cache.store_repos(&owner, &[repo(1, "one"), repo(2, "two")]);
    gateway
}

fn settle_until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..400 {
        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }
        if done() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    done()
}

/// A window built in demo mode — the only keyring-free way to build one — then
/// switched to a live, cached, unreachable gateway as a sign-in would.
fn signed_in_window(services: &AppServices) -> MainWindow {
    let window = MainWindow::new(&app(), services.clone(), true);
    settle_until(|| false);
    *services.gateway.lock() = Some(cached_unreachable_gateway());
    *window.demo_mode.lock() = false;
    window.repos.lock().clear();
    window.actions_states.lock().clear();
    window.workflow_counts.lock().clear();
    *window.selected_repo_id.lock() = None;
    window.offline.begin_session();
    window
}

#[test]
#[ignore = "requires GTK display"]
fn the_sidebar_is_painted_from_the_cache_on_sign_in() {
    run_gtk_test("sidebar_painted_from_cache", || {
        crate::runtime::init_test_runtime();
        let dir = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(dir.path());
        let window = signed_in_window(&services);
        // After the demo start, which saves its own selection.
        let preferences = services.preferences.clone().expect("preferences");
        crate::runtime::handle()
            .block_on(preferences.set_last_selected_repo(Some(2)))
            .expect("store the last selection");

        window.paint_cached_repositories();
        assert!(
            settle_until(|| !window.repos.lock().is_empty()),
            "the cached repositories arrive"
        );

        let ids: Vec<i64> = window.repos.lock().iter().map(|repo| repo.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(
            settle_until(|| window.repo_filter_model.n_items() >= 2),
            "the sidebar rows are rebuilt from the cached list"
        );
        assert_eq!(
            *window.selected_repo_id.lock(),
            Some(2),
            "the last selection is restored, not the first repository"
        );
        assert!(
            window.actions_checked_at.lock().is_empty(),
            "a cached paint must not stamp the status check"
        );

        window.window.destroy();
        settle_until(|| false);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn a_cached_paint_never_lands_over_a_live_list() {
    run_gtk_test("cached_paint_after_live_list", || {
        crate::runtime::init_test_runtime();
        let dir = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(dir.path());
        let window = signed_in_window(&services);

        let attempts = window.offline.cached_paint_attempts.clone();
        let wait_for_attempt =
            |n: u64| settle_until(|| attempts.load(std::sync::atomic::Ordering::Acquire) >= n);

        // The network answered first (with nothing, for the test's sake).
        window.offline.note_live_repos();
        window.paint_cached_repositories();
        assert!(wait_for_attempt(1), "the cached paint arrives");
        assert!(window.repos.lock().is_empty(), "and is turned away");

        // Nor after the session it belonged to has ended.
        window.offline.begin_session();
        window.paint_cached_repositories();
        window.offline.begin_session();
        assert!(wait_for_attempt(2), "the cached paint arrives");
        assert!(window.repos.lock().is_empty(), "and is turned away");

        window.window.destroy();
        settle_until(|| false);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn the_rate_limit_readout_says_offline_while_github_is_unreachable() {
    run_gtk_test("rate_limit_offline", || {
        crate::runtime::init_test_runtime();
        let _guard = crate::kernel::i18n::i18n_test_guard();
        let label = gtk::Label::new(None);
        let gateway = cached_unreachable_gateway();
        let slot = Arc::new(Mutex::new(Some(gateway.clone())));

        render_rate_limit_for_test(&label, &slot);
        assert_ne!(label.text(), tr("Offline"));

        // A long first probe pause, so the state stays put for the test.
        gateway.simulate_offline_for_test(Duration::from_secs(3600));
        render_rate_limit_for_test(&label, &slot);
        assert_eq!(label.text(), tr("Offline"));
    });
}

fn render_rate_limit_for_test(label: &gtk::Label, slot: &Arc<Mutex<Option<GitHubGateway>>>) {
    offline::render_rate_limit(
        label,
        slot,
        Some(RateLimitInfo {
            limit: 5000,
            remaining: 4000,
            reset: 0,
        }),
    );
}

#[test]
#[ignore = "requires GTK display"]
fn the_connectivity_forwarder_stops_with_the_window() {
    run_gtk_test("connectivity_forwarder_stops", || {
        crate::runtime::init_test_runtime();
        let dir = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(dir.path());
        let window = signed_in_window(&services);

        window.watch_connectivity();
        assert!(window.offline.is_watching(), "a live gateway is followed");

        // The forwarder owns a clone of the window. The language-change
        // rebuild stops it explicitly; a window destroyed any other way has to
        // be let go of at the next connectivity change at the latest.
        window.window.destroy();
        services
            .gateway
            .lock()
            .as_ref()
            .expect("gateway")
            .simulate_offline_for_test(Duration::from_secs(3600));
        assert!(
            settle_until(|| !window.offline.is_watching()),
            "the forwarder stops once its window is gone"
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn coming_back_online_reloads_the_repositories() {
    run_gtk_test("online_again_reloads", || {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        crate::runtime::init_test_runtime();
        let server = crate::runtime::handle().block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rate_limit"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/user/repos"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                        "id": 42,
                        "name": "back",
                        "full_name": "octo/back",
                        "owner": { "login": "octo" },
                        "private": false,
                        "permissions": { "admin": true, "push": true, "pull": true },
                        "default_branch": "main"
                    }])),
                )
                .mount(&server)
                .await;
            server
        });

        let dir = tempfile::tempdir().expect("temp dir");
        let services = AppServices::test_fakes(dir.path());
        let window = signed_in_window(&services);
        let cache = Arc::new(DataCache::new());
        let gateway =
            GitHubGateway::live_cached_with_base_url(&server.uri(), "token".into(), &cache)
                .expect("gateway builds");
        *services.gateway.lock() = Some(gateway.clone());
        window.watch_connectivity();

        // Offline, then the probe finds the server: the window reloads.
        gateway.simulate_offline_for_test(Duration::from_millis(20));
        assert!(
            settle_until(|| window.repos.lock().iter().any(|repo| repo.id == 42)),
            "the repository list is fetched again once GitHub answers"
        );
        assert!(!gateway.is_offline());

        window.offline.stop_watching();
        window.window.destroy();
        settle_until(|| false);
        drop(server);
    });
}
