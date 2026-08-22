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
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{error, info, warn};

mod render;
use render::render_ansi_logs;

#[derive(Clone)]
pub struct JobLogsWindow {
    window: adw::Window,
    repo: Repo,
    /// Every job of the run, so the sidebar can offer them all. A run has no log
    /// of its own — it is exactly this set of job logs — so "the run's logs"
    /// can only be shown as a list to pick from.
    jobs: Rc<Vec<Job>>,
    job: Rc<RefCell<Job>>,
    client: Arc<Mutex<GitHubClient>>,
    text_view: gtk::TextView,
    toast_overlay: adw::ToastOverlay,
    run_title: String,
    job_label: gtk::Label,
    copy_button: gtk::Button,
    save_button: gtk::Button,
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
        let jobs = Rc::new(jobs);
        let selected = selected.min(jobs.len().saturating_sub(1));
        let job = jobs
            .get(selected)
            .cloned()
            .unwrap_or_else(|| panic!("JobLogsWindow requires at least one job"));

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
            &job_display_name(&job),
            &repo.full_name,
            &jobs,
            selected,
        );

        let logs_window = Self {
            window: window.clone(),
            repo: repo.clone(),
            jobs: jobs.clone(),
            job: Rc::new(RefCell::new(job)),
            client: client.clone(),
            text_view: text_view.clone(),
            toast_overlay: toast_overlay.clone(),
            run_title,
            job_label: parts.job_label.clone(),
            copy_button: parts.copy.clone(),
            save_button: parts.save.clone(),
        };

        logs_window.connect_refresh_button(&parts.refresh);
        logs_window.connect_copy_button(&parts.copy);
        logs_window.connect_save_button(&parts.save);
        logs_window.connect_job_list(&parts.job_list);
        logs_window.load_logs();

        logs_window
    }

    /// Swaps the viewer to another job of the same run.
    fn show_job(&self, index: usize) {
        let Some(job) = self.jobs.get(index).cloned() else {
            return;
        };
        if job.id == self.job.borrow().id {
            return;
        }

        self.job_label.set_text(&job_display_name(&job));
        *self.job.borrow_mut() = job;
        self.text_view
            .buffer()
            .set_text(tr("Fetching logs...").as_str());
        self.load_logs();
    }

    fn connect_job_list(&self, job_list: &gtk::ListBox) {
        let this = self.clone();
        job_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                this.show_job(row.index().max(0) as usize);
            }
        });
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

    fn load_logs(&self) {
        self.set_copy_save_enabled(false);

        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let job_id = self.job.borrow().id;
        let text_view = self.text_view.clone();
        let copy_button = self.copy_button.clone();
        let save_button = self.save_button.clone();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<String, GitHubError>>(glib::Priority::default());

        receiver.attach(None, move |result| {
            match result {
                Ok(logs) => {
                    info!("Loaded logs ({} bytes)", logs.len());
                    text_view.set_sensitive(true);
                    render_ansi_logs(&text_view, &logs);
                    copy_button.set_sensitive(true);
                    save_button.set_sensitive(true);
                }
                Err(GitHubError::NotFound) => {
                    warn!("Job logs unavailable; job may still be running");
                    text_view.set_sensitive(false);
                    text_view.buffer().set_text(tr("Logs are not yet available for this job. GitHub only provides logs once the job starts streaming output or completes. Try refreshing in a few moments.").as_str());
                    copy_button.set_sensitive(false);
                    save_button.set_sensitive(false);
                }
                Err(GitHubError::Gone) => {
                    warn!("Job logs expired (410 Gone)");
                    text_view.set_sensitive(false);
                    text_view.buffer().set_text(
                        tr("The logs for this run have expired and are no longer available.")
                            .as_str(),
                    );
                    copy_button.set_sensitive(false);
                    save_button.set_sensitive(false);
                }
                Err(e) => {
                    error!("Failed to load logs: {}", e);
                    text_view.set_sensitive(false);
                    text_view.buffer().set_text(
                        tr("Unable to load logs right now. Please try again later.\n\nDetails: {error}")
                            .replace("{error}", e.to_string().as_str())
                            .as_str(),
                    );
                    copy_button.set_sensitive(false);
                    save_button.set_sensitive(false);
                }
            }
            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_clone = client.lock().clone();
            let result = client_clone.get_job_logs(&owner, &repo_name, job_id).await;
            let _ = sender.send(result);
        });
    }

    fn connect_refresh_button(&self, button: &gtk::Button) {
        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let job_id = self.job.borrow().id;
        let text_view = self.text_view.clone();
        let copy_button = self.copy_button.clone();
        let save_button = self.save_button.clone();

        button.connect_clicked(move |_| {
            let client = client.clone();
            let owner = owner.clone();
            let repo_name = repo_name.clone();
            let tv = text_view.clone();
            copy_button.set_sensitive(false);
            save_button.set_sensitive(false);

            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<String, GitHubError>>(glib::Priority::default());
            let tv_for_ui = tv.clone();
            let copy_for_result = copy_button.clone();
            let save_for_result = save_button.clone();

            receiver.attach(None, move |result| {
                match result {
                    Ok(logs) => {
                        info!("Refreshed logs ({} bytes)", logs.len());
                        tv_for_ui.set_sensitive(true);
                        render_ansi_logs(&tv_for_ui, &logs);
                        copy_for_result.set_sensitive(true);
                        save_for_result.set_sensitive(true);
                    }
                    Err(GitHubError::NotFound) => {
                        warn!("Job logs still unavailable during refresh");
                        tv_for_ui.set_sensitive(false);
                        tv_for_ui.buffer().set_text(tr("Logs are not yet available for this job. GitHub only provides logs once the job starts streaming output or completes. Try refreshing in a few moments.").as_str());
                        copy_for_result.set_sensitive(false);
                        save_for_result.set_sensitive(false);
                    }
                    Err(GitHubError::Gone) => {
                        warn!("Job logs expired during refresh (410 Gone)");
                        tv_for_ui.set_sensitive(false);
                        tv_for_ui.buffer().set_text(
                            tr("The logs for this run have expired and are no longer available.")
                                .as_str(),
                        );
                        copy_for_result.set_sensitive(false);
                        save_for_result.set_sensitive(false);
                    }
                    Err(e) => {
                        error!("Failed to refresh logs: {}", e);
                        tv_for_ui.set_sensitive(false);
                        tv_for_ui.buffer().set_text(
                            tr("Unable to load logs right now. Please try again later.\n\nDetails: {error}")
                                .replace("{error}", e.to_string().as_str())
                                .as_str(),
                        );
                        copy_for_result.set_sensitive(false);
                        save_for_result.set_sensitive(false);
                    }
                }

                glib::ControlFlow::Break
            });
            crate::runtime_handle().spawn(async move {
                let client_clone = client.lock().clone();
                let result = client_clone.get_job_logs(&owner, &repo_name, job_id).await;
                let _ = sender.send(result);
            });
        });
    }

    fn set_copy_save_enabled(&self, enabled: bool) {
        self.copy_button.set_sensitive(enabled);
        self.save_button.set_sensitive(enabled);
    }

    fn connect_copy_button(&self, button: &gtk::Button) {
        let text_view = self.text_view.clone();
        let overlay = self.toast_overlay.clone();

        button.connect_clicked(move |_| {
            let buffer = text_view.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();

            if text.is_empty() {
                let toast = adw::Toast::new(tr("Logs are empty; nothing to copy").as_str());
                toast.set_timeout(3);
                overlay.add_toast(toast);
                return;
            }

            let display = gdk::Display::default();
            match display {
                Some(display) => {
                    let clipboard = display.clipboard();
                    clipboard.set_text(&text);
                    let toast = adw::Toast::new(tr("Logs copied to clipboard").as_str());
                    toast.set_timeout(3);
                    overlay.add_toast(toast);
                }
                None => {
                    let toast =
                        adw::Toast::new(tr("Clipboard unavailable on this system").as_str());
                    toast.set_timeout(5);
                    overlay.add_toast(toast);
                }
            }
        });
    }

    fn connect_save_button(&self, button: &gtk::Button) {
        let window = self.window.clone();
        let text_view = self.text_view.clone();
        let overlay = self.toast_overlay.clone();
        let run_title = self.run_title.clone();
        let job_title = job_display_name(&self.job.borrow());

        button.connect_clicked(move |_| {
            let default_name = JobLogsWindow::default_file_name(&run_title, &job_title);
            let dialog = JobLogsWindow::build_save_dialog(&default_name);

            let overlay_for_dialog = overlay.clone();
            let text_for_dialog = text_view.clone();

            dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
                let file = match result {
                    Ok(file) => file,
                    Err(_) => return,
                };

                let buffer = text_for_dialog.buffer();
                let text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();

                if text.is_empty() {
                    let toast = adw::Toast::new(tr("Logs are empty; nothing saved").as_str());
                    toast.set_timeout(3);
                    overlay_for_dialog.add_toast(toast);
                    return;
                }

                if let Some(path) = file.path() {
                    let text_to_write = text.clone();
                    let (sender, receiver) = glib::MainContext::default()
                        .channel::<Result<(), String>>(glib::Priority::default());
                    let overlay_for_result = overlay_for_dialog.clone();

                    receiver.attach(None, move |message| {
                        match message {
                            Ok(()) => {
                                let toast = adw::Toast::new(tr("Logs saved").as_str());
                                toast.set_timeout(3);
                                overlay_for_result.add_toast(toast);
                            }
                            Err(err) => {
                                let toast = adw::Toast::new(
                                    tr("Failed to save logs: {error}")
                                        .replace("{error}", err.as_str())
                                        .as_str(),
                                );
                                toast.set_timeout(5);
                                overlay_for_result.add_toast(toast);
                            }
                        }
                        glib::ControlFlow::Break
                    });

                    crate::runtime_handle().spawn_blocking(move || {
                        let result = std::fs::write(&path, text_to_write);
                        let _ = sender.send(result.map_err(|e| e.to_string()));
                    });
                } else {
                    let toast = adw::Toast::new(tr("Unable to determine save location").as_str());
                    toast.set_timeout(5);
                    overlay_for_dialog.add_toast(toast);
                }
            });
        });
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
