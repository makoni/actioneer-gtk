# AdwDialog

Purpose
- Adaptive dialog container; can present as a centered floating window or a bottom-sheet depending on available space.

Practical tips
- Parent must be an `AdwWindow` or `AdwApplicationWindow` for correct adaptive behaviour.
- Use `adw_dialog_present()` to present the dialog; dialogs are modal and have `content-width` / `content-height` properties.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/class.Dialog.html
