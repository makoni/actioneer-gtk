//! Tests for [`super`].
//!
//! Split out of `workflows.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::User;
use crate::ui::detail_view::filter_controls::FilterControls;
use crate::ui::test_helpers::run_gtk_test;

fn find_expander(widget: gtk::Widget) -> Option<gtk::Expander> {
    if let Ok(expander) = widget.clone().downcast::<gtk::Expander>() {
        return Some(expander);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_expander(current.clone()) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn row_context_stub() -> WorkflowRowContext {
    let workflows_last_loaded = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let workflows_loading_runs = Arc::new(Mutex::new(HashSet::new()));
    let controls = FilterControls::new();

    WorkflowRowContext {
        client: Arc::new(Mutex::new(crate::services::gateway::GitHubGateway::demo())),
        owner: "mak".into(),
        repo: "actioneer".into(),
        repo_model: Repo {
            id: 1,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        },
        parent_window: adw::ApplicationWindow::builder().build(),
        toast_overlay: adw::ToastOverlay::new(),
        job_contexts: Rc::new(RefCell::new(HashMap::new())),
        run_badge_summaries: Rc::new(RefCell::new(HashMap::new())),
        workflows_with_active_runs: Arc::new(Mutex::new(HashSet::new())),
        workflows_last_loaded: workflows_last_loaded.clone(),
        workflows_loading_runs: workflows_loading_runs.clone(),
        run_digests: Arc::new(Mutex::new(std::collections::HashMap::new())),
        notification_manager: None,
        preferences_manager: None,
        run_filters: Arc::new(Mutex::new(crate::ui::detail_view::RunFilters::default())),
        run_load_service: super::super::RunLoadService::new(
            workflows_last_loaded,
            workflows_loading_runs,
        ),
        header: DetailHeaderState::new(
            gtk::Label::new(None),
            gtk::Label::new(None),
            controls.chips.clone(),
        ),
    }
}

#[test]
#[ignore = "requires GTK display"]
fn workflow_row_is_released_when_dropped() {
    run_gtk_test("workflow_row_is_released_when_dropped", || {
        // Regression guard for two cycles that used to pin every workflow row:
        // the trigger handler capturing its own expander, and the run-list model
        // holding a header that held the very buttons whose handler owns that
        // model. While either existed the row's 1s elapsed ticker could never
        // stop, so rebuilt rows accumulated live timers.
        // Both the row *and* its expander must die: a cycle that only pins the
        // expander still leaks the whole subtree hanging off it, while the outer
        // box is released normally.
        let mut weaks: Vec<(String, glib::WeakRef<gtk::Widget>)> = Vec::new();
        let (row_weak, expander_weak) = {
            let context = row_context_stub();
            let workflow = Workflow {
                id: 7,
                name: "CI".into(),
                path: ".github/workflows/ci.yml".into(),
            };
            let row = create_workflow_expander_row(
                &workflow,
                &context,
                WorkflowRowSettings {
                    should_expand: false,
                    initial_expanded_run_ids: Vec::new(),
                    is_first: true,
                },
            );
            let expander = find_expander(row.clone().upcast::<gtk::Widget>())
                .expect("workflow row should contain an expander");
            crate::ui::test_helpers::collect_widget_weaks(
                &row.clone().upcast::<gtk::Widget>(),
                &mut weaks,
            );
            (row.downgrade(), expander.downgrade())
        };

        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }

        assert!(
            row_weak.upgrade().is_none(),
            "workflow row outlived its last strong reference — a signal handler \
             is holding it in a reference cycle"
        );
        assert!(
            expander_weak.upgrade().is_none(),
            "workflow expander outlived its row — a handler on a widget inside \
             the expander is capturing the expander itself"
        );

        // Nothing hung off the row may survive either: a cycle can pin a single
        // button (and through it the run-list model and the pane) while the row
        // and expander themselves are released normally.
        let survivors: Vec<&str> = weaks
            .iter()
            .filter(|(_, weak)| weak.upgrade().is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            survivors.is_empty(),
            "widgets outlived the discarded workflow row: {survivors:?} — a \
             signal handler is holding them in a reference cycle. List-view \
             rows bind lazily and are covered by the run-row test instead."
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn trigger_dialog_starts_with_disabled_trigger() {
    run_gtk_test("trigger_dialog_starts_with_disabled_trigger", || {
        let dialog = build_trigger_dialog("Trigger Workflow");

        assert!(dialog.has_response("cancel"));
        assert!(dialog.has_response("trigger"));
        assert_eq!(dialog.default_response().as_deref(), Some("trigger"));
        assert_eq!(dialog.close_response().as_str(), "cancel");
        assert_eq!(
            dialog.response_appearance("trigger"),
            adw::ResponseAppearance::Suggested
        );
        // The trigger action is disabled until branches and inputs finish loading.
        assert!(!dialog.is_response_enabled("trigger"));
        assert!(dialog.is_response_enabled("cancel"));
    });
}
