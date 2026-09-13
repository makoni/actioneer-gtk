//! The five states a run list can show instead of runs.
//!
//! Split out of `list.rs`: they are leaf widget builders with no bearing on the
//! list's own logic, and they were half its length.

use super::RetryHandler;
use crate::kernel::i18n::tr;
use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn build_spinner() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);
    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(8);
    spinner.set_margin_bottom(8);
    container.append(&spinner);
    container.upcast()
}

pub(super) fn build_idle_placeholder() -> gtk::Widget {
    let label = gtk::Label::new(Some(tr("Click to load runs...").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.set_halign(gtk::Align::Start);
    container.append(&label);
    container.upcast()
}

pub(super) fn build_empty_placeholder() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_halign(gtk::Align::Start);
    let label = gtk::Label::new(Some(tr("No recent runs").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let info_label = gtk::Label::new(Some(
        tr("Triggered runs may take 10-30 seconds to appear").as_str(),
    ));
    info_label.add_css_class("dim-label");
    info_label.add_css_class("caption");
    info_label.set_halign(gtk::Align::Start);
    container.append(&info_label);
    container.upcast()
}

pub(super) fn build_filtered_placeholder() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some(tr("No runs match the current filters").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let hint = gtk::Label::new(Some(
        tr("Adjust the status chips above to see more runs.").as_str(),
    ));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_halign(gtk::Align::Start);
    container.append(&hint);
    container.upcast()
}

pub(super) fn build_error_placeholder() -> (gtk::Widget, gtk::Label, RetryHandler) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some(tr("Unable to load workflow runs").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let detail_label = gtk::Label::new(None);
    detail_label.add_css_class("caption");
    detail_label.add_css_class("dim-label");
    detail_label.set_halign(gtk::Align::Start);
    container.append(&detail_label);

    let retry_button = gtk::Button::with_label(tr("Retry").as_str());
    retry_button.add_css_class("suggested-action");
    retry_button.set_halign(gtk::Align::Start);
    retry_button.set_margin_top(8);
    let retry_handler: RetryHandler = Rc::new(RefCell::new(None));
    let handler_ref = retry_handler.clone();
    retry_button.connect_clicked(move |_| {
        if let Some(callback) = handler_ref.borrow().as_ref() {
            callback();
        }
    });
    container.append(&retry_button);

    (container.upcast(), detail_label, retry_handler)
}
