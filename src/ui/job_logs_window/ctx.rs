//! The job-logs window's shared context and everything it drives.
//!
//! Split out of `job_logs_window.rs`. `Ctx` is the bundle of widgets and state
//! every handler in that window closes over; its impl block was 322 lines, and
//! it is a separate concern from building the window.

use super::*;

impl Ctx {
    pub(super) fn selected_job_id(&self) -> i64 {
        self.jobs.borrow()[self.selected.get()].id
    }

    pub(super) fn selected_job_name(&self) -> String {
        job_display_name(&self.jobs.borrow()[self.selected.get()])
    }

    /// Swaps the viewer to another job of the same run.
    pub(super) fn show_job(self: &Rc<Self>, index: usize) {
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
    pub(super) fn load_logs(self: &Rc<Self>, force: bool) {
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
    pub(super) fn refresh(self: &Rc<Self>) {
        self.fetch_jobs();
        self.load_logs(true);
    }

    /// Re-fetches the run's jobs so the sidebar learns what happened since the
    /// window opened. Best effort: on a failure the reader simply stays on the
    /// old snapshot, and the log fetch reports its own errors.
    pub(super) fn fetch_jobs(self: &Rc<Self>) {
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
    pub(super) fn apply_jobs(self: &Rc<Self>, jobs: Vec<Job>) {
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
    pub(super) fn is_on_screen(&self) -> bool {
        self.window
            .upgrade()
            .is_some_and(|window| window.is_visible())
    }

    pub(super) fn render_logs(&self, logs: &str) {
        self.text_view.set_sensitive(true);
        render_ansi_logs(&self.text_view, logs);
        self.set_copy_save_enabled(true);
    }

    pub(super) fn render_error(&self, error: &GitHubError) {
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

    pub(super) fn set_copy_save_enabled(&self, enabled: bool) {
        if let Some(button) = self.copy_button.upgrade() {
            button.set_sensitive(enabled);
        }
        if let Some(button) = self.save_button.upgrade() {
            button.set_sensitive(enabled);
        }
    }

    pub(super) fn connect_job_list(self: &Rc<Self>, job_list: &gtk::ListBox) {
        let this = self.clone();
        job_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                this.show_job(row.index().max(0) as usize);
            }
        });
    }

    pub(super) fn connect_refresh_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.refresh());
    }

    pub(super) fn connect_copy_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.copy_logs());
    }

    pub(super) fn copy_logs(&self) {
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

    pub(super) fn connect_save_button(self: &Rc<Self>, button: &gtk::Button) {
        let this = self.clone();
        button.connect_clicked(move |_| this.save_logs());
    }

    pub(super) fn save_logs(&self) {
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

    pub(super) fn logs_text(&self) -> String {
        let buffer = self.text_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    }

    pub(super) fn toast(&self, message: &str, timeout: u32) {
        if let Some(overlay) = self.toast_overlay.upgrade() {
            add_toast(&overlay, message, timeout);
        }
    }
}
