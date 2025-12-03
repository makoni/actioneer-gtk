use serde::{Deserialize, Serialize};

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
                "queued" => "Queued".to_string(),
                "in_progress" => "In Progress".to_string(),
                "completed" => {
                    if self.conclusion.is_some() {
                        self.friendly_conclusion()
                    } else {
                        "Completed".to_string()
                    }
                }
                "waiting" => "Waiting".to_string(),
                "requested" => "Requested".to_string(),
                "pending" => "Pending".to_string(),
                _ => status.replace('_', " "),
            }
        } else {
            "Unknown".to_string()
        }
    }

    /// Returns a human-readable conclusion string
    pub fn friendly_conclusion(&self) -> String {
        if let Some(conclusion) = &self.conclusion {
            match conclusion.to_lowercase().as_str() {
                "success" => "Success".to_string(),
                "failure" => "Failed".to_string(),
                "cancelled" => "Cancelled".to_string(),
                "skipped" => "Skipped".to_string(),
                "timed_out" => "Timed Out".to_string(),
                "action_required" => "Action Required".to_string(),
                "neutral" => "Neutral".to_string(),
                "stale" => "Stale".to_string(),
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
            "Just now".to_string()
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            format!("{}m ago", minutes)
        } else if seconds < 86400 {
            let hours = seconds / 3600;
            format!("{}h ago", hours)
        } else if seconds < 604800 {
            let days = seconds / 86400;
            if days == 1 {
                "Yesterday".to_string()
            } else {
                format!("{}d ago", days)
            }
        } else {
            // For older dates, use date formatting
            dt.format("%b %d").to_string()
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
                "queued" => "Queued".to_string(),
                "in_progress" => "In Progress".to_string(),
                "completed" => {
                    if self.conclusion.is_some() {
                        self.friendly_conclusion()
                    } else {
                        "Completed".to_string()
                    }
                }
                "waiting" => "Waiting".to_string(),
                "requested" => "Requested".to_string(),
                "pending" => "Pending".to_string(),
                _ => status.replace('_', " "),
            }
        } else {
            "Unknown".to_string()
        }
    }

    /// Returns a human-readable conclusion string
    pub fn friendly_conclusion(&self) -> String {
        if let Some(conclusion) = &self.conclusion {
            match conclusion.to_lowercase().as_str() {
                "success" => "Success".to_string(),
                "failure" => "Failed".to_string(),
                "cancelled" => "Cancelled".to_string(),
                "skipped" => "Skipped".to_string(),
                "timed_out" => "Timed Out".to_string(),
                "action_required" => "Action Required".to_string(),
                "neutral" => "Neutral".to_string(),
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
}
