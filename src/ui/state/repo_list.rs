//! The repository-status value types now live in `domain::counts`; this
//! re-export keeps the `ui::state::…` paths their consumers use resolving.

pub use crate::domain::counts::{RepoActionsState, WorkflowStatusCounts};
