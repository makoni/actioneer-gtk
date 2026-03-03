use crate::ui::detail_view::helpers::WorkflowRunListModel;
use crate::ui::utils::widget_data::get_data_clone;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::collections::HashSet;

pub(super) fn parse_expander_widget_name(name: &str) -> Option<(i64, bool)> {
    let (base, is_active) = if let Some(stripped) = name.strip_suffix("_ACTIVE") {
        (stripped, true)
    } else {
        (name, false)
    };

    let workflow_id = base.strip_prefix("workflow_")?.parse::<i64>().ok()?;

    Some((workflow_id, is_active))
}

pub(super) fn run_list_for_expander(expander: &gtk::Expander) -> Option<WorkflowRunListModel> {
    get_data_clone(expander, "actioneer-run-list")
}

pub(super) fn visit_expanders<F: FnMut(&gtk::Expander, i64, bool)>(
    widget: &gtk::Widget,
    f: &mut F,
) {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && let Some((workflow_id, is_active)) =
            parse_expander_widget_name(expander.widget_name().as_str())
    {
        f(expander, workflow_id, is_active);
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        visit_expanders(&current, f);
        child = current.next_sibling();
    }
}

pub(super) fn union_active_workflows(
    observed_active: &HashSet<i64>,
    previous_active: &HashSet<i64>,
) -> HashSet<i64> {
    let mut combined = observed_active.clone();
    combined.extend(previous_active.iter().copied());
    combined
}

#[cfg(test)]
mod tests {
    use super::{parse_expander_widget_name, union_active_workflows};
    use std::collections::HashSet;

    #[test]
    fn parses_active_expander_names() {
        let result = parse_expander_widget_name("workflow_123_ACTIVE");
        assert_eq!(result, Some((123, true)));
    }

    #[test]
    fn parses_inactive_expander_names() {
        let result = parse_expander_widget_name("workflow_5");
        assert_eq!(result, Some((5, false)));
    }

    #[test]
    fn rejects_invalid_prefixes() {
        assert_eq!(parse_expander_widget_name("foo"), None);
    }

    #[test]
    fn rejects_non_numeric_ids() {
        assert_eq!(parse_expander_widget_name("workflow_bar"), None);
    }

    #[test]
    fn union_active_workflows_preserves_previous_ids() {
        let observed = HashSet::from([1, 2]);
        let previous = HashSet::from([2, 3]);

        let combined = union_active_workflows(&observed, &previous);

        let expected = HashSet::from([1, 2, 3]);
        assert_eq!(combined, expected);
    }

    #[test]
    fn union_active_workflows_handles_empty_observed() {
        let observed = HashSet::new();
        let previous = HashSet::from([5]);

        let combined = union_active_workflows(&observed, &previous);

        let expected = HashSet::from([5]);
        assert_eq!(combined, expected);
    }
}
