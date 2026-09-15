use crate::kernel::i18n::tr;
use crate::services::api::models::Repo;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::api::models::User;
    use crate::ui::test_helpers::run_gtk_test;
    use gtk4::glib;

    fn repo(name: &str, is_private: bool) -> Repo {
        Repo {
            id: 1,
            name: name.to_string(),
            full_name: format!("owner/{name}"),
            owner: User {
                login: "owner".to_string(),
            },
            is_private,
            permissions: None,
            default_branch: Some("main".to_string()),
        }
    }

    /// Both functions defer their work with `idle_add_local_once`, so the main
    /// context has to run before anything is observable.
    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..50 {
            if !context.iteration(false) {
                break;
            }
        }
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn without_a_repository_the_placeholder_invites_a_selection() {
        run_gtk_test("placeholder_no_repo", || {
            let page = adw::StatusPage::new();
            schedule_status_page_update(page.clone(), None);
            pump();

            assert_eq!(page.icon_name().as_deref(), Some("system-search-symbolic"));
            assert!(!page.title().is_empty());
            assert!(page.description().is_some());
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_public_repository_gets_the_remote_folder_icon() {
        run_gtk_test("placeholder_public_repo", || {
            let page = adw::StatusPage::new();
            schedule_status_page_update(page.clone(), Some(repo("hello", false)));
            pump();

            assert_eq!(page.title(), "hello");
            assert_eq!(page.icon_name().as_deref(), Some("folder-remote-symbolic"));
            assert!(
                page.description()
                    .unwrap_or_default()
                    .contains("owner/hello"),
                "the description names the repository"
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_private_repository_gets_the_read_only_emblem() {
        run_gtk_test("placeholder_private_repo", || {
            let page = adw::StatusPage::new();
            schedule_status_page_update(page.clone(), Some(repo("secret", true)));
            pump();

            assert_eq!(
                page.icon_name().as_deref(),
                Some("emblem-readonly-symbolic")
            );
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_repository_with_actions_disabled_says_so() {
        run_gtk_test("placeholder_actions_disabled", || {
            let page = adw::StatusPage::new();
            schedule_actions_disabled_page(page.clone(), repo("archived", false));
            pump();

            assert_eq!(page.title(), "archived");
            assert_eq!(
                page.icon_name().as_deref(),
                Some("emblem-unreadable-symbolic")
            );
            assert!(
                page.description().unwrap_or_default().contains("Actions"),
                "the description explains why the pane is empty"
            );
        });
    }
}
