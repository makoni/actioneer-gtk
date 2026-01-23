# Compiling & Bundling Libadwaita

Key points
- Package name for pkg-config: libadwaita-1
- Meson: dependency('libadwaita-1') or use as a subproject with a libadwaita.wrap fallback.
- Flatpak: GNOME SDK (42+) includes libadwaita; otherwise add the libadwaita module.
- macOS: install gtk4, meson, gobject-introspection and use pkg-config to link.

Practical notes for this repo (Rust/gtk4):
- On Linux use system libadwaita or package via distribution packages. For Flatpak builds add libadwaita to the runtime.
- For CI, ensure pkg-config and GTK4 development packages are available.

Upstream: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/build-howto.html
