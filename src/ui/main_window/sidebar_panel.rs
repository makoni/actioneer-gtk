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

const STACK_LIST: &str = "list";
const STACK_EMPTY: &str = "empty";

/// Callback fired after a pill filter changes (used to restore the selection).
type FilterChangedHandler = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

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
    filter_changed: FilterChangedHandler,
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
            // Autoselect defaults to true, which makes `set_selected(INVALID)` a
            // no-op and lets GTK pick a replacement row — a section header, or a
            // different repo — synchronously inside `filter.changed()`. The
            // sidebar drives selection explicitly, so it must stay off.
            .autoselect(false)
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

        let filter_changed: FilterChangedHandler = Rc::new(RefCell::new(None));
        let pills = build_filter_pills(&filter, &filter_mode, &filter_changed);
        pills.set_margin_start(12);
        pills.set_margin_end(12);
        pills.set_margin_bottom(10);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&repo_view));

        // A filter can legitimately hide every row (no favourites yet, nothing
        // running). Without this the sidebar just goes blank and reads as broken.
        let empty_state = build_empty_state();
        let list_stack = gtk::Stack::new();
        list_stack.add_named(&scrolled, Some(STACK_LIST));
        list_stack.add_named(&empty_state, Some(STACK_EMPTY));
        list_stack.set_visible_child_name(STACK_LIST);
        list_stack.set_vexpand(true);

        // Both models have to be watched, and only weakly. GtkFilterListModel
        // stays silent when the source changes but the visible set is empty
        // either side of it — which is exactly the rebuild path — so the store
        // is watched too. Strong captures here would pin the model that owns the
        // handler (and through the list view, the window).
        {
            let stack = list_stack.downgrade();
            let store = repo_store.downgrade();
            filter_model.connect_items_changed(move |model, _, _, _| {
                let (Some(stack), Some(store)) = (stack.upgrade(), store.upgrade()) else {
                    return;
                };
                refresh_empty_state(&stack, model, &store);
            });
        }
        {
            let stack = list_stack.downgrade();
            let filtered = filter_model.downgrade();
            repo_store.connect_items_changed(move |store, _, _, _| {
                let (Some(stack), Some(filtered)) = (stack.upgrade(), filtered.upgrade()) else {
                    return;
                };
                refresh_empty_state(&stack, &filtered, store);
            });
        }
        refresh_empty_state(&list_stack, &filter_model, &repo_store);

        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&search_entry);
        sidebar_box.append(&pills);
        sidebar_box.append(&list_stack);
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
            filter_changed,
        }
    }

    /// Registers a callback fired after a pill filter changes, so the window can
    /// re-apply the selection for the repo that is still open in the detail pane.
    pub fn connect_filter_changed<F: Fn() + 'static>(&self, callback: F) {
        *self.filter_changed.borrow_mut() = Some(Rc::new(callback));
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

/// Switches between the repo list and the empty state. Showing the list while
/// no repositories have loaded yet keeps the empty state from flashing at
/// startup.
fn refresh_empty_state(
    stack: &gtk::Stack,
    filtered: &gtk::FilterListModel,
    store: &gio::ListStore,
) {
    let has_rows = filtered.n_items() > 0;
    let has_repos = store.n_items() > 0;
    stack.set_visible_child_name(if has_rows || !has_repos {
        STACK_LIST
    } else {
        STACK_EMPTY
    });
}

/// Shown when the active filter matches no repository.
fn build_empty_state() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_valign(gtk::Align::Center);
    container.set_halign(gtk::Align::Center);
    container.set_margin_start(18);
    container.set_margin_end(18);

    let title = gtk::Label::new(Some(tr("No repositories match").as_str()));
    title.add_css_class("dim-label");
    title.set_wrap(true);
    title.set_justify(gtk::Justification::Center);
    container.append(&title);

    let hint = gtk::Label::new(Some(tr("Try another filter or search term.").as_str()));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    container.append(&hint);

    container
}

/// The All / Favorites / Active pill row above the repo list.
fn build_filter_pills(
    filter: &gtk::CustomFilter,
    mode: &Rc<Cell<SidebarFilter>>,
    filter_changed: &FilterChangedHandler,
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
        let filter_changed = filter_changed.clone();
        pill.connect_toggled(move |btn| {
            if btn.is_active() {
                mode.set(pill_mode);
                filter.changed(gtk::FilterChange::Different);
                // Re-select the repo that is still open, if it survived the
                // filter. The handler is cloned out first: holding the borrow
                // across the call would panic if it ever re-registers itself.
                let callback = filter_changed.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
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
    fn selection_does_not_autoselect() {
        let Some(_guard) = gtk_test_guard("selection_does_not_autoselect") else {
            return;
        };
        let panel = SidebarPanel::new();

        // With autoselect on, `set_selected(INVALID_LIST_POSITION)` is refused and
        // GTK picks a replacement row inside `filter.changed()` — which lands the
        // selection on a section header and swaps the open repo when a pill is
        // toggled.
        assert!(!panel.selection().is_autoselect());
        panel.selection().set_selected(gtk::INVALID_LIST_POSITION);
        assert_eq!(panel.selection().selected(), gtk::INVALID_LIST_POSITION);
    }

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
