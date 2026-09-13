use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::GitHubError;
use crate::services::api::models::{Job, Repo};
use crate::services::gateway::GitHubGateway;
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
    /// can only be shown as a list to pick from. Refresh replaces the snapshot
    /// with fresh data; `selected` tracks the reader's place by job id.
    jobs: Rc<RefCell<Vec<Job>>>,
    selected: Cell<usize>,
    client: Arc<Mutex<GitHubGateway>>,
    window: glib::WeakRef<adw::Window>,
    text_view: gtk::TextView,
    /// Weak like the window: the overlay is the root of the content, so every
    /// button whose handler owns this `Ctx` hangs below it.
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    run_title: String,
    /// Addressed by it for the job re-fetch. A property of the run, not of
    /// whatever row happens to be first in the snapshot.
    run_id: i64,
    job_label: gtk::Label,
    /// Weak, for the same reason: the sidebar's row-selected handler owns this
    /// `Rc`, so a strong link would close a cycle that outlives the window.
    job_list: glib::WeakRef<gtk::ListBox>,
    /// Set while the sidebar is being rebuilt, so the row-selected signals the
    /// rebuild itself fires are not mistaken for the reader's choice.
    rebuilding: Cell<bool>,
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
        client: Arc<Mutex<GitHubGateway>>,
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
        client: Arc<Mutex<GitHubGateway>>,
    ) -> Self {
        assert!(!jobs.is_empty(), "JobLogsWindow requires at least one job");
        let selected = selected.min(jobs.len() - 1);
        let run_id = jobs[0].run_id;

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
        text_view.set_direction(gtk::TextDirection::Ltr);
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
            jobs: Rc::new(RefCell::new(jobs)),
            selected: Cell::new(selected),
            client,
            window: window.downgrade(),
            text_view,
            toast_overlay: toast_overlay.downgrade(),
            run_title,
            run_id,
            job_label: parts.job_label.clone(),
            job_list: parts.job_list.downgrade(),
            rebuilding: Cell::new(false),
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
    fn selected_job_id(&self) -> i64 {
        self.jobs.borrow()[self.selected.get()].id
    }

    fn selected_job_name(&self) -> String {
        job_display_name(&self.jobs.borrow()[self.selected.get()])
    }

    /// Swaps the viewer to another job of the same run.
    fn show_job(self: &Rc<Self>, index: usize) {
        // A rebuild fires row-selected too; that is the refresh, not the reader.
        if self.rebuilding.get() {
            return;
        }
        if index >= self.jobs.borrow().len() || index == self.selected.get() {
            return;
        }

        self.selected.set(index);
        self.job_label
            .set_text(&job_display_name(&self.jobs.borrow()[index]));
        self.load_logs(false);
    }

    /// `force` skips the cache: Refresh has to reach GitHub even for a log that
    /// is already on screen, which is the whole point of pressing it.
    fn load_logs(self: &Rc<Self>, force: bool) {
        let job_id = self.selected_job_id();

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
            let still_showing = this.selected_job_id() == job_id && this.is_on_screen();
            match result {
                Ok(logs) => {
                    info!("Loaded logs ({} bytes)", logs.len());
                    if let Some(job) = this.jobs.borrow().iter().find(|job| job.id == job_id) {
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
        crate::runtime::handle().spawn(async move {
            let client = client.lock().clone();
            let result = client.get_job_logs(&owner, &repo_name, job_id).await;
            let _ = sender.send(result);
        });
    }

    /// Refresh re-fetches both the run's jobs and the current log. The sidebar
    /// is a snapshot taken when the window opened, and without the job re-fetch
    /// a finished job's duration would keep counting and its dot would stay
    /// "in progress" for as long as the window stays open.
    fn refresh(self: &Rc<Self>) {
        self.fetch_jobs();
        self.load_logs(true);
    }

    /// Re-fetches the run's jobs so the sidebar learns what happened since the
    /// window opened. Best effort: on a failure the reader simply stays on the
    /// old snapshot, and the log fetch reports its own errors.
    fn fetch_jobs(self: &Rc<Self>) {
        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Vec<Job>, GitHubError>>(glib::Priority::default());

        let this = self.clone();
        receiver.attach(None, move |result| {
            if let Ok(jobs) = result {
                this.apply_jobs(jobs);
            }
            glib::ControlFlow::Break
        });

        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let run_id = self.run_id;
        crate::runtime::handle().spawn(async move {
            let client = client.lock().clone();
            let result = client.list_jobs(&owner, &repo_name, run_id).await;
            let _ = sender.send(result);
        });
    }

    /// Replaces the sidebar's snapshot with fresh jobs, keeping the reader on
    /// the job they are looking at (by id: a retry can reorder the run). The
    /// rebuilt rows carry the fresh durations, and each rebuilt row's ticker —
    /// if any — dies with the old label it was counting on.
    ///
    /// An empty answer is a legitimate one for a run that has no jobs left or
    /// none yet: like a network failure, it leaves the reader on the old
    /// snapshot.
    fn apply_jobs(self: &Rc<Self>, jobs: Vec<Job>) {
        if jobs.is_empty() || !self.is_on_screen() {
            return;
        }

        let current_id = self.selected_job_id();
        let found = jobs.iter().position(|job| job.id == current_id);
        let selected = found.unwrap_or(0);
        self.selected.set(selected);
        *self.jobs.borrow_mut() = jobs;

        let Some(list) = self.job_list.upgrade() else {
            return;
        };
        self.rebuilding.replace(true);
        let mut child = list.first_child();
        while let Some(row) = child {
            let next = row.next_sibling();
            list.remove(&row);
            child = next;
        }
        for job in self.jobs.borrow().iter() {
            append_job_row(&list, job);
        }
        if let Some(row) = list.row_at_index(selected as i32) {
            list.select_row(Some(&row));
        }
        self.rebuilding.replace(false);

        if found.is_none() {
            // The reader's job left the run. The rebuild guard blocked
            // show_job, so without this the title and log would keep
            // describing a job the sidebar no longer has.
            self.job_label
                .set_text(&job_display_name(&self.jobs.borrow()[selected]));
            self.load_logs(false);
        }
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
        button.connect_clicked(move |_| this.refresh());
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
        let name = self.selected_job_name();
        let default_name = JobLogsWindow::default_file_name(&self.run_title, &name);
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
            crate::runtime::handle().spawn_blocking(move || {
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
        append_job_row(&list, job);
    }

    if let Some(row) = list.row_at_index(selected as i32) {
        list.select_row(Some(&row));
    }

    list
}

fn append_job_row(list: &gtk::ListBox, job: &Job) {
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

    let (duration, live) = crate::ui::utils::duration::job_duration_label(job);
    if let Some(duration) = duration {
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.add_css_class("dim-label");
        duration_label.add_css_class("caption");
        duration_label.add_css_class("mono");
        duration_label.set_halign(gtk::Align::Start);
        if let Some(started_at) = live {
            crate::ui::utils::duration::start_live_duration(&duration_label, started_at);
        }
        text.append(&duration_label);
    }

    content.append(&text);
    row.set_child(Some(&content));
    list.append(&row);
}

#[cfg(test)]
mod tests;
