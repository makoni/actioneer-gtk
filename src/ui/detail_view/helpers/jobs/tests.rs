//! Tests for [`super`].
//!
//! Split out of `jobs.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::{JobStep, User};
use crate::ui::test_helpers::run_gtk_test;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn step_stub(status: Option<&str>, started_at: Option<&str>) -> JobStep {
    JobStep {
        name: Some("Build".into()),
        status: status.map(str::to_string),
        conclusion: None,
        number: Some(7),
        started_at: started_at.map(str::to_string),
        completed_at: None,
    }
}

#[test]
#[ignore = "requires GTK display"]
fn step_rows_use_contiguous_display_numbers() {
    run_gtk_test("step_rows_use_contiguous_display_numbers", || {
        // GitHub step numbers can be sparse, so rows are numbered by position.
        let row = create_job_step_row(&step_stub(Some("completed"), None), 3);
        let index_label = row
            .first_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("step row should start with its index label");
        assert_eq!(index_label.text().as_str(), "3");

        let unnamed = JobStep {
            name: None,
            ..step_stub(Some("queued"), None)
        };
        let row = create_job_step_row(&unnamed, 4);
        let name_label = row
            .last_child()
            .and_then(|child| child.prev_sibling())
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("step row should carry a name label");
        assert_eq!(name_label.text().as_str(), "#4");
    });
}

#[test]
#[ignore = "requires GTK display"]
fn test_job_row_layout_properties() {
    run_gtk_test("test_job_row_layout_properties", || {
        let job = Job {
            id: 1,
            run_id: 1,
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            started_at: Some("2024-01-01T00:00:00Z".to_string()),
            completed_at: Some("2024-01-01T00:05:00Z".to_string()),
            name: Some("Test Job".to_string()),
            steps: Vec::new(),
            html_url: Some("https://github.com/test".to_string()),
        };

        let job_row = create_job_row_simple(&job, None);

        assert_eq!(job_row.orientation(), gtk::Orientation::Vertical);
        assert!(job_row.has_css_class("job-card"));

        let header = job_row
            .first_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("job header row");

        let dot = header
            .first_child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
            .expect("status dot");
        assert!(dot.has_css_class("status-dot"));
        assert!(dot.has_css_class("success"));

        let name_label = dot
            .next_sibling()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("job name label");
        assert_eq!(name_label.text().as_str(), "Test Job");
        assert!(name_label.has_css_class("job-name"));
        assert!(name_label.hexpands());
        assert_eq!(name_label.halign(), gtk::Align::Start);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn in_flight_job_load_flag_blocks_duplicate_start() {
    run_gtk_test("in_flight_job_load_flag_blocks_duplicate_start", || {
        let expander = gtk::Expander::new(None::<&str>);
        assert!(try_begin_job_load(&expander));
        assert!(!try_begin_job_load(&expander));
        finish_job_load(&expander);
        assert!(try_begin_job_load(&expander));
    });
}

#[test]
#[ignore = "requires GTK display"]
fn provisional_context_preserves_cached_jobs() {
    run_gtk_test("provisional_context_preserves_cached_jobs", || {
        let client = Arc::new(Mutex::new(GitHubGateway::demo()));
        let repo = Repo {
            id: 1,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            default_branch: Some("main".into()),
            is_private: false,
            permissions: None,
        };
        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        let job_summaries: RunBadgeSummaryMap = Rc::new(RefCell::new(HashMap::new()));
        let cached_jobs = Arc::new(vec![Job {
            id: 10,
            run_id: 42,
            status: Some("in_progress".into()),
            conclusion: None,
            started_at: None,
            completed_at: None,
            name: Some("Build".into()),
            steps: Vec::new(),
            html_url: None,
        }]);

        store_job_refresh_context(
            &job_contexts,
            client.clone(),
            "mak".into(),
            "actioneer".into(),
            7,
            42,
            gtk::Expander::new(None::<&str>),
            gtk::Box::new(gtk::Orientation::Vertical, 0),
            gtk::Window::new(),
            repo.clone(),
            Some("main".into()),
            "CI".into(),
            cached_jobs.clone(),
            job_summaries,
        );

        let preserved = cached_jobs_for_run(&job_contexts, 42);
        assert_eq!(preserved.len(), 1);
        assert_eq!(preserved[0].id, 10);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn retry_uses_current_context_widgets_when_available() {
    run_gtk_test("retry_uses_current_context_widgets_when_available", || {
        let fallback_expander = gtk::Expander::new(None::<&str>);
        let fallback_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let current_expander = gtk::Expander::new(None::<&str>);
        let current_jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        job_contexts.borrow_mut().insert(
            42,
            JobRefreshContext::from_params(JobRefreshContextParams {
                client: Arc::new(Mutex::new(GitHubGateway::demo())),
                owner: "mak".into(),
                repo: "actioneer".into(),
                workflow_id: 7,
                run_id: 42,
                expander: current_expander.clone(),
                jobs_box: current_jobs_box.clone(),
                parent_window: gtk::Window::new(),
                repo_model: Repo {
                    id: 1,
                    name: "actioneer".into(),
                    full_name: "mak/actioneer".into(),
                    owner: User {
                        login: "mak".into(),
                    },
                    default_branch: Some("main".into()),
                    is_private: false,
                    permissions: None,
                },
                branch: Some("main".into()),
                run_title: "CI".into(),
                jobs: Arc::new(Vec::new()),
                job_summaries: Rc::new(RefCell::new(HashMap::new())),
            }),
        );

        let (resolved_expander, resolved_jobs_box) =
            current_retry_widgets(&job_contexts, 42, &fallback_expander, &fallback_jobs_box);

        assert_eq!(resolved_expander, current_expander);
        assert_eq!(resolved_jobs_box, current_jobs_box);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn test_job_row_renders_step_labels() {
    run_gtk_test("test_job_row_renders_step_labels", || {
        let job = Job {
            id: 1,
            run_id: 1,
            status: Some("in_progress".to_string()),
            conclusion: None,
            started_at: Some("2024-01-01T00:00:00Z".to_string()),
            completed_at: None,
            name: Some("Test Job".to_string()),
            steps: vec![
                JobStep {
                    name: Some("Checkout".to_string()),
                    status: Some("completed".to_string()),
                    conclusion: Some("success".to_string()),
                    number: Some(1),
                    started_at: Some("2024-01-01T00:00:00Z".to_string()),
                    completed_at: Some("2024-01-01T00:00:10Z".to_string()),
                },
                JobStep {
                    name: Some("Run tests".to_string()),
                    status: Some("in_progress".to_string()),
                    conclusion: None,
                    number: Some(2),
                    started_at: Some("2024-01-01T00:00:10Z".to_string()),
                    completed_at: None,
                },
            ],
            html_url: Some("https://github.com/test".to_string()),
        };

        let job_row = create_job_row_simple(&job, None);

        let mut found_checkout = false;
        let mut found_tests = false;
        let mut stack = vec![job_row.upcast::<gtk::Widget>()];
        while let Some(widget) = stack.pop() {
            if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                let text = label.text();
                if text.contains("Checkout") {
                    found_checkout = true;
                }
                if text.contains("Run tests") {
                    found_tests = true;
                }
            }

            let mut child = widget.first_child();
            while let Some(next) = child {
                stack.push(next.clone());
                child = next.next_sibling();
            }
        }

        assert!(found_checkout);
        assert!(found_tests);
    });
}
