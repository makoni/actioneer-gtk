mod data;
mod state;

pub(crate) use state::{
    cancel_run, disable, dispatch_workflow, enable, is_actions_enabled, is_active, job_logs,
    list_branches, list_jobs, list_repos, list_runs, list_workflows, rate_limit_info,
    rerun_failed_jobs, rerun_workflow,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_enable_populates_repos() {
        let repos = enable();
        assert!(!repos.is_empty());
        assert!(is_active());
        disable();
        assert!(!is_active());
    }
}
