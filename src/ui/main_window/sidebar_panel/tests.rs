//! Tests for [`super`].
//!
//! Split out of `sidebar_panel.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::{Repo, User};
use crate::ui::sidebar::{RepoListRenderContext, rebuild_repo_list};
use crate::ui::test_helpers::{pump_frames, run_gtk_test};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[test]
#[ignore = "requires GTK display"]
fn selection_does_not_autoselect() {
    run_gtk_test("selection_does_not_autoselect", || {
        let panel = SidebarPanel::new();

        // With autoselect on, `set_selected(INVALID_LIST_POSITION)` is refused and
        // GTK picks a replacement row inside `filter.changed()` — which lands the
        // selection on a section header and swaps the open repo when a pill is
        // toggled.
        assert!(!panel.selection().is_autoselect());
        panel.selection().set_selected(gtk::INVALID_LIST_POSITION);
        assert_eq!(panel.selection().selected(), gtk::INVALID_LIST_POSITION);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn sidebar_panel_initializes_widgets() {
    run_gtk_test("sidebar_panel_initializes_widgets", || {
        let panel = SidebarPanel::new();
        let expected_placeholder = tr("Search repositories...");
        assert_eq!(
            panel
                .search_entry()
                .placeholder_text()
                .as_ref()
                .map(|s| s.as_str()),
            Some(expected_placeholder.as_str())
        );
        assert_eq!(
            panel.repo_list().accessible_role(),
            gtk::AccessibleRole::List
        );
        assert_eq!(panel.filter_mode.get(), SidebarFilter::All);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn rebuild_keeps_the_top_visible_repo_in_place() {
    run_gtk_test("rebuild_keeps_the_top_visible_repo_in_place", || {
        let panel = SidebarPanel::new();
        let window = gtk::Window::new();
        window.set_default_size(360, 320);
        window.set_child(Some(&panel.clamp()));
        window.present();
        pump_frames();

        let repos = (1..=40)
            .map(|id: i64| Repo {
                id,
                name: format!("repo-{id}"),
                full_name: format!("makoni/repo-{id}"),
                owner: User {
                    login: "makoni".into(),
                },
                is_private: false,
                permissions: None,
                default_branch: Some("main".into()),
            })
            .collect::<Vec<_>>();

        let favorites_state = Arc::new(Mutex::new(HashSet::new()));
        let context = || RepoListRenderContext {
            repos: repos.clone(),
            actions_snapshot: HashMap::new(),
            workflow_snapshot: HashMap::new(),
            favorites_state: favorites_state.clone(),
            favorites_manager: None,
        };

        let repo_view = panel.repo_list();
        let store = panel.repo_store();

        // The order of repository rows in the store, ignoring headers.
        let repo_order = |store: &gio::ListStore| {
            (0..store.n_items())
                .filter_map(|i| {
                    store
                        .item(i)
                        .and_then(|obj| crate::ui::sidebar::repo_id_from_object(&obj))
                })
                .collect::<Vec<i64>>()
        };

        rebuild_repo_list(store.clone(), context());
        pump_frames();
        let order_before = repo_order(&store);

        let adjustment = repo_view
            .vadjustment()
            .expect("repo list is inside a ScrolledWindow");
        assert!(
            adjustment.upper() > adjustment.page_size(),
            "list must overflow to be scrollable (upper {}, page size {})",
            adjustment.upper(),
            adjustment.page_size()
        );

        let target = (adjustment.upper() - adjustment.page_size()) / 2.0;
        adjustment.set_value(target);
        pump_frames();
        let value_before = adjustment.value();
        assert!(
            value_before > 0.0,
            "the list must be scrolled into the repositories"
        );

        // Favoriting repo #1 only flips its star: the row stays in its owner
        // group (there is no "Favorites" section), so the repository order
        // must not change and the list must not jump. The committed state
        // lives in the live favorites the rebuild reads.
        {
            let mut favorites = favorites_state.lock();
            favorites.insert(repos[0].id);
        }
        rebuild_repo_list(store.clone(), context());
        pump_frames();

        let order_after = repo_order(&store);
        assert_eq!(
            order_after, order_before,
            "favoriting a repository must not reorder the list"
        );

        // No section is added, so the content above the anchor is unchanged
        // and the scroll must hold its place (not jump to the top, not drift).
        let value_after = adjustment.value();
        assert!(value_after > 0.0, "the list must not jump to the top");
        assert!(
            value_after >= value_before - 50.0 && value_after <= value_before + 50.0,
            "the list must not jump away from where it was (was {}, now {})",
            value_before,
            value_after
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn rebuild_reuses_header_rows() {
    run_gtk_test("rebuild_reuses_header_rows", || {
        let panel = SidebarPanel::new();
        let window = gtk::Window::new();
        window.set_default_size(360, 320);
        window.set_child(Some(&panel.clamp()));
        window.present();
        pump_frames();

        // Two owners, several repos each: the store holds a section header,
        // two owner headers, and repo rows.
        let repos = (1..=8)
            .map(|id: i64| Repo {
                id,
                name: format!("repo-{id}"),
                full_name: format!("owner-a/repo-{id}"),
                owner: User {
                    login: "owner-a".into(),
                },
                is_private: false,
                permissions: None,
                default_branch: Some("main".into()),
            })
            .chain((101..=108).map(|id: i64| Repo {
                id,
                name: format!("repo-{id}"),
                full_name: format!("owner-b/repo-{id}"),
                owner: User {
                    login: "owner-b".into(),
                },
                is_private: false,
                permissions: None,
                default_branch: Some("main".into()),
            }))
            .collect::<Vec<_>>();

        let favorites_state = Arc::new(Mutex::new(HashSet::new()));
        let context = |repos: Vec<Repo>| RepoListRenderContext {
            repos,
            actions_snapshot: HashMap::new(),
            workflow_snapshot: HashMap::new(),
            favorites_state: favorites_state.clone(),
            favorites_manager: None,
        };

        let store = panel.repo_store();
        let repo_view = panel.repo_list();

        rebuild_repo_list(store.clone(), context(repos.clone()));
        pump_frames();

        let row_pointers = |store: &gio::ListStore| {
            (0..store.n_items())
                .filter_map(|i| store.item(i))
                .map(|obj| obj.as_ptr() as *const ())
                .collect::<Vec<_>>()
        };
        let before = row_pointers(&store);

        // The owner-b header sits at index 2 + 8 = 10 (section, owner-a
        // header, owner-a's eight repos, then owner-b). Scroll it to the top
        // so the anchor lands ON the owner header — the row type this fix
        // newly reuses — not on a repository.
        let owner_b_header = 2usize + 8;
        repo_view.scroll_to(owner_b_header as u32, gtk::ListScrollFlags::NONE, None);
        pump_frames();

        let adjustment = repo_view
            .vadjustment()
            .expect("repo list is inside a ScrolledWindow");
        let value_before = adjustment.value();
        assert!(
            value_before > 0.0,
            "the list must be scrolled past the top, anchor on the owner-b header (now {})",
            value_before
        );

        // A NON-no-op rebuild: add one repository to owner-b, below the
        // anchor. That changes the model while the owner-b header the anchor
        // sits on must be left in place and reused.
        let mut repos_after = repos.clone();
        repos_after.push(Repo {
            id: 109,
            name: "repo-109".into(),
            full_name: "owner-b/repo-109".into(),
            owner: User {
                login: "owner-b".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        });
        rebuild_repo_list(store.clone(), context(repos_after));
        pump_frames();

        let after = row_pointers(&store);
        assert_eq!(
            after.len(),
            before.len() + 1,
            "the rebuild must add exactly the one new repository"
        );
        assert_eq!(
            after[..before.len()].to_vec(),
            before,
            "every row above the new one — headers included — must be reused"
        );
        let value_after = adjustment.value();
        assert!(
            value_after > 0.0
                && value_after >= value_before - 50.0
                && value_after <= value_before + 50.0,
            "anchored on the owner-b header the list must not jump (was {}, now {})",
            value_before,
            value_after
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn rebuild_holds_position_when_workflow_counters_arrive() {
    run_gtk_test(
        "rebuild_holds_position_when_workflow_counters_arrive",
        || {
            let panel = SidebarPanel::new();
            let window = gtk::Window::new();
            window.set_default_size(360, 320);
            window.set_child(Some(&panel.clamp()));
            window.present();
            pump_frames();

            // One owner, many repos. Built first with no workflow counters (the
            // meta row hidden), the user scrolls into the list, then the counters
            // arrive and every row grows taller.
            let repos = (1..=16)
                .map(|id: i64| Repo {
                    id,
                    name: format!("repo-{id}"),
                    full_name: format!("makoni/repo-{id}"),
                    owner: User {
                        login: "makoni".into(),
                    },
                    is_private: false,
                    permissions: None,
                    default_branch: Some("main".into()),
                })
                .collect::<Vec<_>>();

            let favorites_state = Arc::new(Mutex::new(HashSet::new()));
            let store = panel.repo_store();
            let repo_view = panel.repo_list();
            let build =
                |workflow_snapshot: HashMap<i64, crate::ui::state::WorkflowStatusCounts>| {
                    rebuild_repo_list(
                        store.clone(),
                        RepoListRenderContext {
                            repos: repos.clone(),
                            actions_snapshot: HashMap::new(),
                            workflow_snapshot,
                            favorites_state: favorites_state.clone(),
                            favorites_manager: None,
                        },
                    );
                    pump_frames();
                };

            build(HashMap::new());

            let adjustment = repo_view
                .vadjustment()
                .expect("repo list is inside a ScrolledWindow");
            assert!(
                adjustment.upper() > adjustment.page_size(),
                "list must overflow to be scrollable (upper {}, page size {})",
                adjustment.upper(),
                adjustment.page_size()
            );

            // Scroll into the repositories (a mid-list anchor, not the top).
            adjustment.set_value((adjustment.upper() - adjustment.page_size()) / 2.0);
            pump_frames();
            let value_before = adjustment.value();
            assert!(
                value_before > 0.0,
                "the list must be scrolled into the repositories"
            );
            let upper_before = adjustment.upper();

            // Counters arrive: every repository now shows its active/failed row,
            // which makes each row taller. The model is unchanged (same rows,
            // reused) but the layout shifts, so the anchor must still hold.
            let workflow_snapshot = (1..=16)
                .map(|id: i64| {
                    (
                        id,
                        crate::ui::state::WorkflowStatusCounts {
                            active: 3,
                            failed: 1,
                        },
                    )
                })
                .collect();
            build(workflow_snapshot);

            // Guard against a vacuous pass: if the meta rows never grew the
            // rows, `upper` would not move and the position would hold for
            // trivial reasons. The height must actually increase.
            let upper_after = adjustment.upper();
            assert!(
                upper_after > upper_before,
                "the workflow meta rows must make the rows taller (upper was {}, now {})",
                upper_before,
                upper_after
            );

            let value_after = adjustment.value();
            // The rows above the anchor grew, so the value may have moved a
            // little, but the anchor must not have been invalidated (a jump to
            // the top would read as ~0).
            assert!(
                value_after > 0.0
                    && value_after >= value_before - 100.0
                    && value_after <= adjustment.upper() - adjustment.page_size(),
                "the anchor must hold when rows grow taller (was {}, now {}, upper {}, page {})",
                value_before,
                value_after,
                adjustment.upper(),
                adjustment.page_size()
            );
        },
    );
}
