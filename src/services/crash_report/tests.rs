//! Tests for [`super`].
//!
//! Split out of `crash_report.rs` so the file it covers stays readable;
//! the module is unchanged otherwise.

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
    let input = "Authorization: Bearer abc123 token=ghp_secret access_token=xyz github_pat_abcd123";
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
