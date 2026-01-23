# Boxed Lists & Rows

Pattern summary
- Boxed lists are implemented with GtkListBox + style class `.boxed-list` (or `.boxed-list-separate`). Set selection-mode to `GTK_SELECTION_NONE`.
- Use AdwActionRow, AdwSwitchRow, AdwExpanderRow, AdwComboRow, AdwEntryRow, AdwPasswordEntryRow, AdwSpinRow, AdwButtonRow for rows inside boxed lists.

Implementation tips
- For settings/preferences use AdwPreferencesGroup and AdwPreferencesPage to group boxed-list rows.
- For clickable rows prefer AdwActionRow or AdwButtonRow (activatable/clicked signals).

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/boxed-lists.html
