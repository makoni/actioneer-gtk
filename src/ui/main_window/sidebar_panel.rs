use crate::ui::utils::create_sidebar_clamp;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;

#[derive(Clone)]
pub struct SidebarPanel {
    clamp: adw::ClampScrollable,
    search_entry: gtk::SearchEntry,
    repo_list: gtk::ListBox,
}

impl SidebarPanel {
    pub fn new() -> Self {
        let repo_list = gtk::ListBox::new();
        repo_list.add_css_class("boxed-list");
        repo_list.set_margin_top(0);
        repo_list.set_margin_bottom(12);
        repo_list.set_margin_start(12);
        repo_list.set_margin_end(12);
        repo_list.set_accessible_role(gtk::AccessibleRole::List);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search repositories..."));
        search_entry.set_margin_top(12);
        search_entry.set_margin_bottom(12);
        search_entry.set_margin_start(12);
        search_entry.set_margin_end(12);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&repo_list));

        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.append(&search_entry);
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
            repo_list,
        }
    }

    pub fn clamp(&self) -> adw::ClampScrollable {
        self.clamp.clone()
    }

    pub fn search_entry(&self) -> gtk::SearchEntry {
        self.search_entry.clone()
    }

    pub fn repo_list(&self) -> gtk::ListBox {
        self.repo_list.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires GTK display"]
    fn sidebar_panel_initializes_widgets() {
        gtk::init().expect("GTK init failed");
        let panel = SidebarPanel::new();
        assert_eq!(
            panel
                .search_entry()
                .placeholder_text()
                .as_ref()
                .map(|s| s.as_str()),
            Some("Search repositories...")
        );
        assert_eq!(
            panel.repo_list().accessible_role(),
            gtk::AccessibleRole::List
        );
    }
}
