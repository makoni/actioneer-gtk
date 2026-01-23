# Migrating to Adaptive Dialogs

Highlights
- Use AdwDialog (and AdwAlertDialog, AdwPreferencesDialog) — adaptive dialogs can present as floating windows or bottom sheets depending on size.
- Parent windows must be AdwWindow / AdwApplicationWindow for correct behavior.
- Replace older adw_message_dialog / adw_preferences_window APIs with the newer dialog APIs; note changes in constructors and presentation functions.

Practical: migrate preference windows and about dialogs to dialog variants where appropriate; adapt event handling (closed/close-attempt signals) instead of older close-request patterns.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/migrating-to-adaptive-dialogs.html
