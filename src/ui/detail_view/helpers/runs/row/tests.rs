//! Tests for [`super`].
//!
//! Split out of `row.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::{Job, Repo, User};
use crate::services::gateway::GitHubGateway;
use crate::ui::test_helpers::run_gtk_test;
use parking_lot::Mutex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

fn repo_stub() -> Repo {
    Repo {
        id: 1,
        name: "actioneer".into(),
        full_name: "mak/actioneer".into(),
        owner: User {
            login: "mak".into(),
        },
        is_private: false,
        permissions: None,
        default_branch: Some("main".into()),
    }
}

fn client_stub() -> Arc<Mutex<GitHubGateway>> {
    Arc::new(Mutex::new(GitHubGateway::demo()))
}

fn run_stub() -> WorkflowRun {
    WorkflowRun {
        id: 42,
        run_number: Some(128),
        workflow_id: Some(7),
        name: Some("CI".into()),
        display_title: Some("Bundle flatpak manifest".into()),
        head_branch: Some("main".into()),
        head_commit: None,
        status: Some("in_progress".into()),
        conclusion: None,
        run_started_at: None,
        event: None,
        created_at: None,
        updated_at: None,
        html_url: None,
        actor: None,
        triggering_actor: None,
    }
}

fn context_stub() -> RunRowContext {
    RunRowContext::new(
        client_stub(),
        "mak".into(),
        "actioneer".into(),
        repo_stub(),
        adw::ApplicationWindow::builder().build(),
        7,
        adw::ToastOverlay::new(),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
    )
}

fn params_for(
    expander: &gtk::Expander,
    jobs_box: &gtk::Box,
    jobs: Arc<Vec<Job>>,
) -> JobRefreshContextParams {
    JobRefreshContextParams {
        client: client_stub(),
        owner: "mak".into(),
        repo: "actioneer".into(),
        workflow_id: 7,
        run_id: 42,
        expander: expander.clone(),
        jobs_box: jobs_box.clone(),
        parent_window: gtk::Window::new(),
        repo_model: repo_stub(),
        branch: Some("main".into()),
        run_title: "CI".into(),
        jobs,
        job_summaries: Rc::new(RefCell::new(HashMap::new())),
    }
}

#[test]
#[ignore = "requires GTK display"]
fn run_row_is_released_when_dropped() {
    run_gtk_test("run_row_is_released_when_dropped", || {
        // Regression guard: the row's own box must never be captured strongly by
        // a handler on a widget it owns. Such a cycle is invisible at runtime —
        // it silently keeps the row alive, which in turn defeats the weak-ref
        // guard on its 60s refresh timer and leaves it ticking forever.
        let mut weaks: Vec<(String, glib::WeakRef<gtk::Widget>)> = Vec::new();
        let weak = {
            let context = context_stub();
            // A running run with a start also runs the wall-clock duration
            // ticker; the release must survive it.
            let mut run = run_stub();
            run.status = Some("in_progress".into());
            run.run_started_at = Some("2024-01-01T00:00:00Z".into());
            let row = create_run_expander_row(&run, &context, false);
            crate::ui::test_helpers::collect_widget_weaks(
                &row.clone().upcast::<gtk::Widget>(),
                &mut weaks,
            );
            row.downgrade()
        };

        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }

        assert!(
            weak.upgrade().is_none(),
            "run row outlived its last strong reference — a signal handler is \
             holding it in a reference cycle"
        );

        // Checking the outer box alone is not enough: a cycle pinning only the
        // expander leaves `meta_box` alive, and with it the 60s refresh timer
        // whose weak upgrade then never fails.
        let survivors: Vec<&str> = weaks
            .iter()
            .filter(|(_, weak)| weak.upgrade().is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            survivors.is_empty(),
            "widgets outlived the discarded run row: {survivors:?} — a signal \
             handler is holding them in a reference cycle. This covers the \
             widget tree only: a leak that pins no widget is invisible here."
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn run_row_renders_number_dot_and_meta() {
    run_gtk_test("run_row_renders_number_dot_and_meta", || {
        let context = context_stub();
        let row = create_run_expander_row(&run_stub(), &context, false);

        assert!(row.has_css_class("run-item"));

        let row_container = row
            .first_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("row container");
        let expander = row_container
            .first_child()
            .and_then(|child| child.downcast::<gtk::Expander>().ok())
            .expect("run expander");
        assert_eq!(expander.widget_name().as_str(), "run_42");

        let header = expander
            .label_widget()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("header box");
        let dot = header
            .first_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("status dot");
        assert!(dot.has_css_class("status-dot"));
        assert!(dot.has_css_class("accent"));
    });
}

#[test]
#[ignore = "requires GTK display"]
fn remove_job_context_only_removes_matching_expander() {
    run_gtk_test("remove_job_context_only_removes_matching_expander", || {
        let old_expander = gtk::Expander::new(None);
        let new_expander = gtk::Expander::new(None);
        let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        jobs_box.append(&gtk::Spinner::new());
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));

        job_contexts.borrow_mut().insert(
            42,
            JobRefreshContext::from_params(params_for(
                &new_expander,
                &jobs_box,
                Arc::new(Vec::new()),
            )),
        );

        remove_job_context_if_current(&job_contexts, 42, &old_expander);
        assert!(job_contexts.borrow().contains_key(&42));

        remove_job_context_if_current(&job_contexts, 42, &new_expander);
        assert!(!job_contexts.borrow().contains_key(&42));
    });
}

#[test]
#[ignore = "requires GTK display"]
fn rebind_preserved_job_context_skips_placeholders() {
    run_gtk_test("rebind_preserved_job_context_skips_placeholders", || {
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        jobs_box.append(&gtk::Label::new(Some("placeholder")));

        rebind_preserved_job_context(
            &job_contexts,
            params_for(&gtk::Expander::new(None), &jobs_box, Arc::new(Vec::new())),
        );

        assert!(job_contexts.borrow().is_empty());
    });
}

#[test]
#[ignore = "requires GTK display"]
fn rebind_preserved_job_context_keeps_cached_jobs() {
    run_gtk_test("rebind_preserved_job_context_keeps_cached_jobs", || {
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let old_expander = gtk::Expander::new(None);
        let old_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        old_jobs_box.append(&gtk::Spinner::new());

        let cached_jobs = Arc::new(vec![Job {
            id: 1,
            run_id: 42,
            status: Some("in_progress".into()),
            conclusion: None,
            started_at: None,
            completed_at: None,
            name: Some("Build".into()),
            steps: Vec::new(),
            html_url: None,
        }]);

        job_contexts.borrow_mut().insert(
            42,
            JobRefreshContext::from_params(params_for(&old_expander, &old_jobs_box, cached_jobs)),
        );

        let new_expander = gtk::Expander::new(None);
        let new_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        new_jobs_box.append(&gtk::Spinner::new());

        rebind_preserved_job_context(
            &job_contexts,
            params_for(&new_expander, &new_jobs_box, Arc::new(Vec::new())),
        );

        let contexts = job_contexts.borrow();
        let rebound = contexts.get(&42).expect("context preserved");
        assert_eq!(rebound.jobs().len(), 1);
        assert!(rebound.matches_expander(&new_expander));
    });
}
