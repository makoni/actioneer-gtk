use crate::ui::ansi::{AnsiStyle, parse_ansi};
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use gtk4::{self as gtk, pango};
use std::collections::HashMap;

pub(super) fn render_ansi_logs(text_view: &gtk::TextView, logs: &str) {
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
