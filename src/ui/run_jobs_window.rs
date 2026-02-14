use crate::api::models::{Job, Repo, WorkflowRun};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::{error, info};

fn run_display_title(run: &WorkflowRun) -> String {
    let base = run
        .display_title
        .as_deref()
        .or(run.name.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| tr("Workflow Run"));

    if let Some(number) = run.run_number {
        format!("{} #{}", base, number)
    } else {
        base
    }
}

pub struct RunJobsWindow {
    window: adw::Window,
    repo: Repo,
    run: WorkflowRun,
    client: Arc<Mutex<GitHubClient>>,
    jobs: Arc<Mutex<Vec<Job>>>,
}

impl RunJobsWindow {
    pub fn new(
        parent: &impl IsA<gtk::Window>,
        repo: Repo,
        run: WorkflowRun,
        client: Arc<Mutex<GitHubClient>>,
    ) -> Self {
        let title = run_display_title(&run);

        let window = adw::Window::builder()
            .title(tr("{title} - Jobs").replace("{title}", title.as_str()))
            .modal(false)
            .default_width(900)
            .default_height(700)
            .transient_for(parent)
            .build();
        if let Some(app) = parent.application() {
            window.set_application(Some(&app));
        }

        let jobs = Arc::new(Mutex::new(Vec::new()));

        let jobs_window = Self {
            window: window.clone(),
            repo: repo.clone(),
            run: run.clone(),
            client: client.clone(),
            jobs: jobs.clone(),
        };

        jobs_window.build_ui();
        jobs_window.load_jobs();
        jobs_window
    }

    fn build_ui(&self) {
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Header bar
        let header = adw::HeaderBar::new();

        // Refresh button
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some(tr("Refresh jobs").as_str()));
        header.pack_start(&refresh_button);

        // Cancel run button
        let cancel_button = gtk::Button::from_icon_name("process-stop-symbolic");
        cancel_button.set_tooltip_text(Some(tr("Cancel run").as_str()));
        cancel_button.add_css_class("destructive-action");

        // Only show cancel if run is in progress
        if self.run.status.as_deref() == Some("in_progress")
            || self.run.status.as_deref() == Some("queued")
        {
            header.pack_end(&cancel_button);
        }

        main_box.append(&header);

        // Run info
        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        info_box.set_margin_top(12);
        info_box.set_margin_bottom(12);
        info_box.set_margin_start(12);
        info_box.set_margin_end(12);

        let title = self
            .run
            .display_title
            .as_deref()
            .or(self.run.name.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| tr("Workflow Run"));
        let run_label = gtk::Label::new(Some(title.as_str()));
        run_label.add_css_class("title-2");
        run_label.set_halign(gtk::Align::Start);
        info_box.append(&run_label);

        let repo_label = gtk::Label::new(Some(&self.repo.full_name));
        repo_label.add_css_class("dim-label");
        repo_label.set_halign(gtk::Align::Start);
        info_box.append(&repo_label);

        main_box.append(&info_box);

        // Separator
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        main_box.append(&separator);

        // Jobs list
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.set_margin_top(12);
        list_box.set_margin_bottom(12);
        list_box.set_margin_start(12);
        list_box.set_margin_end(12);

        scrolled.set_child(Some(&list_box));

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(900);
        clamp.set_child(Some(&scrolled));

        main_box.append(&clamp);

        self.window.set_content(Some(&main_box));

        // Connect signals
        self.connect_refresh_button(&refresh_button, &list_box);
        self.connect_cancel_button(&cancel_button);
        self.connect_job_selected(&list_box);
    }

    fn load_jobs(&self) {
        let client = self.client.clone();
        let jobs = self.jobs.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let run_id = self.run.id;
        let window = self.window.clone();

        let (sender, receiver) = glib::MainContext::default()
            .channel::<Result<Vec<Job>, GitHubError>>(glib::Priority::default());

        receiver.attach(None, move |result| {
            match result {
                Ok(jobs_list) => {
                    info!("Loaded {} jobs", jobs_list.len());
                    let jobs_clone = jobs_list.clone();
                    *jobs.lock() = jobs_list;

                    if let Some(content) = window.content() {
                        if let Ok(main_box) = content.downcast::<gtk::Box>() {
                            find_and_update_jobs_list(&main_box, &jobs_clone);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to load jobs: {}", e);
                }
            }

            glib::ControlFlow::Break
        });

        crate::runtime_handle().spawn(async move {
            let client_clone = client.lock().clone();
            let result = client_clone.list_jobs(&owner, &repo_name, run_id).await;
            let _ = sender.send(result);
        });
    }

    fn connect_refresh_button(&self, button: &gtk::Button, list_box: &gtk::ListBox) {
        let client = self.client.clone();
        let jobs = self.jobs.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let run_id = self.run.id;
        let list_box = list_box.clone();

        button.connect_clicked(move |_| {
            let client = client.clone();
            let jobs = jobs.clone();
            let owner = owner.clone();
            let repo_name = repo_name.clone();
            let list = list_box.clone();

            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<Vec<Job>, GitHubError>>(glib::Priority::default());
            let jobs_for_ui = jobs.clone();
            let list_for_ui = list.clone();

            receiver.attach(None, move |result| {
                match result {
                    Ok(jobs_list) => {
                        info!("Refreshed {} jobs", jobs_list.len());
                        let jobs_clone = jobs_list.clone();
                        *jobs_for_ui.lock() = jobs_list;
                        update_jobs_list(&list_for_ui, &jobs_clone);
                    }
                    Err(e) => {
                        error!("Failed to refresh jobs: {}", e);
                    }
                }

                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_clone = client.lock().clone();
                let result = client_clone.list_jobs(&owner, &repo_name, run_id).await;
                let _ = sender.send(result);
            });
        });
    }

    fn connect_cancel_button(&self, button: &gtk::Button) {
        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let run_id = self.run.id;
        let window = self.window.clone();

        button.connect_clicked(move |btn| {
            let client = client.clone();
            let owner = owner.clone();
            let repo_name = repo_name.clone();
            let window = window.clone();
            let button_ref = btn.clone();

            btn.set_sensitive(false);

            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<(), GitHubError>>(glib::Priority::default());

            receiver.attach(None, move |result| {
                match result {
                    Ok(_) => {
                        info!("Run cancelled successfully");

                        let dialog = gtk::MessageDialog::new(
                            Some(&window),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Info,
                            gtk::ButtonsType::Ok,
                            tr("Run cancellation requested successfully.").as_str(),
                        );
                        dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        dialog.present();
                    }
                    Err(e) => {
                        error!("Failed to cancel run: {}", e);
                        button_ref.set_sensitive(true);

                        let dialog = gtk::MessageDialog::new(
                            Some(&window),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Error,
                            gtk::ButtonsType::Ok,
                            tr("Failed to cancel run: {error}")
                                .replace("{error}", e.to_string().as_str()),
                        );
                        dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        dialog.present();
                    }
                }

                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let client_clone = client.lock().clone();
                let result = client_clone.cancel_run(&owner, &repo_name, run_id).await;
                let _ = sender.send(result);
            });
        });
    }

    fn connect_job_selected(&self, list_box: &gtk::ListBox) {
        let window = self.window.clone();
        let client = self.client.clone();
        let jobs = self.jobs.clone();
        let repo = self.repo.clone();
        let run_title = run_display_title(&self.run);

        list_box.connect_row_activated(move |_, row| {
            let index = row.index() as usize;
            let job = {
                let jobs_lock = jobs.lock();
                jobs_lock.get(index).cloned()
            };

            if let Some(job) = job {
                let run_title_for_logs = run_title.clone();
                let logs_window = super::job_logs_window::JobLogsWindow::new(
                    &window,
                    repo.clone(),
                    run_title_for_logs,
                    job,
                    client.clone(),
                );
                logs_window.present();
            }
        });
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn update_jobs_list(list_box: &gtk::ListBox, jobs: &[Job]) {
    // Clear existing items
        loop {
            let child_opt = list_box.first_child();
            let Some(child) = child_opt else {
                break;
            };
            list_box.remove(&child);
        }

    // Add new items
    for job in jobs {
        let row = create_job_row(job);
        list_box.append(&row);
    }
}

fn find_and_update_jobs_list(container: &gtk::Box, jobs: &[Job]) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        if let Ok(clamp) = widget.clone().downcast::<adw::Clamp>() {
            if let Some(scrolled) = clamp.child() {
                if let Ok(sw) = scrolled.downcast::<gtk::ScrolledWindow>() {
                    if let Some(list_box) = sw.child() {
                        if let Ok(lb) = list_box.downcast::<gtk::ListBox>() {
                            update_jobs_list(&lb, jobs);
                            return;
                        }
                    }
                }
            }
        }
        child = widget.next_sibling();
    }
}

fn create_job_row(job: &Job) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hbox.set_margin_top(12);
    hbox.set_margin_bottom(12);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);

    // Status icon
    let (icon_name, css_class) = match (job.status.as_deref(), job.conclusion.as_deref()) {
        (Some("completed"), Some("success")) => ("emblem-ok-symbolic", "success"),
        (Some("completed"), Some("failure")) => ("dialog-error-symbolic", "error"),
        (Some("completed"), Some("cancelled")) => ("process-stop-symbolic", "warning"),
        (Some("in_progress"), _) => ("media-playback-start-symbolic", "accent"),
        (Some("queued"), _) => ("document-open-recent-symbolic", ""),
        _ => ("help-about-symbolic", ""),
    };

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(24);
    icon.set_valign(gtk::Align::Start);
    if !css_class.is_empty() {
        icon.add_css_class(css_class);
    }
    hbox.append(&icon);

    // Job info
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);

    let fallback = tr("Job");
    let name = job.name.as_deref().unwrap_or(fallback.as_str());
    let name_label = gtk::Label::new(Some(name));
    name_label.set_halign(gtk::Align::Start);
    name_label.add_css_class("heading");
    vbox.append(&name_label);

    let mut details = Vec::new();

    // Show friendly status
    let status_text = job.friendly_status();
    if !status_text.is_empty() && status_text != tr("Unknown") {
        details.push(status_text);
    }

    // Show duration if available
    if let Some(duration) = job.duration_string() {
        details.push(duration);
    }

    if !details.is_empty() {
        let details_label = gtk::Label::new(Some(&details.join(" • ")));
        details_label.set_halign(gtk::Align::Start);
        details_label.add_css_class("dim-label");
        details_label.add_css_class("caption");
        vbox.append(&details_label);
    }

    hbox.append(&vbox);

    row.set_child(Some(&hbox));
    row
}
