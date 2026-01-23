# Initialization (adw_init / AdwApplication)

Quick guidance
- Preferred: use AdwApplication (adw_application_new) — it auto-initializes libadwaita and integrates with GTK application lifecycle.
- Alternative: call adw_init() early in main() before creating windows if you can't use AdwApplication.

Rust notes
- When using gtk4-rs and libadwaita bindings, ensure `adw::init()` (or equivalent) is invoked before creating AdwWindow/AdwApplicationWindow objects.
- Prefer using AdwApplication equivalent for cleaner lifecycle and style-manager integration.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/initialization.html
