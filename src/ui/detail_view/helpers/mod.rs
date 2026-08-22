mod context;
mod formatting;
mod jobs;
mod run_loader;
pub(in crate::ui::detail_view) mod runs;
mod status_dot;
mod workflow_follow_up;
mod workflows;

pub(crate) use context::{JobContextMap, RunBadgeSummaryMap, current_job_context_run_ids};
pub(crate) use jobs::refresh_jobs_for_workflows;
pub(crate) use run_loader::RunLoadService;
pub(crate) use runs::{LoadRunsParams, RunDigestStore, WorkflowRunListModel, set_expander_active};
pub(crate) use workflow_follow_up::clear_follow_up_refresh_timers;
pub(crate) use workflows::{
    WorkflowRowContext, WorkflowRowSettings, create_workflow_expander_row,
    update_workflow_row_header,
};

/// Status dot for a job, shared with the logs window's job sidebar so the two
/// views cannot drift apart on what a status looks like.
pub(crate) fn build_job_status_dot(job: &crate::api::models::Job) -> gtk4::Box {
    status_dot::build_status_dot(
        formatting::get_job_status_icon(job),
        formatting::get_job_status_class(job),
        status_dot::JOB_DOT_SIZE,
    )
}
