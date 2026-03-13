use crate::api::GitHubClient;
use crate::api::models::{Job, JobSummary, Repo};
use gtk4::prelude::*;
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
    expander: gtk::Expander,
    jobs_box: gtk::Box,
    badges_box: Option<gtk::Box>,
    parent_window: gtk::Window,
    repo_model: Repo,
    branch: Option<String>,
    run_title: String,
    jobs: Arc<Vec<Job>>,
    job_summaries: RunBadgeSummaryMap,
}

pub(crate) struct JobRefreshContextParams {
    pub client: Arc<Mutex<GitHubClient>>,
    pub owner: String,
    pub repo: String,
    pub workflow_id: i64,
    pub run_id: i64,
    pub expander: gtk::Expander,
    pub jobs_box: gtk::Box,
    pub badges_box: Option<gtk::Box>,
    pub parent_window: gtk::Window,
    pub repo_model: Repo,
    pub branch: Option<String>,
    pub run_title: String,
    pub jobs: Arc<Vec<Job>>,
    pub job_summaries: RunBadgeSummaryMap,
}

impl JobRefreshContext {
    pub(crate) fn from_params(params: JobRefreshContextParams) -> Self {
        Self {
            client: params.client,
            owner: params.owner,
            repo: params.repo,
            workflow_id: params.workflow_id,
            run_id: params.run_id,
            expander: params.expander,
            jobs_box: params.jobs_box,
            badges_box: params.badges_box,
            parent_window: params.parent_window,
            repo_model: params.repo_model,
            branch: params.branch,
            run_title: params.run_title,
            jobs: params.jobs,
            job_summaries: params.job_summaries,
        }
    }

    pub(crate) fn workflow_id(&self) -> i64 {
        self.workflow_id
    }

    pub(crate) fn run_id(&self) -> i64 {
        self.run_id
    }

    pub(crate) fn expander(&self) -> gtk::Expander {
        self.expander.clone()
    }

    pub(crate) fn matches_expander(&self, expander: &gtk::Expander) -> bool {
        self.expander == *expander
    }

    pub(crate) fn should_preserve_expansion(&self) -> bool {
        self.expander.is_expanded()
            && self.expander.parent().is_some()
            && self.expander.root().is_some()
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

    pub(crate) fn jobs(&self) -> Arc<Vec<Job>> {
        self.jobs.clone()
    }

    pub(crate) fn job_summaries(&self) -> RunBadgeSummaryMap {
        self.job_summaries.clone()
    }
}

pub(crate) type JobContextMap = Rc<RefCell<HashMap<i64, JobRefreshContext>>>;
pub(crate) type RunBadgeSummaryMap = Rc<RefCell<HashMap<i64, JobSummary>>>;

pub(crate) fn current_job_context_run_ids(
    job_contexts: &JobContextMap,
    workflow_id: i64,
) -> Vec<i64> {
    let guard = job_contexts.borrow();
    guard
        .iter()
        .filter_map(|(&run_id, ctx)| {
            if ctx.workflow_id() == workflow_id && ctx.should_preserve_expansion() {
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
    use crate::api::GitHubClient;
    use crate::api::models::{Repo, User};
    use crate::ui::test_helpers::gtk_test_guard;
    use parking_lot::Mutex;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Arc;

    fn repo_stub() -> Repo {
        Repo {
            id: 1,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        }
    }

    fn client_stub() -> Arc<Mutex<GitHubClient>> {
        Arc::new(Mutex::new(
            GitHubClient::new(None).expect("client stub should build"),
        ))
    }

    fn context_for(expander: &gtk::Expander, run_id: i64, workflow_id: i64) -> JobRefreshContext {
        JobRefreshContext::from_params(JobRefreshContextParams {
            client: client_stub(),
            owner: "mak".into(),
            repo: "actioneer".into(),
            workflow_id,
            run_id,
            expander: expander.clone(),
            jobs_box: gtk::Box::new(gtk::Orientation::Vertical, 0),
            badges_box: None,
            parent_window: gtk::Window::new(),
            repo_model: repo_stub(),
            branch: Some("main".into()),
            run_title: "CI".into(),
            jobs: Arc::new(Vec::new()),
            job_summaries: Rc::new(RefCell::new(HashMap::new())),
        })
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn current_job_context_run_ids_only_preserve_expanded_contexts() {
        let Some(_guard) =
            gtk_test_guard("current_job_context_run_ids_only_preserve_expanded_contexts")
        else {
            return;
        };

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let expanded = gtk::Expander::new(None);
        expanded.set_expanded(true);
        root.append(&expanded);

        let collapsed = gtk::Expander::new(None);
        collapsed.set_expanded(false);
        root.append(&collapsed);

        let contexts: JobContextMap = Rc::new(RefCell::new(HashMap::new()));
        contexts
            .borrow_mut()
            .insert(11, context_for(&expanded, 11, 7));
        contexts
            .borrow_mut()
            .insert(22, context_for(&collapsed, 22, 7));

        assert_eq!(current_job_context_run_ids(&contexts, 7), vec![11]);
    }
}
