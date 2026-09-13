/// Repository actions state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoActionsState {
    Enabled,
    Disabled,
    Unknown,
}

/// Workflow status counts for a repository
#[derive(Clone, Debug, Default)]
pub struct WorkflowStatusCounts {
    pub active: usize,
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_counts_start_at_zero() {
        let counts = WorkflowStatusCounts::default();
        assert_eq!((counts.active, counts.failed), (0, 0));
    }

    #[test]
    fn status_counts_carry_what_they_are_given() {
        let counts = WorkflowStatusCounts {
            active: 3,
            failed: 2,
        };
        assert_eq!((counts.active, counts.failed), (3, 2));
    }

    #[test]
    fn actions_state_variants_are_distinct() {
        assert_ne!(RepoActionsState::Enabled, RepoActionsState::Disabled);
        assert_ne!(RepoActionsState::Enabled, RepoActionsState::Unknown);
        assert_ne!(RepoActionsState::Disabled, RepoActionsState::Unknown);
        assert_eq!(RepoActionsState::Enabled, RepoActionsState::Enabled);
    }
}
