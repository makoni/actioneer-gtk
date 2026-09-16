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

    #[test]
    #[ignore = "requires GTK display"]
    fn entering_demo_mode_installs_a_demo_gateway_and_fills_the_sidebar() {
        run_gtk_test("enter_demo_mode_installs_gateway", || {
            crate::runtime::init_test_runtime();
            let dir = tempfile::tempdir().expect("temp dir");
            let app = adw::Application::builder()
                .application_id("me.spaceinbox.actioneer.DemoModeTest")
                .build();
            let services = AppServices::test_fakes(dir.path());
            services.sign_out();

            let window = MainWindow::new(&app, services.clone(), false);
            pump();
            assert!(
                !window.is_demo_mode(),
                "the window starts outside demo mode"
            );

            window.enter_demo_mode();
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
                "the sidebar is seeded synchronously, not on a later tick"
            );

            window.window.destroy();
            pump();
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn entering_demo_mode_twice_is_a_no_op() {
        run_gtk_test("enter_demo_mode_twice", || {
            crate::runtime::init_test_runtime();
            let dir = tempfile::tempdir().expect("temp dir");
            let app = adw::Application::builder()
                .application_id("me.spaceinbox.actioneer.DemoModeTwiceTest")
                .build();
            let services = AppServices::test_fakes(dir.path());
            services.sign_out();
            let window = MainWindow::new(&app, services, false);
            pump();

            window.enter_demo_mode();
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
