//! Tests for [`super`].
//!
//! Split out of `sidebar.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::{Repo, User};
use crate::ui::state::WorkflowStatusCounts;
use crate::ui::test_helpers::run_gtk_test;
use gtk4::{self as gtk, gio};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[test]
#[ignore = "requires GTK display"]
fn favourite_buttons_line_up_regardless_of_name_length() {
    run_gtk_test(
        "favourite_buttons_line_up_regardless_of_name_length",
        || {
            fn row_for(full_name: &str) -> gtk::Box {
                build_repo_row(
                    Repo {
                        id: full_name.len() as i64,
                        name: full_name.rsplit('/').next().unwrap_or(full_name).into(),
                        full_name: full_name.into(),
                        owner: User {
                            login: "makoni".into(),
                        },
                        is_private: false,
                        permissions: None,
                        default_branch: Some("main".into()),
                    },
                    false,
                    WorkflowStatusCounts::default(),
                    Arc::new(Mutex::new(HashSet::new())),
                    None,
                )
            }

            // A short name and one long enough to be ellipsized: packed without
            // hexpand the star trails the label, so the column zig-zags.
            let rows = [
                row_for("makoni/imetrik"),
                row_for("makoni/Google-Maps-SDK-for-something-long"),
            ];
            let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
            for row in &rows {
                list.append(row);
            }

            let window = gtk::Window::new();
            window.set_default_size(320, 200);
            window.set_child(Some(&list));
            window.present();
            while glib::MainContext::default().pending() {
                let _ = glib::MainContext::default().iteration(false);
            }

            let right_edges: Vec<i32> = rows
                .iter()
                .map(|row| {
                    let button = row
                        .last_child()
                        .and_then(|child| child.downcast::<gtk::ToggleButton>().ok())
                        .expect("each row ends with its favourite button");
                    let bounds = button
                        .compute_bounds(row)
                        .expect("button bounds in row coordinates");
                    (bounds.x() + bounds.width()).round() as i32
                })
                .collect();

            window.destroy();

            assert_eq!(
                right_edges[0], right_edges[1],
                "favourite buttons must share a trailing edge, got {right_edges:?}"
            );
        },
    );
}

#[test]
#[ignore = "requires GTK display"]
fn repo_rows_use_widget_metadata_without_listbox_rows() {
    run_gtk_test("repo_rows_use_widget_metadata_without_listbox_rows", || {
        let store = gio::ListStore::new::<gtk::Widget>();
        let repo = Repo {
            id: 42,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        };

        rebuild_repo_list(
            store.clone(),
            RepoListRenderContext {
                repos: vec![repo.clone()],
                actions_snapshot: HashMap::new(),
                workflow_snapshot: HashMap::from([(repo.id, WorkflowStatusCounts::default())]),
                favorites_state: Arc::new(Mutex::new(HashSet::new())),
                favorites_manager: None,
            },
        );

        let repo_obj = (0..store.n_items())
            .find_map(|idx| {
                store
                    .item(idx)
                    .filter(|obj| repo_from_object(obj).is_some())
            })
            .expect("repo item should exist");

        assert!(repo_obj.downcast_ref::<gtk::ListBoxRow>().is_none());
        assert_eq!(
            repo_from_object(&repo_obj).map(|item| item.id),
            Some(repo.id)
        );
        assert!(row_selectable_from_object(repo_obj.as_ref()));
    });
}

#[test]
#[ignore = "requires GTK display"]
fn header_rows_are_not_activatable_and_repo_rows_are() {
    run_gtk_test("header_rows_are_not_activatable_and_repo_rows_are", || {
        let store = gio::ListStore::new::<gtk::Widget>();
        let repo = Repo {
            id: 7,
            name: "repo-7".into(),
            full_name: "makoni/repo-7".into(),
            owner: User {
                login: "makoni".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        };

        rebuild_repo_list(
            store.clone(),
            RepoListRenderContext {
                repos: vec![repo],
                actions_snapshot: HashMap::new(),
                workflow_snapshot: HashMap::new(),
                favorites_state: Arc::new(Mutex::new(HashSet::new())),
                favorites_manager: None,
            },
        );

        // Load-bearing for the hover: the factory turns this flag into
        // `set_activatable`, and GTK turns that into the `.activatable`
        // class on the list row — which is what the hover rule in
        // `style.rs` selects. Headers must stay out of it, or they light up
        // under the pointer as if they were clickable.
        let mut saw_repo = false;
        let mut saw_header = false;
        for i in 0..store.n_items() {
            let row = store.item(i).expect("store item");
            let is_repo = repo_id_from_object(&row).is_some();
            assert_eq!(
                row_activatable_from_object(&row),
                is_repo,
                "row {i} (is_repo={is_repo}): only repo rows may be activatable"
            );
            saw_repo |= is_repo;
            saw_header |= !is_repo;
        }
        assert!(saw_repo, "expected at least one repo row");
        assert!(saw_header, "expected at least one section/owner header");
    });
}

#[test]
// NOTE: on regression this test does NOT fail fast — the wedged worker hits
// `GTK_TEST_TIMEOUT` (30 s), is marked wedged, and every GTK test queued after
// it fails too, so the whole UI suite looks collapsed. A red `favorite_*` test
// here means the favorites path broke; start the hunt here, not in the cascade.
#[ignore = "requires GTK display"]
fn favorite_err_result_applied_to_button_does_not_deadlock() {
    run_gtk_test(
        "favorite_err_result_applied_to_button_does_not_deadlock",
        || {
            let favorites = Arc::new(Mutex::new(HashSet::new()));
            let row = build_repo_row(
                Repo {
                    id: 42,
                    name: "repo-42".into(),
                    full_name: "makoni/repo-42".into(),
                    owner: User {
                        login: "makoni".into(),
                    },
                    is_private: false,
                    permissions: None,
                    default_branch: Some("main".into()),
                },
                false,
                WorkflowStatusCounts::default(),
                favorites.clone(),
                None,
            );
            let button = get_data_clone::<gtk::ToggleButton, _>(&row, REPO_FAV_BUTTON_KEY)
                .expect("the favorite button is stashed on the row");

            // A user click flips the star on, so the button is now active.
            button.set_active(true);

            // The favorite write then failed, so the stored (pre-click) state is
            // `false` and diverges from the active star. Applying that result must
            // not wedge the GTK thread: pre-fix the receiver held the favorites
            // lock across `set_active`, which re-entered `toggled` and locked the
            // same non-reentrant Mutex.
            apply_favorite_result(
                &favorites,
                42,
                &button,
                Err((anyhow::anyhow!("simulated disk full"), false)),
            );

            // The star is restored to the stored state and the cache agrees.
            assert!(!button.is_active());
            assert!(!favorites.lock().contains(&42));
        },
    );
}
