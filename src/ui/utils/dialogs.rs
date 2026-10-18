use gtk4::prelude::*;
use libadwaita as adw;

/// States the smallest size a dialog with breakpoints lays out for, which
/// libadwaita requires of one (it warns on every open otherwise): narrower than
/// a phone in portrait, lower than one on its side, once the bottom sheet's
/// frame is taken off both.
pub fn set_adaptive_dialog_min_size(dialog: &adw::Dialog) {
    dialog.set_size_request(300, 200);
}
