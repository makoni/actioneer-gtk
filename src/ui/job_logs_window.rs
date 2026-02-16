use crate::api::models::{Job, Repo};
use crate::api::{GitHubClient, GitHubError};
use crate::i18n::tr;
use crate::ui::ansi::{AnsiStyle, parse_ansi};
use crate::ui::utils::channel::MainContextChannelExt;
use gtk4::gdk;
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};
use libadwaita as adw;
use libadwaita::prelude::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
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
        let fallback_job = tr("Job");
        let job_title = job
            .name
            .as_deref()
            .unwrap_or(fallback_job.as_str())
            .to_string();

        let window = adw::Window::builder()
            .title(tr("{title} - Logs").replace("{title}", job_title.as_str()))
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
        text_view.buffer().set_text(tr("Fetching logs...").as_str());

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
        refresh_button.set_tooltip_text(Some(tr("Refresh logs").as_str()));
        header.pack_start(&refresh_button);

        // Copy button
        let copy_button = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_button.set_tooltip_text(Some(tr("Copy logs to clipboard").as_str()));
        header.pack_end(&copy_button);

        // Save button
        let save_button = gtk::Button::from_icon_name("document-save-symbolic");
        save_button.set_tooltip_text(Some(tr("Save logs to file").as_str()));
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
        let job_title = self.job_title.clone();

        button.connect_clicked(move |_| {
            let dialog = gtk::FileChooserNative::builder()
                .title(tr("Save Logs"))
                .accept_label(tr("Save"))
                .cancel_label(tr("Cancel"))
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
                    let toast = adw::Toast::new(tr("Logs are empty; nothing saved").as_str());
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
                            let toast =
                                adw::Toast::new(tr("Unable to determine save location").as_str());
                            toast.set_timeout(5);
                            overlay_for_dialog.add_toast(toast);
                        }
                    }
                    _ => {
                        let toast = adw::Toast::new(tr("No file selected").as_str());
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
    render_structured_logs(text_view, logs);
}

fn render_structured_logs(text_view: &gtk::TextView, logs: &str) {
    let buffer = text_view.buffer();
    buffer.set_text("");

    let mut iter = buffer.end_iter();
    let tag_table = buffer.tag_table();
    let mut ansi_tags: HashMap<AnsiStyle, gtk::TextTag> = HashMap::new();
    let mut tags: HashMap<&'static str, gtk::TextTag> = HashMap::new();

    let mut group_depth = 0usize;

    for (index, line) in logs.split('\n').enumerate() {
        if index > 0 {
            buffer.insert(&mut iter, "\n");
        }

        let (timestamp, rest) = split_timestamp(line);
        if let Some(ts) = timestamp {
            let tag = get_timestamp_tag(&tag_table, &mut tags);
            buffer.insert_with_tags(&mut iter, ts, &[&tag]);
            if !rest.is_empty() {
                let padding = timestamp_padding(ts);
                buffer.insert_with_tags(&mut iter, &" ".repeat(padding), &[&tag]);
            }
        }

        let rest_trim = rest.trim_start();
        if is_group_end(rest_trim) {
            group_depth = group_depth.saturating_sub(1);
            continue;
        }

        if let Some(title) = group_title(rest_trim) {
            insert_indent(&buffer, &mut iter, group_depth);
            let tag = get_group_tag(&tag_table, &mut tags);
            buffer.insert_with_tags(&mut iter, "▾ ", &[&tag]);
            buffer.insert_with_tags(&mut iter, title, &[&tag]);
            group_depth += 1;
            continue;
        }

        insert_indent(&buffer, &mut iter, group_depth);

        if let Some(message) = split_error_prefix(rest_trim) {
            let tag = get_error_line_tag(&tag_table, &mut tags);
            buffer.insert_with_tags(&mut iter, "⛔ ", &[&tag]);
            insert_ansi_text(
                &buffer,
                &mut iter,
                message,
                &tag_table,
                &mut ansi_tags,
                Some(get_secret_tag(&tag_table, &mut tags)),
                Some(tag),
            );
            continue;
        }

        if is_plain_error_line(rest_trim) {
            let tag = get_error_line_tag(&tag_table, &mut tags);
            insert_ansi_text(
                &buffer,
                &mut iter,
                rest_trim,
                &tag_table,
                &mut ansi_tags,
                Some(get_secret_tag(&tag_table, &mut tags)),
                Some(tag),
            );
            continue;
        }

        if let Some(command) = parse_workflow_command(rest_trim) {
            let (icon, tag) = get_annotation_tag(&tag_table, &mut tags, command.kind);
            buffer.insert_with_tags(&mut iter, icon, &[&tag]);
            buffer.insert_with_tags(&mut iter, " ", &[&tag]);
            buffer.insert_with_tags(&mut iter, command.message.as_str(), &[&tag]);
            if let Some(meta) = build_annotation_meta(&command.params) {
                let meta_tag = get_annotation_meta_tag(&tag_table, &mut tags);
                buffer.insert_with_tags(&mut iter, " ", &[&meta_tag]);
                buffer.insert_with_tags(&mut iter, meta.as_str(), &[&meta_tag]);
            }
            continue;
        }

        if let Some((leading_ws, command_tail)) = split_command_prefix(rest) {
            if !leading_ws.is_empty() {
                buffer.insert(&mut iter, leading_ws);
            }
            let command_tag = get_command_tag(&tag_table, &mut tags);
            buffer.insert_with_tags(&mut iter, "▶ [command] ", &[&command_tag]);
            insert_ansi_text(
                &buffer,
                &mut iter,
                command_tail,
                &tag_table,
                &mut ansi_tags,
                Some(get_secret_tag(&tag_table, &mut tags)),
                Some(command_tag),
            );
        } else {
            insert_ansi_text(
                &buffer,
                &mut iter,
                rest,
                &tag_table,
                &mut ansi_tags,
                Some(get_secret_tag(&tag_table, &mut tags)),
                None,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WorkflowCommandKind {
    Notice,
    Warning,
    Error,
    Debug,
}

#[derive(Debug)]
struct WorkflowCommand<'a> {
    kind: WorkflowCommandKind,
    message: String,
    params: HashMap<String, String>,
    _raw: &'a str,
}

fn split_timestamp(line: &str) -> (Option<&str>, &str) {
    let line = line.strip_prefix("\u{FEFF}").unwrap_or(line);
    if line.len() < 10 {
        return (None, line);
    }

    if !line.chars().take(4).all(|ch| ch.is_ascii_digit()) {
        return (None, line);
    }

    if let Some(pos) = line.find("Z ")
        && pos <= 30
    {
        let (ts, rest) = line.split_at(pos + 1);
        let rest = rest.trim_start_matches(' ');
        return (Some(ts), rest);
    }

    (None, line)
}

const TIMESTAMP_PAD_WIDTH: usize = 30;

fn timestamp_padding(timestamp: &str) -> usize {
    let length = timestamp.len();
    if length >= TIMESTAMP_PAD_WIDTH {
        1
    } else {
        TIMESTAMP_PAD_WIDTH.saturating_sub(length) + 1
    }
}

fn is_group_end(text: &str) -> bool {
    text.starts_with("##[endgroup]")
}

fn group_title(text: &str) -> Option<&str> {
    text.strip_prefix("##[group]")
        .map(|title| title.trim())
        .filter(|title| !title.is_empty())
}

fn split_command_prefix(text: &str) -> Option<(&str, &str)> {
    let leading_len = text
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .map(|ch| ch.len_utf8())
        .sum::<usize>();
    let (leading, remainder) = text.split_at(leading_len);
    let tail = remainder
        .strip_prefix("##[command]")
        .or_else(|| remainder.strip_prefix("[command]"))
        .or_else(|| remainder.strip_prefix("▶ [command]"))?;
    Some((leading, tail.trim_start()))
}

fn parse_workflow_command(text: &str) -> Option<WorkflowCommand<'_>> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("::") {
        return None;
    }

    let rest = &trimmed[2..];
    let sep = rest.find("::")?;
    let (command_block, message) = rest.split_at(sep);
    let message = message.trim_start_matches("::").trim().to_string();
    if message.is_empty() {
        return None;
    }

    let mut parts = command_block.splitn(2, char::is_whitespace);
    let name = parts.next()?.trim();
    let params_str = parts.next().unwrap_or("").trim();

    let kind = match name {
        "notice" => WorkflowCommandKind::Notice,
        "warning" => WorkflowCommandKind::Warning,
        "error" => WorkflowCommandKind::Error,
        "debug" => WorkflowCommandKind::Debug,
        _ => return None,
    };

    let params = parse_command_params(params_str);
    Some(WorkflowCommand {
        kind,
        message,
        params,
        _raw: text,
    })
}

fn parse_command_params(input: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if input.is_empty() {
        return params;
    }

    for pair in input.split(',') {
        let mut iter = pair.splitn(2, '=');
        let key = iter.next().unwrap_or("").trim();
        let value = iter.next().unwrap_or("").trim();
        if !key.is_empty() && !value.is_empty() {
            params.insert(key.to_string(), value.to_string());
        }
    }

    params
}

fn build_annotation_meta(params: &HashMap<String, String>) -> Option<String> {
    let file = params.get("file")?;
    let line = params.get("line");
    let col = params.get("col");
    let title = params.get("title");

    let mut parts = Vec::new();
    if let Some(title) = title {
        parts.push(format!("({})", title));
    }

    let mut location = file.clone();
    if let Some(line) = line {
        location.push(':');
        location.push_str(line);
        if let Some(col) = col {
            location.push(':');
            location.push_str(col);
        }
    }

    parts.push(location);

    Some(format!("— {}", parts.join(" ")))
}

fn insert_indent(buffer: &gtk::TextBuffer, iter: &mut gtk::TextIter, depth: usize) {
    if depth == 0 {
        return;
    }

    let indent = "  ".repeat(depth);
    buffer.insert(iter, &indent);
}

fn split_error_prefix(text: &str) -> Option<&str> {
    text.strip_prefix("##[error]")
        .map(|tail| tail.trim_start())
        .filter(|tail| !tail.is_empty())
}

fn is_plain_error_line(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("Error:") || trimmed.starts_with("error:") || trimmed.starts_with("ERROR:")
}

fn insert_ansi_text(
    buffer: &gtk::TextBuffer,
    iter: &mut gtk::TextIter,
    text: &str,
    tag_table: &gtk::TextTagTable,
    ansi_tags: &mut HashMap<AnsiStyle, gtk::TextTag>,
    secret_tag: Option<gtk::TextTag>,
    extra_tag: Option<gtk::TextTag>,
) {
    let spans = parse_ansi(text);
    for span in spans {
        let ansi_tag = if span.style.is_default() {
            None
        } else {
            Some(get_ansi_tag(tag_table, ansi_tags, &span.style))
        };
        insert_text_with_optional_secret(
            buffer,
            iter,
            &span.text,
            ansi_tag,
            secret_tag.clone(),
            extra_tag.clone(),
        );
    }
}

fn insert_text_with_optional_secret(
    buffer: &gtk::TextBuffer,
    iter: &mut gtk::TextIter,
    text: &str,
    ansi_tag: Option<gtk::TextTag>,
    secret_tag: Option<gtk::TextTag>,
    extra_tag: Option<gtk::TextTag>,
) {
    if text.is_empty() {
        return;
    }

    if !text.contains("***") {
        insert_with_tags(buffer, iter, text, ansi_tag, extra_tag);
        return;
    }

    let mut start = 0usize;
    while let Some(pos) = text[start..].find("***") {
        let abs = start + pos;
        let before = &text[start..abs];
        insert_with_tags(buffer, iter, before, ansi_tag.clone(), extra_tag.clone());

        if let Some(secret) = secret_tag.clone() {
            insert_with_tags(buffer, iter, "***", ansi_tag.clone(), Some(secret));
        } else {
            insert_with_tags(buffer, iter, "***", ansi_tag.clone(), extra_tag.clone());
        }

        start = abs + 3;
    }

    if start < text.len() {
        insert_with_tags(buffer, iter, &text[start..], ansi_tag, extra_tag);
    }
}

fn insert_with_tags(
    buffer: &gtk::TextBuffer,
    iter: &mut gtk::TextIter,
    text: &str,
    ansi_tag: Option<gtk::TextTag>,
    extra_tag: Option<gtk::TextTag>,
) {
    if text.is_empty() {
        return;
    }

    match (ansi_tag, extra_tag) {
        (Some(first), Some(second)) => buffer.insert_with_tags(iter, text, &[&first, &second]),
        (Some(tag), None) | (None, Some(tag)) => buffer.insert_with_tags(iter, text, &[&tag]),
        (None, None) => buffer.insert(iter, text),
    }
}

fn get_ansi_tag(
    tag_table: &gtk::TextTagTable,
    ansi_tags: &mut HashMap<AnsiStyle, gtk::TextTag>,
    style: &AnsiStyle,
) -> gtk::TextTag {
    ansi_tags
        .entry(style.clone())
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            if style.bold {
                let weight: i32 = pango::Weight::Bold.into_glib();
                tag.set_property("weight", weight);
            }
            if style.underline {
                tag.set_property("underline", pango::Underline::Single);
            }
            if let Some(fg) = style.fg {
                tag.set_property("foreground", fg.to_css());
            }
            if let Some(bg) = style.bg {
                tag.set_property("background", bg.to_css());
            }
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_timestamp_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("timestamp")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            tag.set_property("foreground", "#8a8a8a");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_group_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("group")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            let weight: i32 = pango::Weight::Bold.into_glib();
            tag.set_property("weight", weight);
            tag.set_property("foreground", "#3465a4");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_command_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("command")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            let weight: i32 = pango::Weight::Bold.into_glib();
            tag.set_property("weight", weight);
            tag.set_property("foreground", "#1a73e8");
            tag.set_property("family", "monospace");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_error_line_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("error-line")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            let weight: i32 = pango::Weight::Bold.into_glib();
            tag.set_property("weight", weight);
            tag.set_property("foreground", "#b3261e");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_secret_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("secret")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            tag.set_property("foreground", "#9aa0a6");
            tag.set_property("background", "#2b2b2b");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

fn get_annotation_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
    kind: WorkflowCommandKind,
) -> (&'static str, gtk::TextTag) {
    match kind {
        WorkflowCommandKind::Notice => (
            "ℹ",
            tags.entry("notice")
                .or_insert_with(|| {
                    let tag = gtk::TextTag::new(None);
                    let weight: i32 = pango::Weight::Bold.into_glib();
                    tag.set_property("weight", weight);
                    tag.set_property("foreground", "#0b57d0");
                    tag_table.add(&tag);
                    tag
                })
                .clone(),
        ),
        WorkflowCommandKind::Warning => (
            "⚠",
            tags.entry("warning")
                .or_insert_with(|| {
                    let tag = gtk::TextTag::new(None);
                    let weight: i32 = pango::Weight::Bold.into_glib();
                    tag.set_property("weight", weight);
                    tag.set_property("foreground", "#b26a00");
                    tag_table.add(&tag);
                    tag
                })
                .clone(),
        ),
        WorkflowCommandKind::Error => (
            "⛔",
            tags.entry("error")
                .or_insert_with(|| {
                    let tag = gtk::TextTag::new(None);
                    let weight: i32 = pango::Weight::Bold.into_glib();
                    tag.set_property("weight", weight);
                    tag.set_property("foreground", "#b3261e");
                    tag_table.add(&tag);
                    tag
                })
                .clone(),
        ),
        WorkflowCommandKind::Debug => (
            "🐛",
            tags.entry("debug")
                .or_insert_with(|| {
                    let tag = gtk::TextTag::new(None);
                    tag.set_property("foreground", "#6a6a6a");
                    tag_table.add(&tag);
                    tag
                })
                .clone(),
        ),
    }
}

fn get_annotation_meta_tag(
    tag_table: &gtk::TextTagTable,
    tags: &mut HashMap<&'static str, gtk::TextTag>,
) -> gtk::TextTag {
    tags.entry("annotation-meta")
        .or_insert_with(|| {
            let tag = gtk::TextTag::new(None);
            tag.set_property("foreground", "#8a8a8a");
            tag_table.add(&tag);
            tag
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::split_timestamp;

    #[test]
    fn split_timestamp_strips_bom_prefix() {
        let line = "\u{FEFF}2026-02-06T07:28:18Z first line";
        let (timestamp, rest) = split_timestamp(line);
        assert_eq!(timestamp, Some("2026-02-06T07:28:18Z"));
        assert_eq!(rest, "first line");
    }

    #[test]
    fn split_timestamp_parses_without_bom() {
        let line = "2026-02-06T07:28:18Z   message";
        let (timestamp, rest) = split_timestamp(line);
        assert_eq!(timestamp, Some("2026-02-06T07:28:18Z"));
        assert_eq!(rest, "message");
    }

    #[test]
    fn split_timestamp_rejects_non_timestamp_lines() {
        let (timestamp, rest) = split_timestamp("2026-02");
        assert_eq!(timestamp, None);
        assert_eq!(rest, "2026-02");

        let line = "INFO 2026-02-06T07:28:18Z message";
        let (timestamp, rest) = split_timestamp(line);
        assert_eq!(timestamp, None);
        assert_eq!(rest, line);
    }

    #[test]
    fn split_timestamp_rejects_missing_or_long_timestamps() {
        let line = "2026-02-06T07:28:18Zmessage";
        let (timestamp, rest) = split_timestamp(line);
        assert_eq!(timestamp, None);
        assert_eq!(rest, line);

        let line = "2026-02-06T07:28:18.123456789012Z message";
        let (timestamp, rest) = split_timestamp(line);
        assert_eq!(timestamp, None);
        assert_eq!(rest, line);
    }
}
