# Migrating from Libhandy 1.4 to Libadwaita

Key migration notes
- Libadwaita replaces many libhandy widgets; migration implies GTK3 → GTK4 changes too.
- Stop using deprecated libhandy symbols; prefer Adw counterparts (AdwActionRow, AdwPreferencesGroup, AdwLeaflet → AdwNavigationView/SplitView replacements, etc.).
- Several widgets were removed (HdyValueObject, HdySearchBar, HdyWindowHandle) — use GTK4 replacements (GtkSearchBar, GtkWindowHandle) or Adw equivalents.

Practical: consult this guide when porting older UI code (especially preference windows and leaflets) to Adw. Prefer AdwNavigation* widgets where leaflets were used.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/migrating-libhandy-1-4-to-libadwaita.html
