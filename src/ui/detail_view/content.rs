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

        self.toast_overlay.set_child(Some(&self.root));
    }
}

fn build_runs_container(list_view: &gtk::ListView) -> adw::ClampScrollable {
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scrolled.set_hexpand(true);
    scrolled.set_vexpand(true);
    scrolled.set_propagate_natural_height(false);
    scrolled.set_child(Some(list_view));

    create_detail_clamp(&scrolled)
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

        let store = gio::ListStore::new::<gtk::ListBoxRow>();
        let selection = gtk::NoSelection::new(Some(store.clone()));
        let factory = gtk::SignalListItemFactory::new();
        let list_view = gtk::ListView::new(Some(selection), Some(factory));
        let clamp = build_runs_container(&list_view);
        let child = clamp.child().expect("Clamp should wrap a widget");
        assert!(child.downcast_ref::<gtk::ScrolledWindow>().is_some());
    }
}
