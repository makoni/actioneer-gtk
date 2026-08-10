use crate::i18n::tr;
use crate::ui::sidebar::{
    SidebarFilter, row_activatable_from_object, row_matches_filter, row_selectable_from_object,
};
use crate::ui::utils::create_sidebar_clamp;
use gtk4::prelude::*;
use gtk4::{self as gtk, gio};
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct SidebarPanel {
    clamp: adw::Clamp,
    search_entry: gtk::SearchEntry,
    repo_view: gtk::ListView,
    repo_store: gio::ListStore,
    filter_model: gtk::FilterListModel,
    selection: gtk::SingleSelection,
    filter: gtk::CustomFilter,
    filter_query: Rc<RefCell<String>>,
    #[allow(dead_code)] // read by tests
    filter_mode: Rc<Cell<SidebarFilter>>,
}

impl SidebarPanel {
    pub fn new() -> Self {
        let repo_store = gio::ListStore::new::<gtk::Widget>();

        let filter_query = Rc::new(RefCell::new(String::new()));
        let filter_mode = Rc::new(Cell::new(SidebarFilter::All));
        let filter = gtk::CustomFilter::new({
            let query = filter_query.clone();
            let mode = filter_mode.clone();
            move |obj| {
                let Some(row) = obj.downcast_ref::<gtk::Widget>() else {
                    return true;
                };
                row_matches_filter(row, &query.borrow(), mode.get())
            }
        });

        let filter_model =
            gtk::FilterListModel::new(Some(repo_store.clone()), Some(filter.clone()));
        let selection = gtk::SingleSelection::builder()
            .model(&filter_model)
            .can_unselect(true)
            .build();

        selection.connect_selection_changed(|sel, _, _| {
            if let Some(item) = sel.selected_item()
                && !row_selectable_from_object(item.as_ref())
            {
                sel.set_selected(gtk::INVALID_LIST_POSITION);
            }
        });

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_bind(|_, list_item| {
            let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(row) = list_item
                .item()
                .and_then(|obj| obj.downcast::<gtk::Widget>().ok())
            else {
                return;
            };
            row.unparent();
            list_item.set_child(Some(&row));
            list_item.set_selectable(row_selectable_from_object(row.as_ref()));
            list_item.set_activatable(row_activatable_from_object(row.as_ref()));
        });
        factory.connect_unbind(|_, list_item| {
            let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            if let Some(child) = list_item.child() {
                child.unparent();
                list_item.set_child(None::<&gtk::Widget>);
            }
        });

        let repo_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
        repo_view.set_margin_top(0);
        repo_view.set_margin_bottom(12);
        repo_view.set_margin_start(12);
        repo_view.set_margin_end(12);
        repo_view.set_accessible_role(gtk::AccessibleRole::List);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some(tr("Search repositories...").as_str()));
        search_entry.set_margin_top(12);
        search_entry.set_margin_bottom(10);
        search_entry.set_margin_start(12);
        search_entry.set_margin_end(12);

        let pills = build_filter_pills(&filter, &filter_mode, &selection);
        pills.set_margin_start(12);
        pills.set_margin_end(12);
        pills.set_margin_bottom(10);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&repo_view));

        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&search_entry);
        sidebar_box.append(&pills);
        sidebar_box.append(&scrolled);
        sidebar_box.set_hexpand(false);
        sidebar_box.set_vexpand(true);

        let viewport = gtk::Viewport::builder()
            .scroll_to_focus(true)
            .hexpand(false)
            .vexpand(true)
            .build();
        viewport.set_child(Some(&sidebar_box));

        let clamp = create_sidebar_clamp(&viewport);

        Self {
            clamp,
            search_entry,
            repo_view,
            repo_store,
            filter_model,
            selection,
            filter,
            filter_query,
            filter_mode,
        }
    }

    pub fn clamp(&self) -> adw::Clamp {
        self.clamp.clone()
    }

    pub fn search_entry(&self) -> gtk::SearchEntry {
        self.search_entry.clone()
    }

    pub fn repo_list(&self) -> gtk::ListView {
        self.repo_view.clone()
    }

    pub fn repo_store(&self) -> gio::ListStore {
        self.repo_store.clone()
    }

    pub fn filter_model(&self) -> gtk::FilterListModel {
        self.filter_model.clone()
    }

    pub fn selection(&self) -> gtk::SingleSelection {
        self.selection.clone()
    }

    pub fn update_filter_query(&self, query: &str) {
        *self.filter_query.borrow_mut() = query.to_lowercase();
        self.filter.changed(gtk::FilterChange::Different);
    }
}

/// The All / Favorites / Active pill row above the repo list.
fn build_filter_pills(
    filter: &gtk::CustomFilter,
    mode: &Rc<Cell<SidebarFilter>>,
    selection: &gtk::SingleSelection,
) -> gtk::Box {
    let pills = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    let all = create_pill(tr("All").as_str());
    let favorites = create_pill(tr("Favorites").as_str());
    let active = create_pill(tr("Active").as_str());

    favorites.set_group(Some(&all));
    active.set_group(Some(&all));
    all.set_active(true);

    for (pill, pill_mode) in [
        (&all, SidebarFilter::All),
        (&favorites, SidebarFilter::Favorites),
        (&active, SidebarFilter::Active),
    ] {
        let filter = filter.clone();
        let mode = mode.clone();
        let selection = selection.clone();
        pill.connect_toggled(move |btn| {
            if btn.is_active() {
                mode.set(pill_mode);
                // Drop the current selection: the filtered row set changes and a
                // stale index could paint selection styling onto a header row.
                selection.set_selected(gtk::INVALID_LIST_POSITION);
                filter.changed(gtk::FilterChange::Different);
            }
        });
    }

    pills.append(&all);
    pills.append(&favorites);
    pills.append(&active);
    pills
}

fn create_pill(label: &str) -> gtk::ToggleButton {
    let pill = gtk::ToggleButton::with_label(label);
    pill.add_css_class("filter-pill");
    pill.add_css_class("flat");
    pill
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn sidebar_panel_initializes_widgets() {
        let Some(_guard) = gtk_test_guard("sidebar_panel_initializes_widgets") else {
            return;
        };
        let panel = SidebarPanel::new();
        let expected_placeholder = tr("Search repositories...");
        assert_eq!(
            panel
                .search_entry()
                .placeholder_text()
                .as_ref()
                .map(|s| s.as_str()),
            Some(expected_placeholder.as_str())
        );
        assert_eq!(
            panel.repo_list().accessible_role(),
            gtk::AccessibleRole::List
        );
        assert_eq!(panel.filter_mode.get(), SidebarFilter::All);
    }
}
