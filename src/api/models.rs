use crate::i18n::tr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub owner: User,
    #[serde(rename = "private")]
    pub is_private: bool,
    pub permissions: Option<RepoPermissions>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPermissions {
    pub admin: bool,
    pub push: bool,
    pub pull: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub commit: BranchCommit,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCommit {
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workflow {
    pub id: i64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowDispatchInputType {
    String,
    Choice,
    Boolean,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowDispatchInputValue {
    String(String),
    Boolean(bool),
}

impl WorkflowDispatchInputValue {
    pub fn as_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDispatchInput {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub input_type: WorkflowDispatchInputType,
    pub default_value: Option<WorkflowDispatchInputValue>,
    pub options: Vec<String>,
}

impl WorkflowDispatchInput {
    pub fn default_as_string(&self) -> Option<String> {
        self.default_value.as_ref().map(|value| value.as_string())
    }
}

pub fn build_dispatch_inputs_payload(
    inputs: &[WorkflowDispatchInput],
    values: &HashMap<String, WorkflowDispatchInputValue>,
) -> Result<Option<serde_json::Value>, String> {
    let mut payload = serde_json::Map::new();

    for input in inputs {
        let Some(value) = values.get(&input.name) else {
            if input.required {
                return Err(format!("Input \"{}\" is required.", input.name));
            }
            continue;
        };

        let value_string = value.as_string();
        if value_string.trim().is_empty() {
            if input.required {
                return Err(format!("Input \"{}\" is required.", input.name));
            }
            continue;
        }

        if !input.required
            && let Some(default_value) = input.default_as_string()
            && default_value == value_string
        {
            continue;
        }

        payload.insert(input.name.clone(), serde_json::Value::String(value_string));
    }

    if payload.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::Value::Object(payload)))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowsResponse {
    pub total_count: i64,
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: i64,
    pub run_number: Option<i64>,
    pub name: Option<String>,
    pub display_title: Option<String>,
    pub head_branch: Option<String>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub run_started_at: Option<String>,
    pub event: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunsResponse {
    pub total_count: i64,
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub run_id: i64,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub name: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsResponse {
    pub total_count: i64,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, Default)]
pub struct JobSummary {
    pub queued: i32,
    pub running: i32,
    pub completed: i32,
}

impl JobSummary {
    pub fn is_empty(&self) -> bool {
        self.queued == 0 && self.running == 0 && self.completed == 0
    }

    pub fn from_jobs(jobs: &[Job]) -> Self {
        let mut summary = Self::default();
        for job in jobs {
            if let Some(status) = &job.status {
                match status.to_lowercase().as_str() {
                    "queued" | "waiting" => summary.queued += 1,
                    "in_progress" => summary.running += 1,
                    "completed" => summary.completed += 1,
                    _ => {}
                }
            }
        }
        summary
    }
}

impl WorkflowRun {
    /// Returns a human-readable status string
    pub fn friendly_status(&self) -> String {
        if let Some(status) = &self.status {
            match status.to_lowercase().as_str() {
                "queued" => tr("Queued"),
                "in_progress" => tr("In Progress"),
                "completed" => {
                    if self.conclusion.is_some() {
                        self.friendly_conclusion()
                    } else {
                        tr("Completed")
                    }
                }
                "waiting" => tr("Waiting"),
                "requested" => tr("Requested"),
                "pending" => tr("Pending"),
                _ => status.replace('_', " "),
            }
        } else {
            tr("Unknown")
        }
    }

    /// Returns a human-readable conclusion string
    pub fn friendly_conclusion(&self) -> String {
        if let Some(conclusion) = &self.conclusion {
            match conclusion.to_lowercase().as_str() {
                "success" => tr("Success"),
                "failure" => tr("Failed"),
                "cancelled" => tr("Cancelled"),
                "skipped" => tr("Skipped"),
                "timed_out" => tr("Timed Out"),
                "action_required" => tr("Action Required"),
                "neutral" => tr("Neutral"),
                "stale" => tr("Stale"),
                _ => conclusion.replace('_', " "),
            }
        } else {
            String::new()
        }
    }

    /// Returns a relative time string like "2h ago", "Just now", etc.
    pub fn relative_time_string(&self) -> String {
        let timestamp = self
            .run_started_at
            .as_ref()
            .or(self.created_at.as_ref())
            .or(self.updated_at.as_ref());

        if let Some(ts) = timestamp {
            relative_time_from_iso(ts)
        } else {
            String::new()
        }
    }

    /// Check if this run is currently active (in progress/queued)
    pub fn is_active(&self) -> bool {
        if let Some(status) = &self.status {
            matches!(
                status.to_lowercase().as_str(),
                "queued" | "in_progress" | "waiting" | "requested" | "pending"
            )
        } else {
            false
        }
    }

    /// Check if this run can be cancelled
    pub fn is_cancellable(&self) -> bool {
        if let Some(status) = &self.status {
            matches!(
                status.to_lowercase().as_str(),
                "queued" | "in_progress" | "waiting"
            )
        } else {
            false
        }
    }

    /// Check if this run can be re-run
    pub fn is_rerunnable(&self) -> bool {
        if let Some(status) = &self.status {
            if status.to_lowercase() != "completed" {
                return false;
            }
        } else {
            return false;
        }

        if let Some(conclusion) = &self.conclusion {
            matches!(
                conclusion.to_lowercase().as_str(),
                "success" | "failure" | "cancelled"
            )
        } else {
            false
        }
    }

    /// Check if this run has failed jobs (can re-run failed only)
    pub fn has_failed_jobs(&self) -> bool {
        if let Some(status) = &self.status {
            if status.to_lowercase() != "completed" {
                return false;
            }
        } else {
            return false;
        }

        if let Some(conclusion) = &self.conclusion {
            conclusion.to_lowercase() == "failure"
        } else {
            false
        }
    }
}

/// Parse ISO 8601 timestamp and return relative time string
fn relative_time_from_iso(iso_string: &str) -> String {
    use chrono::{DateTime, Utc};

    // Try parsing the ISO string
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_string) {
        let now = Utc::now();
        let duration = now.signed_duration_since(dt.with_timezone(&Utc));

        let seconds = duration.num_seconds();

        if seconds < 60 {
            tr("Just now")
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            tr("{count}m ago").replace("{count}", minutes.to_string().as_str())
        } else if seconds < 86400 {
            let hours = seconds / 3600;
            tr("{count}h ago").replace("{count}", hours.to_string().as_str())
        } else if seconds < 604800 {
            let days = seconds / 86400;
            if days == 1 {
                tr("Yesterday")
            } else {
                tr("{count}d ago").replace("{count}", days.to_string().as_str())
            }
        } else {
            // Use numeric format to avoid locale-specific month abbreviations.
            dt.format("%Y-%m-%d").to_string()
        }
    } else {
        String::new()
    }
}

impl Job {
    /// Returns a human-readable status string
    pub fn friendly_status(&self) -> String {
        if let Some(status) = &self.status {
            match status.to_lowercase().as_str() {
                "queued" => tr("Queued"),
                "in_progress" => tr("In Progress"),
                "completed" => {
                    if self.conclusion.is_some() {
                        self.friendly_conclusion()
                    } else {
                        tr("Completed")
                    }
                }
                "waiting" => tr("Waiting"),
                "requested" => tr("Requested"),
                "pending" => tr("Pending"),
                _ => status.replace('_', " "),
            }
        } else {
            tr("Unknown")
        }
    }

    /// Returns a human-readable conclusion string
    pub fn friendly_conclusion(&self) -> String {
        if let Some(conclusion) = &self.conclusion {
            match conclusion.to_lowercase().as_str() {
                "success" => tr("Success"),
                "failure" => tr("Failed"),
                "cancelled" => tr("Cancelled"),
                "skipped" => tr("Skipped"),
                "timed_out" => tr("Timed Out"),
                "action_required" => tr("Action Required"),
                "neutral" => tr("Neutral"),
                _ => conclusion.replace('_', " "),
            }
        } else {
            String::new()
        }
    }

    /// Returns a formatted duration string (e.g., "2m 34s")
    pub fn duration_string(&self) -> Option<String> {
        let started = self.started_at.as_ref()?;
        let completed = self.completed_at.as_ref()?;

        // Parse ISO 8601 timestamps
        let start_time = chrono::DateTime::parse_from_rfc3339(started).ok()?;
        let end_time = chrono::DateTime::parse_from_rfc3339(completed).ok()?;

        let duration = end_time.signed_duration_since(start_time);
        let seconds = duration.num_seconds();

        if seconds < 60 {
            Some(format!("{}s", seconds))
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            let secs = seconds % 60;
            Some(format!("{}m {}s", minutes, secs))
        } else {
            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            Some(format!("{}h {}m", hours, minutes))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub limit: i64,
    pub remaining: i64,
    pub reset: i64, // Unix timestamp
}

impl RateLimitInfo {
    pub fn is_low(&self) -> bool {
        self.remaining < 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_deserialization() {
        let json = r#"{
            "id": 123,
            "name": "test-repo",
            "full_name": "owner/test-repo",
            "owner": {
                "login": "owner"
            },
            "private": false,
            "permissions": {
                "admin": true,
                "push": true,
                "pull": true
            }
        }"#;

        let repo: Repo = serde_json::from_str(json).unwrap();
        assert_eq!(repo.id, 123);
        assert_eq!(repo.name, "test-repo");
        assert_eq!(repo.owner.login, "owner");
    }

    #[test]
    fn test_workflow_deserialization() {
        let json = r#"{
            "id": 456,
            "name": "CI",
            "path": ".github/workflows/ci.yml"
        }"#;

        let workflow: Workflow = serde_json::from_str(json).unwrap();
        assert_eq!(workflow.id, 456);
        assert_eq!(workflow.name, "CI");
    }

    #[test]
    fn test_rate_limit_is_low() {
        let high = RateLimitInfo {
            limit: 5000,
            remaining: 4500,
            reset: 1234567890,
        };
        assert!(!high.is_low());

        let low = RateLimitInfo {
            limit: 5000,
            remaining: 50,
            reset: 1234567890,
        };
        assert!(low.is_low());
    }

    #[test]
    fn test_build_dispatch_inputs_payload() {
        let inputs = vec![
            WorkflowDispatchInput {
                name: "tag".to_string(),
                description: None,
                required: true,
                input_type: WorkflowDispatchInputType::String,
                default_value: None,
                options: Vec::new(),
            },
            WorkflowDispatchInput {
                name: "dry_run".to_string(),
                description: None,
                required: false,
                input_type: WorkflowDispatchInputType::Boolean,
                default_value: Some(WorkflowDispatchInputValue::Boolean(false)),
                options: Vec::new(),
            },
            WorkflowDispatchInput {
                name: "channel".to_string(),
                description: None,
                required: false,
                input_type: WorkflowDispatchInputType::Choice,
                default_value: Some(WorkflowDispatchInputValue::String("stable".to_string())),
                options: vec!["stable".to_string(), "beta".to_string()],
            },
        ];

        let mut values = HashMap::new();
        values.insert(
            "tag".to_string(),
            WorkflowDispatchInputValue::String("v1.0.5".to_string()),
        );
        values.insert(
            "dry_run".to_string(),
            WorkflowDispatchInputValue::Boolean(false),
        );
        values.insert(
            "channel".to_string(),
            WorkflowDispatchInputValue::String("stable".to_string()),
        );

        let payload = build_dispatch_inputs_payload(&inputs, &values)
            .unwrap()
            .unwrap();

        assert_eq!(payload["tag"], "v1.0.5");
        assert!(payload.get("dry_run").is_none());
        assert!(payload.get("channel").is_none());
    }

    #[test]
    fn test_build_dispatch_inputs_payload_requires_value() {
        let inputs = vec![WorkflowDispatchInput {
            name: "tag".to_string(),
            description: None,
            required: true,
            input_type: WorkflowDispatchInputType::String,
            default_value: None,
            options: Vec::new(),
        }];

        let mut values = HashMap::new();
        values.insert(
            "tag".to_string(),
            WorkflowDispatchInputValue::String("".to_string()),
        );

        let result = build_dispatch_inputs_payload(&inputs, &values);
        assert!(result.is_err());
    }
}
