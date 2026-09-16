use super::MainWindow;
use gtk4::glib;
use tracing::info;

impl MainWindow {
    pub(super) fn is_demo_mode(&self) -> bool {
        *self.demo_mode.lock()
    }

    pub(crate) fn enter_demo_mode(&self) {
        if self.is_demo_mode() {
            info!("Demo mode already active");
            return;
        }

        info!("Entering demo mode with mock data");
        self.stop_background_refresh();

        // The slot transition belongs to the services; everything below it in
        // this function is the UI reacting, which is why it stays here.
        let (repos, rate_info) = self.services.enter_demo();

        {
            let mut flag = self.demo_mode.lock();
            *flag = true;
        }

        {
            let mut selected = self.selected_repo_id.lock();
            *selected = None;
        }

        self.favorites.lock().clear();

        self.show_authenticated_ui();
        self.show_header_loading(false);

        self.refresh_repository_view(repos.clone(), rate_info.clone());

        // Prefer the demo repo with the richest workflow/run data for a representative view.
        let preferred_repo = repos
            .iter()
            .find(|repo| repo.full_name == "demo-org/actioneer-demo-app")
            .or(repos.first())
            .cloned();
        if let Some(repo) = preferred_repo {
            let repo_id = repo.id;
            *self.selected_repo_id.lock() = Some(repo_id);
            let this = self.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                *this.selected_repo_id.lock() = Some(repo_id);
                this.restore_repo_selection_now();
            });
        } else {
            self.show_detail_placeholder();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::app_services::AppServices;
    use crate::ui::test_helpers::run_gtk_test;
    use gtk4::prelude::*;
    use libadwaita as adw;

    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..200 {
            if !context.iteration(false) {
                break;
            }
        }
    }

    /// Builds a window already in demo mode.
    ///
    /// `start_demo_mode: true` is required, not a convenience. The `false` path
    /// runs `check_authentication()`, which reads the real keyring — forbidden
    /// by AGENTS.md — and, worse, races: its reply handler calls
    /// `enter_signed_out_state()`, so on a machine where the lookup is slower
    /// than the test (CI, for one) it lands *after* `enter_demo_mode` and
    /// empties the gateway slot while the demo flag stays set.
    fn demo_window(id: &str, dir: &std::path::Path) -> (MainWindow, AppServices) {
        let app = adw::Application::builder().application_id(id).build();
        let services = AppServices::test_fakes(dir);
        let window = MainWindow::new(&app, services.clone(), true);
        (window, services)
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn starting_in_demo_mode_installs_a_demo_gateway_and_fills_the_sidebar() {
        run_gtk_test("demo_mode_installs_gateway", || {
            crate::runtime::init_test_runtime();
            let dir = tempfile::tempdir().expect("temp dir");
            let (window, services) =
                demo_window("me.spaceinbox.actioneer.DemoModeTest", dir.path());
            pump();

            assert!(window.is_demo_mode(), "the demo flag is set");
            assert!(
                services
                    .gateway
                    .lock()
                    .as_ref()
                    .is_some_and(|gateway| gateway.is_demo()),
                "a demo gateway is installed in the slot"
            );
            assert!(
                !window.repos.lock().is_empty(),
                "the sidebar is seeded from the demo fixtures"
            );

            window.window.destroy();
            pump();
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn entering_demo_mode_again_is_a_no_op() {
        run_gtk_test("demo_mode_twice", || {
            crate::runtime::init_test_runtime();
            let dir = tempfile::tempdir().expect("temp dir");
            let (window, _services) =
                demo_window("me.spaceinbox.actioneer.DemoModeTwiceTest", dir.path());
            pump();
            let first = window.repos.lock().len();

            // The guard matters: a second entry would re-seed the sidebar and
            // restart the selection timer while the first one is still pending.
            window.enter_demo_mode();
            pump();

            assert_eq!(
                window.repos.lock().len(),
                first,
                "the second call did nothing"
            );

            window.window.destroy();
            pump();
        });
    }
}
