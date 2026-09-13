//! Tests for [`super`].
//!
//! Split out of `job_logs_window.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

use super::*;
use crate::services::api::models::Job;
use crate::ui::test_helpers::run_gtk_test;

fn repo_stub() -> Repo {
    Repo {
        id: 1,
        name: "repo".into(),
        full_name: "owner/repo".into(),
        owner: crate::services::api::models::User {
            login: "owner".into(),
        },
        is_private: false,
        permissions: None,
        default_branch: Some("main".into()),
    }
}

/// The duration label is the only `mono` label in a row, so hunt it by
/// class rather than by position in the row's layout.
fn mono_labels_in(row: &gtk::Widget) -> Vec<gtk::Label> {
    let mut found = Vec::new();
    let mut stack = vec![row.clone()];
    while let Some(widget) = stack.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk::Label>()
            && label.has_css_class("mono")
        {
            found.push(label);
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            stack.push(next.clone());
            child = next.next_sibling();
        }
    }
    found
}

fn job_stub(name: &str, conclusion: Option<&str>) -> Job {
    Job {
        id: name.len() as i64,
        run_id: 1,
        name: Some(name.to_string()),
        status: Some("completed".into()),
        conclusion: conclusion.map(str::to_string),
        started_at: None,
        completed_at: None,
        html_url: None,
        steps: Vec::new(),
    }
}

#[test]
fn most_relevant_job_prefers_the_failure_then_the_running_one() {
    let jobs = vec![
        job_stub("lock-sync", Some("success")),
        job_stub("build", Some("failure")),
        job_stub("publish", Some("success")),
    ];
    // Reading a run almost always means reading why it failed.
    assert_eq!(most_relevant_job(&jobs), Some(1));

    let jobs = vec![
        job_stub("lock-sync", Some("success")),
        job_stub("build", None),
    ];
    assert_eq!(most_relevant_job(&jobs), Some(1));

    // All green: nothing stands out, so the first job is as good as any.
    let jobs = vec![
        job_stub("lock-sync", Some("success")),
        job_stub("build", Some("success")),
    ];
    assert_eq!(most_relevant_job(&jobs), Some(0));

    assert_eq!(most_relevant_job(&[]), None);
}

#[test]
#[ignore = "requires GTK display"]
fn run_with_several_jobs_lists_them_all() {
    run_gtk_test("run_with_several_jobs_lists_them_all", || {
        let jobs = vec![
            job_stub("lock-sync", Some("success")),
            job_stub("fmt-clippy", Some("success")),
            job_stub("build-test", Some("failure")),
        ];
        let list = build_job_list(&jobs, 2);

        let mut rows = 0;
        let mut child = list.first_child();
        while let Some(row) = child {
            rows += 1;
            child = row.next_sibling();
        }
        assert_eq!(rows, 3, "every job of the run must be offered");

        // The failed job is preselected: that is what the reader came for.
        let selected = list.selected_row().expect("a job should be selected");
        assert_eq!(selected.index(), 2);
    });
}

#[test]
#[ignore = "requires GTK display"]
fn sidebar_ticks_the_duration_of_a_running_job() {
    run_gtk_test("sidebar_ticks_the_duration_of_a_running_job", || {
        let started = (chrono::Utc::now() - chrono::Duration::seconds(72)).to_rfc3339();
        let running = Job {
            id: 1,
            run_id: 1,
            name: Some("build".into()),
            status: Some("in_progress".into()),
            conclusion: None,
            started_at: Some(started.clone()),
            completed_at: None,
            html_url: None,
            steps: Vec::new(),
        };
        let queued = Job {
            id: 2,
            run_id: 1,
            name: Some("deploy".into()),
            status: Some("queued".into()),
            conclusion: None,
            started_at: Some(started),
            completed_at: None,
            html_url: None,
            steps: Vec::new(),
        };

        let list = build_job_list(&[running, queued], 0);
        // The ticker only runs while its label is inside a toplevel, so the
        // floating list needs a window of its own here.
        let window = gtk::Window::new();
        window.set_child(Some(&list));

        let running_row = list.row_at_index(0).expect("a running job row");
        let queued_row = list.row_at_index(1).expect("a queued job row");

        // A running job shows its wall-clock elapsed time, like the main window.
        let running_labels = mono_labels_in(running_row.upcast_ref::<gtk::Widget>());
        assert_eq!(
            running_labels.len(),
            1,
            "a running job shows exactly one duration label"
        );
        let initial = running_labels[0].text();
        assert!(
            ["01:12", "01:13"].contains(&initial.as_str()),
            "expected a live mm:ss count, got {initial:?}"
        );

        // And it keeps counting: the label must be attached to the ticker,
        // not merely show the initial value. Pump non-blocking with a sleep
        // between passes: a blocking iteration would wait on a scheduler
        // that is not there in the failure case this assertion guards.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while running_labels[0].text() == initial && std::time::Instant::now() < deadline {
            glib::MainContext::default().iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_ne!(
            running_labels[0].text(),
            initial,
            "the running job's duration must keep counting"
        );

        // A queued job has not started executing: no timer of any kind.
        assert!(
            mono_labels_in(queued_row.upcast_ref::<gtk::Widget>()).is_empty(),
            "a queued job shows no duration"
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn apply_jobs_rebuilds_the_sidebar_and_keeps_the_reader() {
    run_gtk_test(
        "apply_jobs_rebuilds_the_sidebar_and_keeps_the_reader",
        || {
            let running = Job {
                id: 1,
                run_id: 7,
                name: Some("build".into()),
                status: Some("in_progress".into()),
                conclusion: None,
                started_at: Some((chrono::Utc::now() - chrono::Duration::seconds(72)).to_rfc3339()),
                completed_at: None,
                html_url: None,
                steps: Vec::new(),
            };
            let queued = Job {
                id: 2,
                run_id: 7,
                name: Some("deploy".into()),
                status: Some("queued".into()),
                conclusion: None,
                started_at: None,
                completed_at: None,
                html_url: None,
                steps: Vec::new(),
            };
            let parent = gtk::Window::new();
            let logs = JobLogsWindow::build(
                &parent,
                repo_stub(),
                "CI • main #1".to_string(),
                vec![running, queued],
                0,
                Arc::new(Mutex::new(GitHubGateway::demo())),
            );
            logs.present();

            // The run changed while the reader was watching: their job finished
            // and a retry pushed it to the back of the list. The sidebar must
            // follow the job by id and swap the live counter for the final duration.
            let retry = Job {
                id: 3,
                run_id: 7,
                name: Some("retry".into()),
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                started_at: None,
                completed_at: None,
                html_url: None,
                steps: Vec::new(),
            };
            let finished = Job {
                id: 1,
                run_id: 7,
                name: Some("build".into()),
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                started_at: Some("2026-01-24T10:00:00Z".into()),
                completed_at: Some("2026-01-24T10:00:18Z".into()),
                html_url: None,
                steps: Vec::new(),
            };
            logs.ctx.apply_jobs(vec![retry, finished]);

            let list = logs
                .ctx
                .job_list
                .upgrade()
                .expect("the sidebar is on screen");
            let mut rows = 0;
            let mut child = list.first_child();
            while let Some(row) = child {
                rows += 1;
                child = row.next_sibling();
            }
            assert_eq!(rows, 2, "the sidebar is rebuilt from the fresh jobs");
            let selected = list
                .selected_row()
                .expect("the reader's job stays selected");
            assert_eq!(
                selected.index(),
                1,
                "reselection follows the job by id, not by position"
            );

            // The live counter is gone: the label carries the final duration.
            let labels = mono_labels_in(selected.upcast_ref::<gtk::Widget>());
            assert_eq!(labels.len(), 1);
            assert_eq!(
                labels[0].text().as_str(),
                "00:18",
                "a finished job shows its final duration, not wall-clock time"
            );
        },
    );
}

#[test]
#[ignore = "requires GTK display"]
fn apply_jobs_keeps_the_snapshot_on_an_empty_answer() {
    run_gtk_test("apply_jobs_keeps_the_snapshot_on_an_empty_answer", || {
        let parent = gtk::Window::new();
        let logs = JobLogsWindow::build(
            &parent,
            repo_stub(),
            "CI • main #1".to_string(),
            vec![job_stub("build", Some("success")), job_stub("test", None)],
            1,
            Arc::new(Mutex::new(GitHubGateway::demo())),
        );
        logs.present();

        // Retention can empty a run between opening and refresh, and GitHub
        // answers that with an empty array, not an error. The reader stays
        // on the old snapshot, and the window must not walk a job out of
        // an empty vector on the next refresh, save, or log reply.
        logs.ctx.apply_jobs(Vec::new());
        logs.window.close();

        let jobs = logs.ctx.jobs.borrow();
        assert_eq!(jobs.len(), 2, "the old snapshot is kept");
        assert_eq!(logs.ctx.selected.get(), 1);
        assert_eq!(
            logs.ctx.selected_job_id(),
            jobs[1].id,
            "an empty answer must not leave the window without a job"
        );
        drop(jobs);

        let list = logs
            .ctx
            .job_list
            .upgrade()
            .expect("the sidebar is on screen");
        let mut rows = 0;
        let mut child = list.first_child();
        while let Some(row) = child {
            rows += 1;
            child = row.next_sibling();
        }
        assert_eq!(rows, 2, "the sidebar is untouched");
    });
}

/// Demo data is global: switch it off however the test ends, or an
/// unrelated test that builds a client picks up the demo data.
#[test]
#[ignore = "requires GTK display"]
fn apply_jobs_resyncs_when_the_readers_job_leaves_the_run() {
    run_gtk_test(
        "apply_jobs_resyncs_when_the_readers_job_leaves_the_run",
        || {
            // Each test owns its own backend now — there is no global to
            // enable or tear down. The demo methods do no I/O, so a trivial
            // executor resolves them without a Tokio runtime.
            let backend = crate::demo::DemoBackend::new();
            // The reader is watching the in-progress job of the demo's
            // running CI run.
            let jobs = futures::executor::block_on(backend.list_jobs(
                "demo-org",
                "actioneer-demo-app",
                30_108,
            ))
            .expect("demo data has the running CI run's jobs");
            let reader_job = jobs
                .iter()
                .find(|job| job.status.as_deref() == Some("in_progress"))
                .expect("the demo run has an in-progress job");
            let selected = jobs
                .iter()
                .position(|job| job.id == reader_job.id)
                .expect("the reader's job is in the list");
            let remaining: Vec<Job> = jobs
                .iter()
                .filter(|job| job.id != reader_job.id)
                .cloned()
                .collect();

            let parent = gtk::Window::new();
            let logs = JobLogsWindow::build(
                &parent,
                repo_stub(),
                "CI • main #134".to_string(),
                jobs.clone(),
                selected,
                Arc::new(Mutex::new(GitHubGateway::demo())),
            );
            logs.present();
            logs.ctx.text_view.buffer().set_text("the old job's log");

            // Pre-store the survivor's log: the resync's load_logs(false)
            // must take the cache path and render synchronously. A spawned
            // fetch would race the DemoData guard at the end of this test
            // and, losing the race, fall through to the real API.
            let survivor = remaining
                .first()
                .expect("a job remains after the reader's job is gone");
            logs.ctx.cache.store(survivor, "prepare's log");

            // Retention ate the job the reader was on. The window must resync
            // to what is left: the title, the log, and the selection.
            logs.ctx.apply_jobs(remaining);

            let list = logs
                .ctx
                .job_list
                .upgrade()
                .expect("the sidebar is on screen");
            let selected = list.selected_row().expect("a fallback row is selected");
            assert_eq!(
                selected.index(),
                0,
                "the fallback row is the first of the jobs that remain"
            );
            assert_eq!(
                logs.ctx.job_label.text().as_str(),
                "prepare",
                "the title follows the reader to the job that remains"
            );
            let buffer = logs.ctx.text_view.buffer();
            let (start, end) = buffer.bounds();
            assert_eq!(
                buffer.text(&start, &end, false).as_str(),
                "prepare's log",
                "the vanished job's log is replaced by the survivor's"
            );
        },
    );
}

#[test]
fn cache_keeps_finished_jobs_and_skips_running_ones() {
    let cache = LogCache::default();
    let finished = job_stub("build", Some("failure"));
    let running = job_stub("deploy", None);

    cache.store(&finished, "boom");
    cache.store(&running, "half a log");

    assert_eq!(cache.get(finished.id).as_deref(), Some("boom"));
    // A running job is still writing: a cached copy would freeze its log at
    // whatever the first read caught, and Refresh would never move.
    assert_eq!(cache.get(running.id), None);
}

#[test]
#[ignore = "requires GTK display"]
fn a_closed_window_is_not_worth_rendering_into() {
    run_gtk_test("a_closed_window_is_not_worth_rendering_into", || {
        let parent = gtk::Window::new();
        let logs = JobLogsWindow::build(
            &parent,
            repo_stub(),
            "CI • main #1".to_string(),
            vec![job_stub("build", Some("success"))],
            0,
            Arc::new(Mutex::new(GitHubGateway::demo())),
        );

        logs.present();
        assert!(
            logs.ctx.is_on_screen(),
            "a presented window should take its reply"
        );

        logs.window.close();
        assert!(
            !logs.ctx.is_on_screen(),
            "a reply arriving after the reader closed the window must not be \
             parsed and rendered into it"
        );

        logs.window.destroy();
    });
}

#[test]
#[ignore = "requires GTK display"]
fn window_is_released_once_closed() {
    run_gtk_test("window_is_released_once_closed", || {
        // The header buttons and the sidebar own the shared state, and that
        // state reaches back to the window and to those same buttons. Holding
        // either one strongly would close the loop and leak the whole window,
        // logs and all, every time the reader opens one.
        let mut weaks: Vec<(String, glib::WeakRef<gtk::Widget>)> = Vec::new();
        let parent = gtk::Window::new();
        let weak = {
            let logs = JobLogsWindow::build(
                &parent,
                repo_stub(),
                "CI • main #1".to_string(),
                vec![
                    job_stub("build", Some("success")),
                    job_stub("test", Some("failure")),
                ],
                1,
                Arc::new(Mutex::new(GitHubGateway::demo())),
            );
            let window = logs.window.clone();
            crate::ui::test_helpers::collect_widget_weaks(
                &window.clone().upcast::<gtk::Widget>(),
                &mut weaks,
            );
            window.destroy();
            window.downgrade()
        };

        while glib::MainContext::default().pending() {
            let _ = glib::MainContext::default().iteration(false);
        }

        assert!(
            weak.upgrade().is_none(),
            "logs window outlived its last strong reference — a handler is \
             holding it in a reference cycle"
        );

        let survivors: Vec<&str> = weaks
            .iter()
            .filter(|(_, weak)| weak.upgrade().is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            survivors.is_empty(),
            "widgets outlived the closed logs window: {survivors:?} — a signal \
             handler is holding them in a reference cycle"
        );
    });
}

#[test]
#[ignore = "requires GTK display"]
fn log_viewer_stays_left_to_right_in_rtl_languages() {
    run_gtk_test("log_viewer_stays_left_to_right_in_rtl_languages", || {
        // An Arabic or Urdu session runs the whole UI with the RTL default
        // direction; the log itself is machine output and must not follow it.
        // The guard restores the default however this test ends: the GTK
        // worker is shared, so a panic here would leave every later UI test
        // running right-to-left.
        struct DirectionGuard(gtk::TextDirection);

        impl Drop for DirectionGuard {
            fn drop(&mut self) {
                gtk::Widget::set_default_direction(self.0);
            }
        }

        let _guard = DirectionGuard(gtk::Widget::default_direction());
        gtk::Widget::set_default_direction(gtk::TextDirection::Rtl);
        let parent = gtk::Window::new();
        let logs = JobLogsWindow::build(
            &parent,
            repo_stub(),
            "CI • main #1".to_string(),
            vec![job_stub("build", Some("success"))],
            0,
            Arc::new(Mutex::new(GitHubGateway::demo())),
        );
        assert_eq!(
            logs.ctx.text_view.direction(),
            gtk::TextDirection::Ltr,
            "log output is code: it must read left-to-right even when the UI is RTL"
        );
        logs.window.destroy();
    });
}

#[test]
#[ignore = "requires GTK display"]
fn save_dialog_prefills_default_name() {
    run_gtk_test("save_dialog_prefills_default_name", || {
        let default_name = JobLogsWindow::default_file_name("CI", "build");
        let dialog = JobLogsWindow::build_save_dialog(&default_name);

        assert_eq!(
            dialog.initial_name().as_deref(),
            Some(default_name.as_str())
        );
        assert_eq!(dialog.title().as_str(), tr("Save Logs").as_str());
        assert_eq!(dialog.accept_label().as_deref(), Some(tr("Save").as_str()));
    });
}
