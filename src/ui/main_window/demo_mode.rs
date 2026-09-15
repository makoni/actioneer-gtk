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
