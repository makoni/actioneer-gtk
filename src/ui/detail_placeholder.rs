use crate::api::models::Repo;
use crate::i18n::tr;
use gtk4::glib::idle_add_local_once;
use libadwaita as adw;

pub fn schedule_status_page_update(status_page: adw::StatusPage, repo: Option<Repo>) {
    idle_add_local_once(move || match repo {
        Some(repo) => {
            let icon_name = if repo.is_private {
                "emblem-readonly-symbolic"
            } else {
                "folder-remote-symbolic"
            };
            let description = format!(
                "{}\n{}",
                repo.full_name,
                tr("Workflows and runs will load in the detail pane.")
            );

            status_page.set_title(&repo.name);
            status_page.set_description(Some(&description));
            status_page.set_icon_name(Some(icon_name));
        }
        None => {
            status_page.set_title(tr("Select a repository").as_str());
            status_page.set_description(Some(
                tr("Choose a repository from the sidebar to browse its workflows and runs here.")
                    .as_str(),
            ));
            status_page.set_icon_name(Some("system-search-symbolic"));
        }
    });
}

pub fn schedule_actions_disabled_page(status_page: adw::StatusPage, repo: Repo) {
    idle_add_local_once(move || {
        status_page.set_title(&repo.name);
        status_page.set_description(Some(
            tr("GitHub Actions is disabled for this repository. Enable Actions to view workflows and runs.").as_str(),
        ));
        status_page.set_icon_name(Some("emblem-unreadable-symbolic"));
    });
}
