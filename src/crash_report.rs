use chrono::Utc;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrashKind {
    Panic,
    AbnormalExit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrashReport {
    pub id: String,
    pub timestamp: String,
    pub app_version: String,
    pub profile: String,
    pub locale: Option<String>,
    pub kind: CrashKind,
    pub panic_location: Option<String>,
    pub panic_payload: Option<String>,
    pub backtrace: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMarker {
    pub session_id: String,
    pub started_at: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingReport {
    pub report_id: String,
    pub report_path: String,
    pub created_at: String,
}

pub fn app_state_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("actioneer")
}

pub fn ensure_storage_dirs_in(base_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(crash_reports_dir_from(base_dir))?;
    fs::create_dir_all(session_dir_from(base_dir))?;
    Ok(())
}

pub fn generate_report_id(pid: u32) -> String {
    format!("crash-{}-{}", Utc::now().format("%Y%m%dT%H%M%S%.3fZ"), pid)
}

pub fn persist_panic_report(
    panic_location: &str,
    panic_payload: &str,
    backtrace: &str,
    session_id: Option<&str>,
) -> io::Result<PathBuf> {
    persist_panic_report_in(
        &app_state_dir(),
        panic_location,
        panic_payload,
        backtrace,
        session_id,
    )
}

pub fn initialize_session_lifecycle() -> io::Result<SessionMarker> {
    initialize_session_lifecycle_in(&app_state_dir(), std::process::id())
}

pub fn initialize_session_lifecycle_in(base_dir: &Path, pid: u32) -> io::Result<SessionMarker> {
    initialize_session_lifecycle_with(base_dir, pid, is_process_active)
}

fn initialize_session_lifecycle_with<F>(
    base_dir: &Path,
    pid: u32,
    is_pid_active: F,
) -> io::Result<SessionMarker>
where
    F: Fn(u32) -> bool,
{
    ensure_storage_dirs_in(base_dir)?;
    let stale_markers = read_session_marker_entries_in(base_dir)?
        .into_iter()
        .filter(|entry| !is_pid_active(entry.marker.pid))
        .collect::<Vec<_>>();
    let has_pending_report = read_pending_report_in(base_dir)?.is_some();

    if !has_pending_report && let Some(previous) = stale_markers.first() {
        persist_abnormal_exit_report_in(base_dir, &previous.marker)?;
    }

    for stale in &stale_markers {
        if stale.path.exists() {
            fs::remove_file(&stale.path)?;
        }
    }

    let current = SessionMarker {
        session_id: generate_session_id(pid),
        started_at: Utc::now().to_rfc3339(),
        pid,
    };
    write_session_marker_in(base_dir, &current)?;
    Ok(current)
}

pub fn mark_session_clean(session_id: &str) -> io::Result<()> {
    mark_session_clean_in(&app_state_dir(), session_id)
}

pub fn mark_session_clean_in(base_dir: &Path, session_id: &str) -> io::Result<()> {
    let marker_path = session_marker_path_from(base_dir, session_id);
    if marker_path.exists() {
        fs::remove_file(marker_path)?;
    }
    Ok(())
}

pub fn persist_panic_report_in(
    base_dir: &Path,
    panic_location: &str,
    panic_payload: &str,
    backtrace: &str,
    session_id: Option<&str>,
) -> io::Result<PathBuf> {
    ensure_storage_dirs_in(base_dir)?;

    let report_id = generate_report_id(std::process::id());
    let now = Utc::now().to_rfc3339();
    let report = CrashReport {
        id: report_id.clone(),
        timestamp: now.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        profile: option_env!("PROFILE").unwrap_or("unknown").to_string(),
        locale: std::env::var("ACTIONEER_EFFECTIVE_LANG").ok(),
        kind: CrashKind::Panic,
        panic_location: Some(sanitize_for_report(panic_location)),
        panic_payload: Some(sanitize_for_report(panic_payload)),
        backtrace: Some(sanitize_for_report(backtrace)),
        session_id: session_id.map(ToString::to_string),
    };

    let json_path = report_json_path_from(base_dir, &report.id);
    let text_path = report_text_path_from(base_dir, &report.id);
    let json_body = serde_json::to_string_pretty(&report)
        .map_err(|err| io::Error::other(format!("failed to serialize crash report: {err}")))?;
    fs::write(&json_path, json_body)?;
    fs::write(&text_path, render_text_report(&report))?;

    let pending = PendingReport {
        report_id: report.id.clone(),
        report_path: json_path.to_string_lossy().to_string(),
        created_at: now,
    };
    let pending_body = serde_json::to_string_pretty(&pending)
        .map_err(|err| io::Error::other(format!("failed to serialize pending report: {err}")))?;
    fs::write(pending_report_path_from(base_dir), pending_body)?;

    Ok(json_path)
}

pub fn read_pending_report() -> io::Result<Option<PendingReport>> {
    read_pending_report_in(&app_state_dir())
}

pub fn read_pending_report_in(base_dir: &Path) -> io::Result<Option<PendingReport>> {
    let pending_path = pending_report_path_from(base_dir);
    if !pending_path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(pending_path)?;
    let report = serde_json::from_str::<PendingReport>(&data).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse pending crash report: {err}"),
        )
    })?;
    Ok(Some(report))
}

pub fn clear_pending_report() -> io::Result<()> {
    clear_pending_report_in(&app_state_dir())
}

pub fn clear_pending_report_in(base_dir: &Path) -> io::Result<()> {
    let pending_path = pending_report_path_from(base_dir);
    if pending_path.exists() {
        fs::remove_file(pending_path)?;
    }
    Ok(())
}

pub fn build_issue_url_for_pending_report() -> io::Result<Option<String>> {
    let Some(pending) = read_pending_report()? else {
        return Ok(None);
    };
    build_issue_url_for_pending_report_in(&pending).map(Some)
}

pub fn build_issue_url_for_pending_report_in(pending: &PendingReport) -> io::Result<String> {
    let report_data = fs::read_to_string(&pending.report_path)?;
    let report = serde_json::from_str::<CrashReport>(&report_data).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse crash report: {err}"),
        )
    })?;

    let title = match report.kind {
        CrashKind::Panic => format!("Crash report: {}", report.id),
        CrashKind::AbnormalExit => format!("Abnormal exit report: {}", report.id),
    };
    let location = report.panic_location.as_deref().unwrap_or("unknown");
    let payload = report.panic_payload.as_deref().unwrap_or("unknown");
    let body = format!(
        "## Crash report\n\
Please attach this crash report file:\n\
`{}`\n\n\
## Summary\n\
- App version: {}\n\
- Profile: {}\n\
- Timestamp: {}\n\
- Kind: {:?}\n\
- Panic location: {}\n\
- Panic payload: {}\n\
\n\
## Steps to reproduce\n\
1. ...\n\
2. ...\n\
3. ...\n\
\n\
## Expected behavior\n\
...\n\
\n\
## Actual behavior\n\
...\n",
        pending.report_path,
        report.app_version,
        report.profile,
        report.timestamp,
        report.kind,
        location,
        payload
    );

    let mut url = Url::parse("https://github.com/makoni/actioneer-gtk/issues/new")
        .map_err(|err| io::Error::other(format!("invalid issue URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("title", title.as_str())
        .append_pair("body", body.as_str())
        .append_pair("labels", "bug");
    Ok(url.into())
}

pub fn sanitize_for_report(input: &str) -> String {
    let mut sanitized = input.to_string();
    sanitized = redact_prefixed_token(&sanitized, "ghp_");
    sanitized = redact_prefixed_token(&sanitized, "github_pat_");
    sanitized = redact_bearer_token(&sanitized);
    sanitized = redact_key_value(&sanitized, "token");
    sanitized = redact_key_value(&sanitized, "access_token");
    sanitized
}

fn redact_prefixed_token(input: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if input[i..].starts_with(prefix) {
            out.push_str(prefix);
            out.push_str("[REDACTED]");
            i += prefix.len();
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn redact_bearer_token(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let lower = input.to_ascii_lowercase();
    let mut idx = 0;
    while let Some(found) = lower[idx..].find("bearer ") {
        let start = idx + found;
        out.push_str(&input[idx..start + "bearer ".len()]);
        let mut j = start + "bearer ".len();
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let c = bytes[j] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                j += 1;
            } else {
                break;
            }
        }
        out.push_str("[REDACTED]");
        idx = j;
    }
    out.push_str(&input[idx..]);
    out
}

fn redact_key_value(input: &str, key: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let lower = input.to_ascii_lowercase();
    let needle = format!("{key}=");
    let mut idx = 0;
    while let Some(found) = lower[idx..].find(&needle) {
        let start = idx + found;
        out.push_str(&input[idx..start + needle.len()]);
        out.push_str("[REDACTED]");
        let mut j = start + needle.len();
        let bytes = input.as_bytes();
        while j < bytes.len() {
            let c = bytes[j] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                j += 1;
            } else {
                break;
            }
        }
        idx = j;
    }
    out.push_str(&input[idx..]);
    out
}

fn crash_reports_dir_from(base_dir: &Path) -> PathBuf {
    base_dir.join("crashes")
}

fn session_dir_from(base_dir: &Path) -> PathBuf {
    base_dir.join("session")
}

fn session_marker_path_from(base_dir: &Path, session_id: &str) -> PathBuf {
    session_dir_from(base_dir).join(format!("{session_id}.json"))
}

fn pending_report_path_from(base_dir: &Path) -> PathBuf {
    crash_reports_dir_from(base_dir).join("pending.json")
}

fn report_json_path_from(base_dir: &Path, report_id: &str) -> PathBuf {
    crash_reports_dir_from(base_dir).join(format!("{report_id}.json"))
}

fn report_text_path_from(base_dir: &Path, report_id: &str) -> PathBuf {
    crash_reports_dir_from(base_dir).join(format!("{report_id}.txt"))
}

fn write_session_marker_in(base_dir: &Path, marker: &SessionMarker) -> io::Result<()> {
    ensure_storage_dirs_in(base_dir)?;
    let body = serde_json::to_string_pretty(marker)
        .map_err(|err| io::Error::other(format!("failed to serialize session marker: {err}")))?;
    fs::write(session_marker_path_from(base_dir, &marker.session_id), body)?;
    Ok(())
}

fn read_session_marker_entries_in(base_dir: &Path) -> io::Result<Vec<SessionMarkerEntry>> {
    let session_dir = session_dir_from(base_dir);
    if !session_dir.exists() {
        return Ok(Vec::new());
    }

    let mut markers = Vec::new();
    for entry in fs::read_dir(session_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let data = fs::read_to_string(&path)?;
        let marker = serde_json::from_str::<SessionMarker>(&data).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse session marker '{}': {err}", path.display()),
            )
        })?;
        markers.push(SessionMarkerEntry { marker, path });
    }

    markers.sort_by(|left, right| {
        left.marker
            .started_at
            .cmp(&right.marker.started_at)
            .then(left.marker.session_id.cmp(&right.marker.session_id))
    });

    Ok(markers)
}

fn persist_abnormal_exit_report_in(base_dir: &Path, marker: &SessionMarker) -> io::Result<PathBuf> {
    let report_id = generate_report_id(std::process::id());
    let now = Utc::now().to_rfc3339();
    let report = CrashReport {
        id: report_id.clone(),
        timestamp: now.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        profile: option_env!("PROFILE").unwrap_or("unknown").to_string(),
        locale: std::env::var("ACTIONEER_EFFECTIVE_LANG").ok(),
        kind: CrashKind::AbnormalExit,
        panic_location: None,
        panic_payload: Some("Previous Actioneer session exited unexpectedly.".to_string()),
        backtrace: None,
        session_id: Some(marker.session_id.clone()),
    };

    let json_path = report_json_path_from(base_dir, &report.id);
    let text_path = report_text_path_from(base_dir, &report.id);
    let json_body = serde_json::to_string_pretty(&report)
        .map_err(|err| io::Error::other(format!("failed to serialize crash report: {err}")))?;
    fs::write(&json_path, json_body)?;
    fs::write(&text_path, render_text_report(&report))?;

    let pending = PendingReport {
        report_id: report.id.clone(),
        report_path: json_path.to_string_lossy().to_string(),
        created_at: now,
    };
    let pending_body = serde_json::to_string_pretty(&pending)
        .map_err(|err| io::Error::other(format!("failed to serialize pending report: {err}")))?;
    fs::write(pending_report_path_from(base_dir), pending_body)?;
    Ok(json_path)
}

fn generate_session_id(pid: u32) -> String {
    format!(
        "session-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        pid
    )
}

#[derive(Debug, Clone)]
struct SessionMarkerEntry {
    marker: SessionMarker,
    path: PathBuf,
}

#[cfg(target_os = "linux")]
fn is_process_active(pid: u32) -> bool {
    pid != 0 && Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn is_process_active(_pid: u32) -> bool {
    false
}

fn render_text_report(report: &CrashReport) -> String {
    format!(
        "Actioneer crash report\n\
Timestamp: {}\n\
Version: {}\n\
Profile: {}\n\
Locale: {}\n\
Kind: {:?}\n\
Session ID: {}\n\
Panic location: {}\n\
Panic payload: {}\n\n\
Backtrace:\n{}\n",
        report.timestamp,
        report.app_version,
        report.profile,
        report.locale.as_deref().unwrap_or("unknown"),
        report.kind,
        report.session_id.as_deref().unwrap_or("unknown"),
        report.panic_location.as_deref().unwrap_or("unknown"),
        report.panic_payload.as_deref().unwrap_or("unknown"),
        report.backtrace.as_deref().unwrap_or("not captured")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::tempdir;

    #[test]
    fn crash_paths_use_actioneer_state_dir() {
        let base = app_state_dir();
        assert!(base.to_string_lossy().contains("actioneer"));
        assert_eq!(crash_reports_dir_from(&base), base.join("crashes"));
        assert_eq!(session_dir_from(&base), base.join("session"));
        assert_eq!(
            session_marker_path_from(&base, "session-1"),
            base.join("session").join("session-1.json")
        );
        assert_eq!(
            pending_report_path_from(&base),
            base.join("crashes").join("pending.json")
        );
    }

    #[test]
    fn sanitize_redacts_token_patterns() {
        let input =
            "Authorization: Bearer abc123 token=ghp_secret access_token=xyz github_pat_abcd123";
        let output = sanitize_for_report(input);
        assert!(!output.contains("abc123"));
        assert!(!output.contains("ghp_secret"));
        assert!(!output.contains("xyz"));
        assert!(!output.contains("github_pat_abcd123"));
        assert!(output.contains("Bearer [REDACTED]"));
        assert!(output.contains("token=[REDACTED]"));
        assert!(output.contains("access_token=[REDACTED]"));
        assert!(output.contains("github_pat_[REDACTED]"));
    }

    #[test]
    fn persist_panic_report_writes_json_text_and_pending() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        let output_path = persist_panic_report_in(
            &base,
            "src/main.rs:10:2",
            "token=ghp_abc123",
            "Authorization: Bearer supersecret",
            Some("session-1"),
        )
        .expect("persist panic report");

        assert!(output_path.exists());
        let text_path = output_path.with_extension("txt");
        assert!(text_path.exists());

        let json = fs::read_to_string(&output_path).expect("read json report");
        assert!(!json.contains("ghp_abc123"));
        assert!(json.contains("token=[REDACTED]"));

        let pending = read_pending_report_in(&base)
            .expect("read pending")
            .expect("pending report exists");
        assert_eq!(pending.report_path, output_path.to_string_lossy());
        assert!(!pending.report_id.is_empty());
    }

    #[test]
    fn initialize_session_lifecycle_generates_unclean_exit_report() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");

        let old_marker = SessionMarker {
            session_id: "session-old".to_string(),
            started_at: Utc::now().to_rfc3339(),
            pid: 100,
        };
        write_session_marker_in(&base, &old_marker).expect("write previous marker");

        let current = initialize_session_lifecycle_with(&base, 200, |_| false)
            .expect("initialize session lifecycle");
        assert_ne!(current.session_id, old_marker.session_id);

        let pending = read_pending_report_in(&base)
            .expect("read pending")
            .expect("pending should exist");
        let report_json = fs::read_to_string(&pending.report_path).expect("read report json");
        let report: CrashReport = serde_json::from_str(&report_json).expect("parse report json");
        assert_eq!(report.kind, CrashKind::AbnormalExit);
        assert_eq!(report.session_id.as_deref(), Some("session-old"));
        assert!(!session_marker_path_from(&base, "session-old").exists());
    }

    #[test]
    fn initialize_session_lifecycle_ignores_live_other_instances() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        let live_marker = SessionMarker {
            session_id: "session-live".to_string(),
            started_at: Utc::now().to_rfc3339(),
            pid: 100,
        };
        write_session_marker_in(&base, &live_marker).expect("write previous marker");

        let current =
            initialize_session_lifecycle_with(&base, 200, |pid| pid == 100).expect("initialize");

        assert_eq!(read_pending_report_in(&base).expect("read pending"), None);
        assert!(session_marker_path_from(&base, &live_marker.session_id).exists());
        assert!(session_marker_path_from(&base, &current.session_id).exists());
    }

    #[test]
    fn initialize_session_lifecycle_clears_stale_markers_when_pending_report_exists() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        let stale_marker = SessionMarker {
            session_id: "session-stale".to_string(),
            started_at: Utc::now().to_rfc3339(),
            pid: 100,
        };
        write_session_marker_in(&base, &stale_marker).expect("write stale marker");
        fs::write(
            pending_report_path_from(&base),
            r#"{"report_id":"id","report_path":"/tmp/id.json","created_at":"now"}"#,
        )
        .expect("write pending");

        let current = initialize_session_lifecycle_with(&base, 200, |_| false)
            .expect("initialize session lifecycle");

        let session_dir = session_dir_from(&base);
        let session_files = fs::read_dir(&session_dir)
            .expect("read session dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<HashSet<_>>();
        assert_eq!(
            session_files,
            HashSet::from([format!("{}.json", current.session_id)])
        );
    }

    #[test]
    fn mark_session_clean_removes_matching_marker() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        let marker = SessionMarker {
            session_id: "session-abc".to_string(),
            started_at: Utc::now().to_rfc3339(),
            pid: 100,
        };
        let other = SessionMarker {
            session_id: "session-other".to_string(),
            started_at: Utc::now().to_rfc3339(),
            pid: 200,
        };
        write_session_marker_in(&base, &marker).expect("write marker");
        write_session_marker_in(&base, &other).expect("write other marker");
        mark_session_clean_in(&base, "session-abc").expect("mark clean");
        assert!(!session_marker_path_from(&base, "session-abc").exists());
        assert!(session_marker_path_from(&base, "session-other").exists());
    }

    #[test]
    fn clear_pending_report_removes_marker_file() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        ensure_storage_dirs_in(&base).expect("create dirs");
        let pending_path = pending_report_path_from(&base);
        fs::write(
            &pending_path,
            r#"{"report_id":"id","report_path":"/tmp/id.json","created_at":"now"}"#,
        )
        .expect("write pending");
        clear_pending_report_in(&base).expect("clear pending");
        assert!(!pending_path.exists());
    }

    #[test]
    fn builds_github_issue_url_for_pending_report() {
        let temp = tempdir().expect("temp dir");
        let base = temp.path().join("state");
        ensure_storage_dirs_in(&base).expect("create dirs");

        let report = CrashReport {
            id: "crash-1".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            app_version: "1.0.7".to_string(),
            profile: "debug".to_string(),
            locale: Some("ru".to_string()),
            kind: CrashKind::Panic,
            panic_location: Some("src/main.rs:10:2".to_string()),
            panic_payload: Some("boom".to_string()),
            backtrace: Some("trace".to_string()),
            session_id: Some("session-1".to_string()),
        };
        let report_path = report_json_path_from(&base, "crash-1");
        fs::write(
            &report_path,
            serde_json::to_string_pretty(&report).expect("serialize report"),
        )
        .expect("write report");

        let pending = PendingReport {
            report_id: "crash-1".to_string(),
            report_path: report_path.to_string_lossy().to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        let url = build_issue_url_for_pending_report_in(&pending).expect("build issue url");
        let parsed = Url::parse(url.as_str()).expect("parse issue URL");
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query.get("labels").map(String::as_str), Some("bug"));
        assert!(
            query
                .get("title")
                .is_some_and(|title| title.contains("crash-1"))
        );
        assert!(
            query
                .get("body")
                .is_some_and(|body| body.contains("attach this crash report file"))
        );
    }
}
