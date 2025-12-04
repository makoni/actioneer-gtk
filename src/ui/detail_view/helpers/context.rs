use crate::api::GitHubClient;
use crate::api::models::Repo;
use gtk4::{self as gtk};
use parking_lot::Mutex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Shared state for refreshing job lists when background tasks update.
#[derive(Clone)]
pub(crate) struct JobRefreshContext {
    client: Arc<Mutex<GitHubClient>>,
    owner: String,
    repo: String,
    workflow_id: i64,
    run_id: i64,
    jobs_box: gtk::Box,
    badges_box: Option<gtk::Box>,
    parent_window: gtk::Window,
    repo_model: Repo,
    branch: Option<String>,
    run_title: String,
}

pub(crate) struct JobRefreshContextParams {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub workflow_id: i64,
    pub run_id: i64,
    pub jobs_box: gtk::Box,
    pub badges_box: Option<gtk::Box>,
    pub parent_window: gtk::Window,
    pub repo_model: Repo,
    pub branch: Option<String>,
    pub run_title: String,
}

impl JobRefreshContext {
    pub(crate) fn from_params(params: JobRefreshContextParams) -> Self {
        Self {
            client: params.client,
            owner: params.owner,
            repo: params.repo,
            workflow_id: params.workflow_id,
            run_id: params.run_id,
            jobs_box: params.jobs_box,
            badges_box: params.badges_box,
            parent_window: params.parent_window,
            repo_model: params.repo_model,
            branch: params.branch,
            run_title: params.run_title,
        }
    }

    pub(crate) fn workflow_id(&self) -> i64 {
        self.workflow_id
    }

    pub(crate) fn run_id(&self) -> i64 {
        self.run_id
    }

    pub(crate) fn client(&self) -> Arc<Mutex<GitHubClient>> {
        self.client.clone()
    }

    pub(crate) fn owner(&self) -> String {
        self.owner.clone()
    }

    pub(crate) fn repo(&self) -> String {
        self.repo.clone()
    }

    pub(crate) fn jobs_box(&self) -> gtk::Box {
        self.jobs_box.clone()
    }

    pub(crate) fn badges_box(&self) -> Option<gtk::Box> {
        self.badges_box.clone()
    }

    pub(crate) fn parent_window(&self) -> gtk::Window {
        self.parent_window.clone()
    }

    pub(crate) fn repo_model(&self) -> Repo {
        self.repo_model.clone()
    }

    pub(crate) fn branch(&self) -> Option<String> {
        self.branch.clone()
    }

    pub(crate) fn run_title(&self) -> String {
        self.run_title.clone()
    }
}

pub(crate) type JobContextMap = Rc<RefCell<HashMap<i64, JobRefreshContext>>>;

pub(crate) fn take_job_context_run_ids(job_contexts: &JobContextMap, workflow_id: i64) -> Vec<i64> {
    let mut guard = job_contexts.borrow_mut();
    let run_ids: Vec<i64> = guard
        .iter()
        .filter_map(|(&run_id, ctx)| {
            if ctx.workflow_id() == workflow_id {
                Some(run_id)
            } else {
                None
            }
        })
        .collect();

    for run_id in &run_ids {
        guard.remove(run_id);
    }

    run_ids
}

pub(crate) fn current_job_context_run_ids(
    job_contexts: &JobContextMap,
    workflow_id: i64,
) -> Vec<i64> {
    let guard = job_contexts.borrow();
    guard
        .iter()
        .filter_map(|(&run_id, ctx)| {
            if ctx.workflow_id() == workflow_id {
                Some(run_id)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::User;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn take_job_context_run_ids_removes_entries_for_workflow() {
        let Some(_guard) = gtk_test_guard("take_job_context_run_ids_removes_entries_for_workflow")
        else {
            return;
        };

        let client = Arc::new(Mutex::new(GitHubClient::new(None).unwrap()));
        let jobs_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let parent_window = gtk::Window::builder().build();
        let repo_model = Repo {
            id: 1,
            name: "repo".to_string(),
            full_name: "owner/repo".to_string(),
            owner: User {
                login: "owner".to_string(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".to_string()),
        };

        let context_one = JobRefreshContext::from_params(JobRefreshContextParams {
            client: client.clone(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            workflow_id: 42,
            run_id: 1,
            jobs_box: jobs_box.clone(),
            badges_box: None,
            parent_window: parent_window.clone(),
            repo_model: repo_model.clone(),
            branch: Some("main".to_string()),
            run_title: "Run One".to_string(),
        });
        let context_two = JobRefreshContext::from_params(JobRefreshContextParams {
            client: client.clone(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            workflow_id: 42,
            run_id: 2,
            jobs_box: jobs_box.clone(),
            badges_box: None,
            parent_window: parent_window.clone(),
            repo_model: repo_model.clone(),
            branch: Some("feature".to_string()),
            run_title: "Run Two".to_string(),
        });
        let context_other = JobRefreshContext::from_params(JobRefreshContextParams {
            client: client.clone(),
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            workflow_id: 7,
            run_id: 99,
            jobs_box: jobs_box.clone(),
            badges_box: None,
            parent_window: parent_window.clone(),
            repo_model: repo_model.clone(),
            branch: None,
            run_title: "Other".to_string(),
        });

        let job_contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        {
            let mut guard = job_contexts.borrow_mut();
            guard.insert(1, context_one);
            guard.insert(2, context_two);
            guard.insert(99, context_other);
        }

        let removed_ids = take_job_context_run_ids(&job_contexts, 42);
        assert_eq!(removed_ids.len(), 2);
        assert!(removed_ids.contains(&1));
        assert!(removed_ids.contains(&2));

        let guard = job_contexts.borrow();
        assert!(!guard.contains_key(&1));
        assert!(!guard.contains_key(&2));
        assert!(guard.contains_key(&99));
    }
}
