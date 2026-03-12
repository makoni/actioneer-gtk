use super::MainWindow;
use crate::ui::sidebar::{find_first_repo_index, find_repo_index, repo_from_object};
use gtk4::{self as gtk, glib, prelude::*};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SidebarSelectionPlan {
    index: Option<u32>,
    repo_id: Option<i64>,
}

impl MainWindow {
    pub(super) fn connect_search(&self) {
        let panel = self.sidebar_panel.clone();
        let window = self.clone();

        self.search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_lowercase();
            panel.update_filter_query(&text);
            window.restore_repo_selection_async();
        });
    }

    pub(super) fn connect_repo_selection(&self) {
        let window = self.clone();
        let selection = self.repo_selection.clone();

        selection.connect_selected_notify(move |sel| {
            let window = window.clone();
            let repo = sel.selected_item().and_then(|obj| repo_from_object(&obj));

            let current_selection = *window.selected_repo_id.lock();
            let new_selection = repo.as_ref().map(|r| r.id);

            if *window.handling_selection.lock() {
                info!("Already handling selection, ignoring signal");
                return;
            }

            info!(
                "Selection signal: current={:?}, new={:?}, repo={:?}",
                current_selection,
                new_selection,
                repo.as_ref().map(|r| r.full_name.as_str())
            );

            if new_selection.is_none() && window.active_detail.borrow().is_some() {
                info!("Ignoring transient deselection (detail pane is active)");
                return;
            }

            if current_selection != new_selection {
                window.handle_repo_selection(repo);
            }
        });
    }

    pub(super) fn connect_repo_activation(&self) {
        let window = self.clone();
        let selection = self.repo_selection.clone();
        let repo_view = self.sidebar_panel.repo_list();

        repo_view.connect_activate(move |_, position| {
            let Some(obj) = window.repo_filter_model.item(position) else {
                return;
            };
            let Some(repo) = repo_from_object(&obj) else {
                return;
            };

            let active_detail_repo_id = window
                .active_detail
                .borrow()
                .as_ref()
                .map(|pane| pane.repo().id);
            let action = determine_activation_action(
                selection.selected(),
                position,
                active_detail_repo_id,
                repo.id,
            );

            match action {
                RepoActivationAction::SelectOnly => {
                    selection.set_selected(position);
                }
                RepoActivationAction::HandleSelection => {
                    if selection.selected() != position {
                        selection.set_selected(position);
                    }
                    window.handle_repo_selection(Some(repo));
                }
                RepoActivationAction::None => {}
            }
        });
    }

    pub(super) fn restore_repo_selection_async(&self) {
        let window = self.clone();
        glib::idle_add_local_once(move || {
            window.restore_repo_selection_now();
        });
    }

    pub(super) fn restore_repo_selection_now(&self) {
        let target = *self.selected_repo_id.lock();

        *self.handling_selection.lock() = true;
        let restored_repo_id =
            restore_sidebar_selection(&self.repo_selection, &self.repo_filter_model, target);
        *self.selected_repo_id.lock() = restored_repo_id;
        *self.handling_selection.lock() = false;

        self.ensure_detail_matches_selection();
    }
}

pub(super) fn restore_sidebar_selection(
    selection: &gtk::SingleSelection,
    model: &gtk::FilterListModel,
    repo_id: Option<i64>,
) -> Option<i64> {
    let plan = build_sidebar_selection_plan(model, repo_id);

    if let Some(index) = plan.index {
        selection.set_selected(index);
    } else {
        selection.unselect_all();
    }

    plan.repo_id
}

fn build_sidebar_selection_plan(
    model: &gtk::FilterListModel,
    requested_repo_id: Option<i64>,
) -> SidebarSelectionPlan {
    let requested_index = requested_repo_id.and_then(|repo_id| find_repo_index(model, repo_id));
    let first_visible = first_visible_repo(model);
    determine_sidebar_selection_plan(requested_repo_id, requested_index, first_visible)
}

fn first_visible_repo(model: &gtk::FilterListModel) -> Option<(u32, i64)> {
    let index = find_first_repo_index(model)?;
    let repo_id = model
        .item(index)
        .and_then(|obj| repo_from_object(&obj))
        .map(|repo| repo.id)?;

    Some((index, repo_id))
}

fn determine_sidebar_selection_plan(
    requested_repo_id: Option<i64>,
    requested_index: Option<u32>,
    first_visible: Option<(u32, i64)>,
) -> SidebarSelectionPlan {
    if let (Some(repo_id), Some(index)) = (requested_repo_id, requested_index) {
        return SidebarSelectionPlan {
            index: Some(index),
            repo_id: Some(repo_id),
        };
    }

    if let Some((index, repo_id)) = first_visible {
        return SidebarSelectionPlan {
            index: Some(index),
            repo_id: Some(repo_id),
        };
    }

    SidebarSelectionPlan {
        index: None,
        repo_id: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepoActivationAction {
    None,
    SelectOnly,
    HandleSelection,
}

fn determine_activation_action(
    selected_position: u32,
    activated_position: u32,
    active_detail_repo_id: Option<i64>,
    activated_repo_id: i64,
) -> RepoActivationAction {
    if selected_position != activated_position {
        return RepoActivationAction::SelectOnly;
    }

    if active_detail_repo_id != Some(activated_repo_id) {
        return RepoActivationAction::HandleSelection;
    }

    RepoActivationAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_requested_repo_when_it_is_visible() {
        let plan = determine_sidebar_selection_plan(Some(42), Some(3), Some((0, 7)));
        assert_eq!(
            plan,
            SidebarSelectionPlan {
                index: Some(3),
                repo_id: Some(42),
            }
        );
    }

    #[test]
    fn falls_back_to_first_visible_repo_when_requested_repo_is_hidden() {
        let plan = determine_sidebar_selection_plan(Some(42), None, Some((0, 7)));
        assert_eq!(
            plan,
            SidebarSelectionPlan {
                index: Some(0),
                repo_id: Some(7),
            }
        );
    }

    #[test]
    fn clears_selection_when_no_visible_repo_exists() {
        let plan = determine_sidebar_selection_plan(Some(42), None, None);
        assert_eq!(
            plan,
            SidebarSelectionPlan {
                index: None,
                repo_id: None,
            }
        );
    }

    #[test]
    fn activation_selects_row_when_user_activates_a_different_position() {
        assert_eq!(
            determine_activation_action(2, 5, Some(42), 7),
            RepoActivationAction::SelectOnly
        );
    }

    #[test]
    fn activation_handles_selected_row_when_detail_pane_is_out_of_sync() {
        assert_eq!(
            determine_activation_action(3, 3, Some(42), 7),
            RepoActivationAction::HandleSelection
        );
    }

    #[test]
    fn activation_is_noop_when_selected_row_already_matches_detail_pane() {
        assert_eq!(
            determine_activation_action(3, 3, Some(7), 7),
            RepoActivationAction::None
        );
    }
}
