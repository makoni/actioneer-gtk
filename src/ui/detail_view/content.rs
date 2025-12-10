use super::RepoDetailPane;
use crate::ui::utils::create_detail_clamp;
#[cfg(test)]
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;

impl RepoDetailPane {
    pub(super) fn attach_run_list(&self) {
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.set_margin_start(12);
        separator.set_margin_end(12);
        self.root.append(&separator);

        let clamp = build_runs_container(&self.workflow_view);
        self.root.append(&clamp);
    }
}

fn build_runs_container(list_view: &gtk::ListView) -> adw::Clamp {
    list_view.set_hexpand(true);
    list_view.set_vexpand(true);
    list_view.set_halign(gtk::Align::Fill);
    list_view.set_valign(gtk::Align::Fill);

    create_detail_clamp(list_view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    fn builds_runs_container_with_scrolled_child() {
        let Some(_guard) = gtk_test_guard("builds_runs_container_with_scrolled_child") else {
            return;
        };

        let store = gio::ListStore::new::<gtk::Widget>();
        let selection = gtk::NoSelection::new(Some(store.clone()));
        let factory = gtk::SignalListItemFactory::new();
        let list_view = gtk::ListView::new(Some(selection), Some(factory));
        let clamp = build_runs_container(&list_view);
        let child = clamp.child().expect("Clamp should wrap a widget");
        assert!(child.downcast_ref::<gtk::ListView>().is_some());
    }
}
