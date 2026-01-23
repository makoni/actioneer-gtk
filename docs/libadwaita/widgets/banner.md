# AdwBanner

Short summary: A simple banner widget for prominent inline messages, typically used at the top of a view to surface alerts and actions.

When to use: For important but non-modal messages that should remain visible in the layout and can include actions (buttons) and dismiss controls.

Key API notes (Rust/gtk4):
- Construct with `adw::Banner::new()` and configure the label and start/end content. Use `set_title()`/`set_body()` for text.
- Add action buttons via `add_prefix()`/`add_suffix()` or by packing `gtk::Button` into the banner's content areas.
- Dismissal: call `close()` to hide; connect to `close` or `response` signals to react.

Accessibility: Mark role as alert where appropriate and ensure actions are reachable by keyboard.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/widgets/banner.html
