use super::MainWindow;
use crate::ui::sidebar::{find_first_repo_index, find_repo_index, repo_from_object};
use gtk4::{self as gtk, glib, prelude::*};
use tracing::info;

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

    pub(super) fn restore_repo_selection_async(&self) {
        let selection = self.repo_selection.clone();
        let model = self.repo_filter_model.clone();
        let target = *self.selected_repo_id.lock();

        glib::idle_add_local_once(move || {
            restore_sidebar_selection(&selection, &model, target);
        });
    }
}

pub(super) fn restore_sidebar_selection(
    selection: &gtk::SingleSelection,
    model: &gtk::FilterListModel,
    repo_id: Option<i64>,
) {
    if let Some(repo_id) = repo_id {
        if let Some(index) = find_repo_index(model, repo_id) {
            selection.set_selected(index);
            return;
        }
    } else if let Some(index) = find_first_repo_index(model) {
        selection.set_selected(index);
        return;
    }

    selection.unselect_all();
}
