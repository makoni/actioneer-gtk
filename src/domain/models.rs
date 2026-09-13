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
    #[serde(default)]
    pub workflow_id: Option<i64>,
    pub name: Option<String>,
    pub display_title: Option<String>,
    pub head_branch: Option<String>,
    pub head_commit: Option<RunHeadCommit>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub run_started_at: Option<String>,
    pub event: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub html_url: Option<String>,
    #[serde(default)]
    pub actor: Option<User>,
    #[serde(default)]
    pub triggering_actor: Option<User>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHeadCommit {
    pub id: String,
    pub message: String,
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
    #[serde(default)]
    pub steps: Vec<JobStep>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStep {
    pub name: Option<String>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub number: Option<i64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
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
fn duration_string_from_bounds(
    started: Option<&String>,
    completed: Option<&String>,
) -> Option<String> {
    let started = started?;
    let completed = completed?;

    let start_time = chrono::DateTime::parse_from_rfc3339(started).ok()?;
    let end_time = chrono::DateTime::parse_from_rfc3339(completed).ok()?;

    let duration = end_time.signed_duration_since(start_time);
    let seconds = duration.num_seconds().max(0);

    Some(super::formatting::format_elapsed(seconds))
}

impl Job {
    /// Returns a formatted duration string (e.g., "12:34")
    pub fn duration_string(&self) -> Option<String> {
        duration_string_from_bounds(self.started_at.as_ref(), self.completed_at.as_ref())
    }
}

impl WorkflowRun {
    /// Returns a formatted duration for the workflow run, if both timestamps are present.
    pub fn run_duration_string(&self) -> Option<String> {
        duration_string_from_bounds(self.run_started_at.as_ref(), self.updated_at.as_ref())
    }

    /// Returns the commit message for the run, truncated to the first line.
    pub fn commit_message_headline(&self) -> Option<String> {
        self.head_commit
            .as_ref()
            .map(|commit| commit.message.lines().next().unwrap_or("").to_string())
            .filter(|message| !message.is_empty())
    }

    /// Returns the login of the actor who triggered or re-triggered the run.
    pub fn actor_login(&self) -> Option<String> {
        self.triggering_actor
            .as_ref()
            .or(self.actor.as_ref())
            .map(|user| user.login.clone())
    }
}

impl JobStep {
    pub fn duration_string(&self) -> Option<String> {
        duration_string_from_bounds(self.started_at.as_ref(), self.completed_at.as_ref())
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
    fn test_repo_deserialization_without_deprecated_repository_fields() {
        let json = r#"{
            "id": 124,
            "name": "trimmed-repo",
            "full_name": "owner/trimmed-repo",
            "owner": { "login": "owner" },
            "private": false,
            "permissions": { "admin": false, "push": true, "pull": true },
            "default_branch": "main"
        }"#;

        let repo: Repo = serde_json::from_str(json).expect("repo");
        assert_eq!(repo.id, 124);
        assert_eq!(repo.default_branch.as_deref(), Some("main"));
        assert!(!repo.is_private);
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
    fn test_workflow_run_deserialization_with_optional_workflow_id() {
        let json = r#"{
            "id": 789,
            "run_number": 12,
            "workflow_id": 456,
            "status": "completed",
            "conclusion": "success"
        }"#;

        let run: WorkflowRun = serde_json::from_str(json).unwrap();
        assert_eq!(run.id, 789);
        assert_eq!(run.workflow_id, Some(456));

        let json_without_workflow_id = r#"{
            "id": 790,
            "run_number": 13,
            "status": "queued"
        }"#;
        let run_without_workflow_id: WorkflowRun =
            serde_json::from_str(json_without_workflow_id).unwrap();
        assert_eq!(run_without_workflow_id.workflow_id, None);
    }

    #[test]
    fn test_job_deserialization_includes_steps() {
        let json = r#"{
            "id": 1,
            "run_id": 2,
            "status": "in_progress",
            "name": "build",
            "steps": [
                {
                    "name": "Checkout",
                    "status": "completed",
                    "conclusion": "success",
                    "number": 1,
                    "started_at": "2024-01-01T00:00:00Z",
                    "completed_at": "2024-01-01T00:00:10Z"
                },
                {
                    "name": "Test",
                    "status": "in_progress",
                    "conclusion": null,
                    "number": 2,
                    "started_at": "2024-01-01T00:00:10Z",
                    "completed_at": null
                }
            ]
        }"#;

        let job: Job = serde_json::from_str(json).unwrap();
        assert_eq!(job.steps.len(), 2);
        assert_eq!(job.steps[0].name.as_deref(), Some("Checkout"));
        assert_eq!(job.steps[1].status.as_deref(), Some("in_progress"));
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

    #[test]
    fn test_run_duration_string_is_minutes_seconds() {
        let mut run = WorkflowRun {
            id: 1,
            run_number: Some(1),
            workflow_id: None,
            name: None,
            display_title: None,
            head_branch: None,
            head_commit: None,
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            run_started_at: Some("2024-01-01T00:00:00Z".into()),
            event: None,
            created_at: None,
            updated_at: Some("2024-01-01T00:00:45Z".into()),
            html_url: None,
            actor: None,
            triggering_actor: None,
        };

        // The final duration matches the live ticker's shape, so no separate
        // locale-aware units are needed.
        assert_eq!(run.run_duration_string(), Some("00:45".into()));

        run.updated_at = Some("2024-01-01T01:01:01Z".into());
        assert_eq!(run.run_duration_string(), Some("1:01:01".into()));

        run.run_started_at = None;
        assert_eq!(run.run_duration_string(), None);
    }
}
