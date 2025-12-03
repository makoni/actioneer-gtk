use gtk4::prelude::*;
use gtk4::{self as gtk};

pub(super) fn clear_runs_box(runs_box: &gtk::Box) {
    loop {
        let child_opt = runs_box.first_child();
        let Some(child) = child_opt else {
            break;
        };
        runs_box.remove(&child);
    }
}

pub(super) fn append_spinner(runs_box: &gtk::Box) {
    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(8);
    spinner.set_margin_bottom(8);
    runs_box.append(&spinner);
}

pub(super) fn append_empty_runs_state(runs_box: &gtk::Box) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.set_halign(gtk::Align::Start);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);

    let label = gtk::Label::new(Some("No recent runs"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    vbox.append(&label);

    let info_label = gtk::Label::new(Some("Triggered runs may take 10-30 seconds to appear"));
    info_label.add_css_class("dim-label");
    info_label.add_css_class("caption");
    info_label.set_halign(gtk::Align::Start);
    vbox.append(&info_label);

    runs_box.append(&vbox);
}

pub(super) fn append_filtered_runs_placeholder(runs_box: &gtk::Box) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.set_halign(gtk::Align::Start);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);

    let label = gtk::Label::new(Some("No runs match the current filters"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    vbox.append(&label);

    let hint = gtk::Label::new(Some("Adjust the status chips above to see more runs."));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_halign(gtk::Align::Start);
    vbox.append(&hint);

    runs_box.append(&vbox);
}

pub(super) fn append_runs_header(
    runs_box: &gtk::Box,
    visible_count: usize,
    filtered_total: usize,
    overall_total: usize,
) {
    let label_text = if filtered_total == overall_total {
        if visible_count < overall_total {
            format!(
                "Recent runs (showing {} of {})",
                visible_count, overall_total
            )
        } else {
            format!("Recent runs ({})", overall_total)
        }
    } else {
        format!(
            "Recent runs (showing {} of {} matching filters)",
            visible_count, filtered_total
        )
    };

    let count_label = gtk::Label::new(Some(&label_text));
    count_label.add_css_class("dim-label");
    count_label.add_css_class("caption");
    count_label.set_halign(gtk::Align::Start);
    count_label.set_margin_bottom(8);
    runs_box.append(&count_label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::gtk_test_guard;

    #[test]
    #[ignore = "requires GTK display"]
    fn append_empty_runs_adds_child() {
        let Some(_guard) = gtk_test_guard("append_empty_runs_adds_child") else {
            return;
        };

        let runs_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        append_empty_runs_state(&runs_box);

        assert!(runs_box.first_child().is_some());
    }
}
