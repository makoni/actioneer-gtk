use super::RepoDetailPane;
use crate::i18n::tr;
#[cfg(test)]
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;

impl RepoDetailPane {
    pub(super) fn attach_run_list(&self) {
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        self.root.append(&separator);

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let (heading, plain_script) = crate::ui::utils::section_heading(&tr("Workflows"));
        let section_label = gtk::Label::new(Some(&heading));
        if plain_script {
            section_label.add_css_class("no-tracking");
        }
        section_label.add_css_class("section-label");
        section_label.set_halign(gtk::Align::Start);
        section_label.set_margin_start(4);
        section_label.set_margin_bottom(10);
        container.append(&section_label);

        container.append(&self.workflow_view);

        let footer_label = self.header.footer_label();
        footer_label.set_margin_top(12);
        footer_label.set_margin_start(4);
        container.append(&footer_label);

        let clamp = build_runs_container(&container);

        let viewport = gtk::Viewport::builder()
            .scroll_to_focus(false)
            .hexpand(true)
            .build();
        viewport.set_child(Some(&clamp));

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled_window.set_child(Some(&viewport));
        self.root.append(&scrolled_window);
    }
}

fn build_runs_container(content: &gtk::Box) -> adw::Clamp {
    content.set_hexpand(true);
    content.set_halign(gtk::Align::Fill);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(880);
    clamp.set_hexpand(true);
    clamp.set_vexpand(false);
    clamp.set_margin_top(20);
    clamp.set_margin_bottom(28);
    clamp.set_margin_start(24);
    clamp.set_margin_end(24);
    clamp.set_child(Some(content));
    clamp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    fn builds_runs_container_with_clamped_content() {
        let Some(_guard) = gtk_test_guard("builds_runs_container_with_clamped_content") else {
            return;
        };

        let store = gio::ListStore::new::<gtk::Widget>();
        let selection = gtk::NoSelection::new(Some(store.clone()));
        let factory = gtk::SignalListItemFactory::new();
        let list_view = gtk::ListView::new(Some(selection), Some(factory));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&list_view);

        let clamp = build_runs_container(&content);
        assert_eq!(clamp.maximum_size(), 880);
        let child = clamp.child().expect("Clamp should wrap a widget");
        let content = child.downcast::<gtk::Box>().expect("content box");
        let first = content.first_child().expect("list view inside");
        assert!(first.downcast_ref::<gtk::ListView>().is_some());
    }
}
