use crate::api::models::{Job, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::ui::ansi::{AnsiSpan, AnsiStyle, parse_ansi};
use crate::ui::utils::MainContextChannelExt;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};
use gtk4::glib::translate::IntoGlib;
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use tracing::{error, info, warn};

pub struct JobLogsWindow {
    window: adw::Window,
    repo: Repo,
    job: Job,
    client: Arc<Mutex<GitHubClient>>,
    text_view: gtk::TextView,
    toast_overlay: adw::ToastOverlay,
    run_title: String,
    job_title: String,
    copy_button: gtk::Button,
    save_button: gtk::Button,
}

struct ActionButtons {
    refresh: gtk::Button,
    copy: gtk::Button,
    save: gtk::Button,
}

impl JobLogsWindow {
    pub fn new(
        parent: &impl IsA<gtk::Window>,
        repo: Repo,
        run_title: String,
        job: Job,
        client: Arc<Mutex<GitHubClient>>,
    ) -> Self {
        let job_title = job.name.as_deref().unwrap_or("Job").to_string();

        let window = adw::Window::builder()
            .title(format!("{} - Logs", job_title))
            .modal(false)
            .default_width(1000)
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
        text_view.buffer().set_text("Fetching logs…");

        let toast_overlay = adw::ToastOverlay::new();
        let buttons = Self::build_ui(
            &window,
            &toast_overlay,
            &text_view,
            &job_title,
            &repo.full_name,
        );

        let logs_window = Self {
            window: window.clone(),
            repo: repo.clone(),
            job: job.clone(),
            client: client.clone(),
            text_view: text_view.clone(),
            toast_overlay: toast_overlay.clone(),
            run_title,
            job_title,
            copy_button: buttons.copy.clone(),
            save_button: buttons.save.clone(),
        };

        logs_window.connect_refresh_button(&buttons.refresh);
        logs_window.connect_copy_button(&buttons.copy);
        logs_window.connect_save_button(&buttons.save);
        logs_window.load_logs();

        logs_window
    }

    fn build_ui(
        window: &adw::Window,
        toast_overlay: &adw::ToastOverlay,
        text_view: &gtk::TextView,
        job_title: &str,
        repo_full_name: &str,
    ) -> ActionButtons {
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Header bar
        let header = adw::HeaderBar::new();
        // Refresh button
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Refresh logs"));
        header.pack_start(&refresh_button);

        // Copy button
        let copy_button = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_button.set_tooltip_text(Some("Copy logs to clipboard"));
        header.pack_end(&copy_button);

        // Save button
        let save_button = gtk::Button::from_icon_name("document-save-symbolic");
        save_button.set_tooltip_text(Some("Save logs to file"));
        header.pack_end(&save_button);
        main_box.append(&header);

        // Job info
        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        info_box.set_margin_top(12);
        info_box.set_margin_bottom(12);
        info_box.set_margin_start(12);
        info_box.set_margin_end(12);

        let job_label = gtk::Label::new(Some(job_title));
        job_label.add_css_class("title-2");
        job_label.set_halign(gtk::Align::Start);
        info_box.append(&job_label);
        let repo_label = gtk::Label::new(Some(repo_full_name));
        repo_label.add_css_class("dim-label");
        repo_label.set_halign(gtk::Align::Start);
        info_box.append(&repo_label);

        main_box.append(&info_box);
        // Separator
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        main_box.append(&separator);
        // Logs view
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(text_view));
        main_box.append(&scrolled);

        toast_overlay.set_child(Some(&main_box));
        window.set_content(Some(toast_overlay));

        ActionButtons {
            refresh: refresh_button,
            copy: copy_button,
            save: save_button,
        }
    }

    fn load_logs(&self) {
        self.set_copy_save_enabled(false);

        let client = self.client.clone();
        let owner = self.repo.owner.login.clone();
        let repo_name = self.repo.name.clone();
        let job_id = self.job.id;
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
                    text_view.buffer().set_text(
                        "Logs are not yet available for this job. GitHub only provides logs once the job starts streaming output or completes. Try refreshing in a few moments.",
                    );
                    copy_button.set_sensitive(false);
                    save_button.set_sensitive(false);
                }
                Err(e) => {
                    error!("Failed to load logs: {}", e);
                    text_view.set_sensitive(false);
                    text_view.buffer().set_text(&format!(
                        "Unable to load logs right now. Please try again later.\n\nDetails: {}",
                        e
                    ));
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
        let job_id = self.job.id;
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
                        tv_for_ui.buffer().set_text(
                            "Logs are not yet available for this job. GitHub only provides logs once the job starts streaming output or completes. Try refreshing in a few moments.",
                        );
                        copy_for_result.set_sensitive(false);
                        save_for_result.set_sensitive(false);
                    }
                    Err(e) => {
                        error!("Failed to refresh logs: {}", e);
                        tv_for_ui.set_sensitive(false);
                        tv_for_ui.buffer().set_text(&format!(
                            "Unable to load logs right now. Please try again later.\n\nDetails: {}",
                            e
                        ));
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
                let toast = adw::Toast::new("Logs are empty; nothing to copy");
                toast.set_timeout(3);
                overlay.add_toast(toast);
                return;
            }

            let display = gdk::Display::default();
            match display {
                Some(display) => {
                    let clipboard = display.clipboard();
                    clipboard.set_text(&text);
                    let toast = adw::Toast::new("Logs copied to clipboard");
                    toast.set_timeout(3);
                    overlay.add_toast(toast);
                }
                None => {
                    let toast = adw::Toast::new("Clipboard unavailable on this system");
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
        let job_title = self.job_title.clone();

        button.connect_clicked(move |_| {
            let dialog = gtk::FileChooserNative::builder()
                .title("Save Logs")
                .accept_label("Save")
                .cancel_label("Cancel")
                .action(gtk::FileChooserAction::Save)
                .transient_for(&window)
                .modal(true)
                .build();

            let default_name = JobLogsWindow::default_file_name(&run_title, &job_title);
            dialog.set_current_name(&default_name);

            let overlay_for_dialog = overlay.clone();
            let text_for_dialog = text_view.clone();

            dialog.connect_response(move |dialog, response| {
                if response != gtk::ResponseType::Accept {
                    dialog.destroy();
                    return;
                }

                let buffer = text_for_dialog.buffer();
                let text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();

                if text.is_empty() {
                    let toast = adw::Toast::new("Logs are empty; nothing saved");
                    toast.set_timeout(3);
                    overlay_for_dialog.add_toast(toast);
                    dialog.destroy();
                    return;
                }

                match dialog.file() {
                    Some(file) => {
                        if let Some(path) = file.path() {
                            let text_to_write = text.clone();
                            let (sender, receiver) = glib::MainContext::default()
                                .channel::<Result<(), String>>(glib::Priority::default());
                            let overlay_for_result = overlay_for_dialog.clone();

                            receiver.attach(None, move |message| {
                                match message {
                                    Ok(()) => {
                                        let toast = adw::Toast::new("Logs saved");
                                        toast.set_timeout(3);
                                        overlay_for_result.add_toast(toast);
                                    }
                                    Err(err) => {
                                        let toast = adw::Toast::new(&format!(
                                            "Failed to save logs: {}",
                                            err
                                        ));
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
                            let toast = adw::Toast::new("Unable to determine save location");
                            toast.set_timeout(5);
                            overlay_for_dialog.add_toast(toast);
                        }
                    }
                    _ => {
                        let toast = adw::Toast::new("No file selected");
                        toast.set_timeout(5);
                        overlay_for_dialog.add_toast(toast);
                    }
                }

                dialog.destroy();
            });

            dialog.show();
        });
    }

    fn default_file_name(run_title: &str, job_title: &str) -> String {
        let run_segment =
            Self::sanitize_filename_segment(run_title).unwrap_or_else(|| "run".to_string());
        let job_segment =
            Self::sanitize_filename_segment(job_title).unwrap_or_else(|| "job".to_string());
        format!("{} - {}.log", run_segment, job_segment)
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

fn render_ansi_logs(text_view: &gtk::TextView, logs: &str) {
    let spans = parse_ansi(logs);
    apply_ansi_spans(text_view, &spans);
}

fn apply_ansi_spans(text_view: &gtk::TextView, spans: &[AnsiSpan]) {
    let buffer = text_view.buffer();
    buffer.set_text("");

    let mut iter = buffer.end_iter();
    let tag_table = buffer.tag_table();
    let mut tags: HashMap<AnsiStyle, gtk::TextTag> = HashMap::new();

    for span in spans {
        if span.text.is_empty() {
            continue;
        }

        if span.style.is_default() {
            buffer.insert(&mut iter, &span.text);
            continue;
        }

        let tag = tags.entry(span.style.clone()).or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            if span.style.bold {
                let weight: i32 = pango::Weight::Bold.into_glib();
                tag.set_property("weight", &weight);
            }
            if span.style.underline {
                tag.set_property("underline", &pango::Underline::Single);
            }
            if let Some(fg) = span.style.fg {
                tag.set_property("foreground", &fg.to_css());
            }
            if let Some(bg) = span.style.bg {
                tag.set_property("background", &bg.to_css());
            }
            tag_table.add(&tag);
            tag
        });

        buffer.insert_with_tags(&mut iter, &span.text, &[tag]);
    }
}
