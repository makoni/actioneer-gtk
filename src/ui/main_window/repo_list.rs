use super::MainWindow;
use crate::api::models::Repo;
use crate::ui::sidebar::{find_label_by_name, row_matches_query};
use gtk4::{self as gtk, prelude::*};
use tracing::info;

impl MainWindow {
    pub(super) fn connect_search(&self) {
        let list_box = self.repo_list.clone();

        self.search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_lowercase();
            let query = text.clone();
            list_box.set_filter_func(move |row: &gtk::ListBoxRow| row_matches_query(row, &query));
            list_box.invalidate_filter();
        });
    }

    pub(super) fn connect_repo_selection(&self) {
        let window = self.clone();

        self.repo_list
            .connect_selected_rows_changed(move |list_box| {
                let window = window.clone();
                let repo_name = selected_repo_name(list_box);

                let repo = {
                    let repos = window.repos.lock();
                    resolve_repo_by_name(&repos, repo_name)
                };

                let current_selection = *window.selected_repo_id.lock();
                let new_selection = repo.as_ref().map(|r| r.id);

                if *window.handling_selection.lock() {
                    info!("Already handling selection, ignoring signal");
                    return;
                }

                info!(
                    "Selection signal: current={:?}, new={:?}, repo={:?}",
                    current_selection,
                    new_selection,
                    repo.as_ref().map(|r| r.full_name.as_str())
                );

                if new_selection.is_none() && window.active_detail.borrow().is_some() {
                    info!("Ignoring transient deselection (detail pane is active)");
                    return;
                }

                if current_selection != new_selection {
                    window.handle_repo_selection(repo);
                }
            });
    }
}

fn selected_repo_name(list_box: &gtk::ListBox) -> Option<String> {
    list_box
        .selected_row()
        .and_then(|row| row.child())
        .and_then(|child| find_label_by_name(&child, "repo-name-label"))
        .map(|label| label.text().to_string())
}

fn resolve_repo_by_name(repos: &[Repo], name: Option<String>) -> Option<Repo> {
    let target = name?;
    repos.iter().find(|repo| repo.full_name == target).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{Repo, User};

    fn make_repo(id: i64, full_name: &str) -> Repo {
        let name = full_name.split('/').last().unwrap_or(full_name).to_string();
        Repo {
            id,
            name,
            full_name: full_name.to_string(),
            owner: User {
                login: "tester".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        }
    }

    #[test]
    fn resolves_repo_by_matching_full_name() {
        let repos = vec![make_repo(1, "makoni/actioneer"), make_repo(2, "foo/bar")];
        let repo = resolve_repo_by_name(&repos, Some("foo/bar".into()));
        assert_eq!(repo.map(|r| r.id), Some(2));
    }

    #[test]
    fn returns_none_for_unknown_repo() {
        let repos = vec![make_repo(1, "makoni/actioneer")];
        let repo = resolve_repo_by_name(&repos, Some("nope".into()));
        assert!(repo.is_none());
    }

    #[test]
    fn returns_none_when_missing_name() {
        let repos = vec![make_repo(1, "makoni/actioneer")];
        let repo = resolve_repo_by_name(&repos, None);
        assert!(repo.is_none());
    }
}
