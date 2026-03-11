use crate::api::models::WorkflowRun;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct RunDigest {
    pub(crate) id: i64,
    pub(crate) status: Option<String>,
    pub(crate) conclusion: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) notified_conclusion: Option<String>,
}

pub(crate) type RunDigestMap = HashMap<i64, RunDigest>;
pub(crate) type RunDigestStore = HashMap<i64, RunDigestMap>;
pub(crate) type NotificationRequest = (String, String, Option<String>);

pub(crate) fn update_digest_and_collect_notifications(
    store: &mut RunDigestStore,
    workflow_id: i64,
    runs: &[WorkflowRun],
) -> (bool, Option<Vec<NotificationRequest>>) {
    let previous = store.get(&workflow_id).cloned();

    let mut digest = digest_runs(runs);
    if let Some(prev_map) = previous.as_ref() {
        for (run_id, entry) in digest.iter_mut() {
            if let Some(prev) = prev_map.get(run_id) {
                entry.notified_conclusion = prev.notified_conclusion.clone();
            }
        }
    }

    let changed = previous.as_ref() != Some(&digest);

    let mut notification_requests = None;
    if changed {
        if let Some(prev) = previous.as_ref() {
            let requests = collect_completed_notifications(prev, runs);

            if !requests.is_empty() {
                for run in runs.iter() {
                    if let Some(entry) = digest.get_mut(&run.id)
                        && run
                            .status
                            .as_deref()
                            .map(|s| s.eq_ignore_ascii_case("completed"))
                            .unwrap_or(false)
                    {
                        entry.notified_conclusion = run.conclusion.clone();
                    }
                }

                notification_requests = Some(requests);
            }
        }

        store.insert(workflow_id, digest);
    } else {
        // Ensure we store the initial digest even when unchanged (empty previous store).
        store.entry(workflow_id).or_insert(digest);
    }

    (changed, notification_requests)
}

pub(super) fn digest_runs(runs: &[WorkflowRun]) -> RunDigestMap {
    runs.iter()
        .map(|run| {
            (
                run.id,
                RunDigest {
                    id: run.id,
                    status: run.status.clone(),
                    conclusion: run.conclusion.clone(),
                    updated_at: run.updated_at.clone(),
                    notified_conclusion: None,
                },
            )
        })
        .collect()
}

pub(super) fn collect_completed_notifications(
    previous: &RunDigestMap,
    runs: &[WorkflowRun],
) -> Vec<(String, String, Option<String>)> {
    runs.iter()
        .filter(|run| is_completed_status(run.status.as_ref()))
        .filter_map(|run| {
            let prior = previous.get(&run.id)?;

            let prev_completed = is_completed_status(prior.status.as_ref());
            let conclusion_changed = prior.conclusion != run.conclusion;

            if prev_completed
                && prior.notified_conclusion.is_some()
                && prior.notified_conclusion == run.conclusion
            {
                return None;
            }

            if !prev_completed || conclusion_changed {
                let title = build_run_notification_title(run);
                let status = run
                    .status
                    .clone()
                    .unwrap_or_else(|| "completed".to_string());
                let conclusion = run.conclusion.clone();
                Some((title, status, conclusion))
            } else {
                None
            }
        })
        .collect()
}

fn is_completed_status(status: Option<&String>) -> bool {
    status
        .map(|value| value.eq_ignore_ascii_case("completed"))
        .unwrap_or(false)
}

fn build_run_notification_title(run: &WorkflowRun) -> String {
    let base = run
        .display_title
        .clone()
        .or(run.name.clone())
        .unwrap_or_else(|| {
            run.run_number
                .map(|num| format!("Run #{num}"))
                .unwrap_or_else(|| format!("Run {}", run.id))
        });

    if let Some(branch) = run.head_branch.as_ref().filter(|b| !b.is_empty()) {
        format!("{base} ({branch})")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_run(id: i64, status: &str, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            workflow_id: None,
            name: Some(format!("Run {id}")),
            display_title: Some(format!("Run {id}")),
            head_branch: Some("main".to_string()),
            status: Some(status.to_string()),
            conclusion: conclusion.map(|c| c.to_string()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            html_url: None,
        }
    }

    #[test]
    fn digest_runs_treats_reordering_as_unchanged() {
        let run_a = build_run(1, "completed", Some("success"));
        let run_b = build_run(2, "in_progress", None);

        let digest_first = digest_runs(&[run_a.clone(), run_b.clone()]);
        let digest_second = digest_runs(&[run_b, run_a]);

        assert_eq!(digest_first, digest_second);
    }

    #[test]
    fn collect_completed_notifications_detects_new_completion() {
        let previous = digest_runs(&[build_run(1, "in_progress", None)]);
        let current = vec![build_run(1, "completed", Some("success"))];

        let notifications = collect_completed_notifications(&previous, &current);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "Run 1 (main)");
        assert_eq!(notifications[0].1, "completed");
        assert_eq!(notifications[0].2, Some("success".to_string()));
    }

    #[test]
    fn update_digest_marks_notifications_once() {
        let mut store = RunDigestStore::new();

        // initial in-progress snapshot
        let initial_runs = vec![build_run(1, "in_progress", None)];
        let (changed_initial, notifications_initial) =
            update_digest_and_collect_notifications(&mut store, 1, &initial_runs);

        assert!(changed_initial);
        assert!(notifications_initial.is_none());

        // run completes — should notify once and mark notified
        let completed_runs = vec![build_run(1, "completed", Some("success"))];
        let (changed_completed, notifications_completed) =
            update_digest_and_collect_notifications(&mut store, 1, &completed_runs);

        assert!(changed_completed);
        let notes = notifications_completed.expect("expected notifications");
        assert_eq!(notes.len(), 1);

        // same data again — should not notify twice
        let (changed_repeat, notifications_repeat) =
            update_digest_and_collect_notifications(&mut store, 1, &completed_runs);

        assert!(!changed_repeat);
        assert!(notifications_repeat.is_none());
    }
}
