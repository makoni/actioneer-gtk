use crate::api::models::{Job, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::utils::channel::MainContextChannelExt;
use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{error, info, warn};

mod render;
use render::render_ansi_logs;

#[derive(Clone)]
pub struct JobLogsWindow {
    window: adw::Window,
    ctx: Rc<Ctx>,
}

/// Everything the header buttons and the job sidebar act on.
///
/// It holds the window and its own buttons weakly on purpose: those widgets
/// carry the handlers that own this `Rc`, so a strong link back would form a
/// cycle and keep the whole window alive long after it is closed.
struct Ctx {
    repo: Repo,
    /// Every job of the run, so the sidebar can offer them all. A run has no log
    /// of its own — it is exactly this set of job logs — so "the run's logs"
    /// can only be shown as a list to pick from.
    jobs: Vec<Job>,
    selected: Cell<usize>,
    client: Arc<Mutex<GitHubClient>>,
    window: glib::WeakRef<adw::Window>,
    text_view: gtk::TextView,
    /// Weak like the window: the overlay is the root of the content, so every
    /// button whose handler owns this `Ctx` hangs below it.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    run_title: String,
    job_label: gtk::Label,
    copy_button: glib::WeakRef<gtk::Button>,
    save_button: glib::WeakRef<gtk::Button>,
    cache: LogCache,
}

/// Logs already fetched while this window has been open, keyed by job id.
///
/// Stepping back and forth between jobs is the normal way to read a run, and
/// GitHub hands out a fresh download URL per request, so re-fetching a log the
/// reader has already seen costs a round trip and an API call for nothing.
#[derive(Default)]
struct LogCache(RefCell<HashMap<i64, String>>);

impl LogCache {
    fn get(&self, job_id: i64) -> Option<String> {
        self.0.borrow().get(&job_id).cloned()
    }

    /// Keeps the log only if the job has finished: a running job is still
    /// writing, so a cached copy would freeze it at whatever the first read
    /// happened to catch.
    fn store(&self, job: &Job, logs: &str) {
        if job.conclusion.is_some() {
            self.0.borrow_mut().insert(job.id, logs.to_string());
        }
    }
}

struct WindowParts {
    refresh: gtk::Button,
    copy: gtk::Button,
    save: gtk::Button,
    job_label: gtk::Label,
    job_list: gtk::ListBox,
}

/// Picks the job a reader most likely wants: the one that failed, else the one
/// still running, else the first. Used when the caller names a run rather than
/// a specific job.
pub fn most_relevant_job(jobs: &[Job]) -> Option<usize> {
    if jobs.is_empty() {
        return None;
    }
    let failed = jobs.iter().position(|job| {
        matches!(job.conclusion.as_deref(), Some(c) if c != "success" && c != "skipped" && c != "neutral")
    });
    Some(
        failed
            .or_else(|| jobs.iter().position(|job| job.conclusion.is_none()))
            .unwrap_or(0),
    )
}

impl JobLogsWindow {
    /// Opens the logs for a whole run: the sidebar lists every job and the
    /// viewer shows the selected one. `selected` indexes into `jobs`.
    pub fn for_run(
        parent: &impl IsA<gtk::Window>,
        repo: Repo,
        run_title: String,
        jobs: Vec<Job>,
        selected: usize,
        client: Arc<Mutex<GitHubClient>>,
    ) -> Self {
        let logs_window = Self::build(parent, repo, run_title, jobs, selected, client);
        logs_window.ctx.load_logs(false);
        logs_window
    }

    /// Assembles the window without touching the network, so tests can build a
    /// complete one and watch it go away again.
    fn build(
        parent: &impl IsA<gtk::Window>,
        repo: Repo,
        run_title: String,
        jobs: Vec<Job>,
        selected: usize,
        client: Arc<Mutex<GitHubClient>>,
    ) -> Self {
        assert!(!jobs.is_empty(), "JobLogsWindow requires at least one job");
        let selected = selected.min(jobs.len() - 1);

        let window = adw::Window::builder()
            .title(tr("{title} - Logs").replace("{title}", run_title.as_str()))
            .modal(false)
            .default_width(if jobs.len() > 1 { 1180 } else { 1000 })
            .default_height(700)
            .transient_for(parent)
            .build();
        if let Some(app) = parent.application() {
            window.set_application(Some(&app));
        }
        let text_view = gtk::TextView::builder()
            .editable(false)
            .monospace(true)
            .left_margin(12)
            .right_margin(12)
            .top_margin(12)
            .bottom_margin(12)
            .wrap_mode(gtk::WrapMode::Word)
            .build();
        text_view.buffer().set_text(tr("Fetching logs...").as_str());

        let toast_overlay = adw::ToastOverlay::new();
        let parts = Self::build_ui(
            &window,
            &toast_overlay,
            &text_view,
            &job_display_name(&jobs[selected]),
            &repo.full_name,
            &jobs,
            selected,
        );

        let ctx = Rc::new(Ctx {
            repo,
            jobs,
            selected: Cell::new(selected),
            client,
            window: window.downgrade(),
            text_view,
            toast_overlay: toast_overlay.downgrade(),
            run_title,
            job_label: parts.job_label.clone(),
            copy_button: parts.copy.downgrade(),
            save_button: parts.save.downgrade(),
            cache: LogCache::default(),
        });

        ctx.connect_refresh_button(&parts.refresh);
        ctx.connect_copy_button(&parts.copy);
        ctx.connect_save_button(&parts.save);
        ctx.connect_job_list(&parts.job_list);

        Self { window, ctx }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_ui(
        window: &adw::Window,
        toast_overlay: &adw::ToastOverlay,
        text_view: &gtk::TextView,
        job_title: &str,
        repo_full_name: &str,
        jobs: &[Job],
        selected: usize,
    ) -> WindowParts {
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let header = adw::HeaderBar::new();
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        crate::ui::utils::describe_control(&refresh_button, tr("Refresh logs").as_str());
        header.pack_start(&refresh_button);

        let copy_button = gtk::Button::from_icon_name("edit-copy-symbolic");
        crate::ui::utils::describe_control(&copy_button, tr("Copy logs to clipboard").as_str());
        header.pack_end(&copy_button);

        let save_button = gtk::Button::from_icon_name("document-save-symbolic");
        crate::ui::utils::describe_control(&save_button, tr("Save logs to file").as_str());
        header.pack_end(&save_button);
        main_box.append(&header);

        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        info_box.set_margin_top(12);
        info_box.set_margin_bottom(12);
        info_box.set_margin_start(12);
        info_box.set_margin_end(12);

        let job_label = gtk::Label::new(Some(job_title));
        job_label.add_css_class("title-2");
        job_label.set_halign(gtk::Align::Start);
        job_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info_box.append(&job_label);
        let repo_label = gtk::Label::new(Some(repo_full_name));
        repo_label.add_css_class("dim-label");
        repo_label.set_halign(gtk::Align::Start);
        repo_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info_box.append(&repo_label);

        main_box.append(&info_box);
        main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(text_view));

        let job_list = build_job_list(jobs, selected);

        if jobs.len() > 1 {
            // A run's "logs" are the logs of its jobs, so the reader picks one
            // instead of the caller guessing. With a single job there is nothing
            // to pick, and the sidebar would only take space.
            let sidebar_scroller = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .child(&job_list)
                .build();
            sidebar_scroller.set_size_request(220, -1);

            let split = gtk::Paned::builder()
                .orientation(gtk::Orientation::Horizontal)
                .position(240)
                .shrink_start_child(false)
                .resize_start_child(false)
                .start_child(&sidebar_scroller)
                .end_child(&scrolled)
                .vexpand(true)
                .build();
            main_box.append(&split);
        } else {
            main_box.append(&scrolled);
        }

        toast_overlay.set_child(Some(&main_box));
        window.set_content(Some(toast_overlay));

        WindowParts {
            refresh: refresh_button,
            copy: copy_button,
            save: save_button,
            job_label,
            job_list,
        }
    }

    fn default_file_name(run_title: &str, job_title: &str) -> String {
        let run_segment =
            Self::sanitize_filename_segment(run_title).unwrap_or_else(|| "run".to_string());
        let job_segment =
            Self::sanitize_filename_segment(job_title).unwrap_or_else(|| "job".to_string());
        format!("{} - {}.log", run_segment, job_segment)
    }

    /// Builds the "Save Logs" file dialog pre-filled with the default file name.
    fn build_save_dialog(default_name: &str) -> gtk::FileDialog {
        gtk::FileDialog::builder()
            .title(tr("Save Logs"))
            .accept_label(tr("Save"))
            .modal(true)
            .initial_name(default_name)
            .build()
    }

    fn sanitize_filename_segment(input: &str) -> Option<String> {
        let filtered: String = input
            .chars()
            .map(|ch| match ch {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c if c.is_control() => '_',
                _ => ch,
            })
            .collect();

        let cleaned = filtered.trim().trim_matches('.').trim().to_string();
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

impl Ctx {
    fn job(&self) -> &Job {
        &self.jobs[self.selected.get()]
    }

    /// Swaps the viewer to another job of the same run.
    fn show_job(self: &Rc<Self>, index: usize) {
        if index >= self.jobs.len() || index == self.selected.get() {
            return;
        }

        self.selected.set(index);
        self.job_label.set_text(&job_display_name(self.job()));
        self.load_logs(false);
    }

    /// `force` skips the cache: Refresh has to reach GitHub even for a log that
    /// is already on screen, which is the whole point of pressing it.
    fn load_logs(self: &Rc<Self>, force: bool) {
        let job_id = self.job().id;

        if !force && let Some(cached) = self.cache.get(job_id) {
            self.render_logs(&cached);
            return;
        }

        self.text_view
            .buffer()
            .set_text(tr("Fetching logs...").as_str());
        self.set_copy_save_enabled(false);

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<String, GitHubError>>(glib::Priority::default());

        let this = self.clone();
        receiver.attach(None, move |result| {
            // The reader may have moved on while this was in flight. A stale
            // reply must not paint over the job now on screen — but it is still
            // worth keeping, which is exactly what the cache is for.
            let still_showing = this.job().id == job_id && this.is_on_screen();
            match result {
                Ok(logs) => {
                    info!("Loaded logs ({} bytes)", logs.len());
                    if let Some(job) = this.jobs.iter().find(|job| job.id == job_id) {
                        this.cache.store(job, &logs);
                    }
                    if still_showing {
                        this.render_logs(&logs);
                    }
                }
                Err(error) => {
                    if still_showing {
                        this.render_error(&error);
                    }
                }
            }
            glib::ControlFlow::Break
        });

        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        crate::runtime_handle().spawn(async move {
            let client = client.lock().clone();
            let result = client.get_job_logs(&owner, &repo_name, job_id).await;
            let _ = sender.send(result);
        });
    }

    /// Whether rendering would be seen. A reply can arrive seconds after the
    /// reader closed the window, and a run's log runs to megabytes: parsing its
    /// ANSI into a buffer nobody will look at is pure waste.
    fn is_on_screen(&self) -> bool {
        self.window
            .upgrade()
            .is_some_and(|window| window.is_visible())
    }

    fn render_logs(&self, logs: &str) {
        self.text_view.set_sensitive(true);
        render_ansi_logs(&self.text_view, logs);
        self.set_copy_save_enabled(true);
    }

    fn render_error(&self, error: &GitHubError) {
        let message = match error {
            GitHubError::NotFound => {
                warn!("Job logs unavailable; job may still be running");
                tr(
                    "Logs are not yet available for this job. GitHub only provides logs once the job starts streaming output or completes. Try refreshing in a few moments.",
                )
            }
            GitHubError::Gone => {
                warn!("Job logs expired (410 Gone)");
                tr("The logs for this run have expired and are no longer available.")
            }
            other => {
                error!("Failed to load logs: {}", other);
                tr("Unable to load logs right now. Please try again later.\n\nDetails: {error}")
                    .replace("{error}", other.to_string().as_str())
            }
        };

        self.text_view.set_sensitive(false);
        self.text_view.buffer().set_text(&message);
        self.set_copy_save_enabled(false);
    }

    fn set_copy_save_enabled(&self, enabled: bool) {
        if let Some(button) = self.copy_button.upgrade() {
            button.set_sensitive(enabled);
        }
        if let Some(button) = self.save_button.upgrade() {
            button.set_sensitive(enabled);
        }
    }

    fn connect_job_list(self: &Rc<Self>, job_list: &gtk::ListBox) {
        let this = self.clone();
        job_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                this.show_job(row.index().max(0) as usize);
            }
        });
    }

    fn connect_refresh_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.load_logs(true));
    }

    fn connect_copy_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.copy_logs());
    }

    fn copy_logs(&self) {
        let text = self.logs_text();

        if text.is_empty() {
            self.toast(tr("Logs are empty; nothing to copy").as_str(), 3);
            return;
        }

        match gdk::Display::default() {
            Some(display) => {
                display.clipboard().set_text(&text);
                self.toast(tr("Logs copied to clipboard").as_str(), 3);
            }
            None => {
                error!("No display available for clipboard access");
                self.toast(tr("Unable to access the clipboard").as_str(), 5);
            }
        }
    }

    fn connect_save_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.save_logs());
    }

    fn save_logs(&self) {
        let default_name =
            JobLogsWindow::default_file_name(&self.run_title, &job_display_name(self.job()));
        let dialog = JobLogsWindow::build_save_dialog(&default_name);

        let Some(overlay) = self.toast_overlay.upgrade() else {
            return;
        };
        let text = self.logs_text();
        let window = self.window.upgrade();

        dialog.save(window.as_ref(), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else {
                return;
            };

            if text.is_empty() {
                add_toast(&overlay, tr("Logs are empty; nothing saved").as_str(), 3);
                return;
            }

            let Some(path) = file.path() else {
                add_toast(
                    &overlay,
                    tr("Unable to determine save location").as_str(),
                    5,
                );
                return;
            };

            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<(), String>>(glib::Priority::default());
            let overlay_for_result = overlay.clone();

            receiver.attach(None, move |message| {
                match message {
                    Ok(()) => add_toast(&overlay_for_result, tr("Logs saved").as_str(), 3),
                    Err(err) => add_toast(
                        &overlay_for_result,
                        tr("Failed to save logs: {error}")
                            .replace("{error}", err.as_str())
                            .as_str(),
                        5,
                    ),
                }
                glib::ControlFlow::Break
            });

            let text_to_write = text.clone();
            crate::runtime_handle().spawn_blocking(move || {
                let result = std::fs::write(&path, text_to_write);
                let _ = sender.send(result.map_err(|e| e.to_string()));
            });
        });
    }

    fn logs_text(&self) -> String {
        let buffer = self.text_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    }

    fn toast(&self, message: &str, timeout: u32) {
        if let Some(overlay) = self.toast_overlay.upgrade() {
            add_toast(&overlay, message, timeout);
        }
    }
}

fn add_toast(overlay: &adw::ToastOverlay, message: &str, timeout: u32) {
    let toast = adw::Toast::new(message);
    toast.set_timeout(timeout);
    overlay.add_toast(toast);
}

fn job_display_name(job: &Job) -> String {
    job.name
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| tr("Job"))
}

/// The run's jobs, in order, each with its status dot and duration.
fn build_job_list(jobs: &[Job], selected: usize) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.add_css_class("job-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);

    for job in jobs {
        let row = gtk::ListBoxRow::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.set_margin_start(6);
        content.set_margin_end(6);

        let dot = crate::ui::detail_view::build_job_status_dot(job);
        content.append(&dot);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);

        let name = gtk::Label::new(Some(&job_display_name(job)));
        name.set_halign(gtk::Align::Start);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&name);

        if let Some(duration) = job.duration_string() {
            let duration_label = gtk::Label::new(Some(&duration));
            duration_label.add_css_class("dim-label");
            duration_label.add_css_class("caption");
            duration_label.add_css_class("mono");
            duration_label.set_halign(gtk::Align::Start);
            text.append(&duration_label);
        }

        content.append(&text);
        row.set_child(Some(&content));
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(selected as i32) {
        list.select_row(Some(&row));
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Job;
    use crate::ui::test_helpers::run_gtk_test;

    fn repo_stub() -> Repo {
        Repo {
            id: 1,
            name: "repo".into(),
            full_name: "owner/repo".into(),
            owner: crate::api::models::User {
                login: "owner".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        }
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
                Arc::new(Mutex::new(
                    GitHubClient::new(None).expect("client stub should build"),
                )),
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
                    Arc::new(Mutex::new(
                        GitHubClient::new(None).expect("client stub should build"),
                    )),
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
}
