# TODO: Backlog

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

Where the code lives and how to validate a change are documented once, in
`AGENTS.md` and `docs/agent-guide.md` — not repeated here, where the copy only
drifts.

## Open items (optional / low priority)

Neither is required work, and neither has been started.

### AppImage: bundle the newest GTK the way Flatpak does

Question: Flatpak gets "latest GTK" declaratively (`org.gnome.Platform "50"`);
the AppImage bundles whatever the CI runner has (4.14) — four GNOME minors
behind (46 → 50), and on unpinned tooling.

How the plugin works (read from `linuxdeploy-plugin-gtk.sh`, master):
- GTK 4 path: `gtk4_libdir = $(pkg-config --variable=libdir gtk4)/gtk-4.0` of
  the *build environment*, copied into the AppDir; same for the gdk-pixbuf
  loaders, gobject/gio/rsvg/pango. The version is 100% determined by the
  runner's `libgtk-4-dev`.
- Only overrides: `LD_GTK_LIBRARY_PATH` (library source dir) and
  `DEPLOY_GTK_VERSION` (major only). No "fetch GTK from elsewhere" mode.
- Environment tools the plugin requires: `pkg-config`, `file`, `find`, `ldd`,
  `realpath`, `glib-compile-schemas`, `gdk-pixbuf-query-loaders`, and
  `dpkg-architecture` (hard exit when `/etc/os-release` says debian/ubuntu).
- Our workflow pins nothing: `linuxdeploy` from `continuous`, the plugin from
  `master` (`appimage-ci.yml:80-81`).

Options:
1. **(recommended) run the packaging step inside the Flatpak runtime** —
   install the runtime ref (e.g. `flatpak install flathub
   org.gnome.Platform/50`) and run `flatpak run --command=/usr/bin/bash
   org.gnome.Platform/50 -c "linuxdeploy … --plugin gtk"` on the shared
   workspace. Inside the sandbox the plugin's `pkg-config` resolves the
   runtime's `gtk4.pc` (≈4.22) and copies *the runtime's* libraries — the
   AppImage then carries the same GTK as the Flatpak build. One version
   variable for both packagings: bumping the runtime in the Flatpak manifest
   lifts the AppImage automatically.
   To verify on implementation day: `flatpak` on GH runners (apt + flathub
   remote, cache `~/.local/share/flatpak` — the runtime is hundreds of MB);
   the runtime must ship the plugin's toolchain (org.gnome.Platform is built
   on freedesktop-sdk, a full dev environment — check `pkg-config`,
   `dpkg-architecture`, `ldd`, `glib-compile-schemas`,
   `gdk-pixbuf-query-loaders`); linuxdeploy must run in extracted mode
   (`APPIMAGE_EXTRACT_AND_RUN=1`, already exported in the workflow);
   aarch64 runner parity (`ubuntu-24.04-arm` is native arm64).
2. Newer build environment (runner/container with GTK ≥ 4.22): GH-hosted
   runners top out at ubuntu-24.04 → 4.14. Self-hosted / Arch container is
   fragile. Rejected.
3. Keep 4.14, but **pin** `linuxdeploy` + `linuxdeploy-plugin-gtk` to tags and
   add a CI assertion that the bundled `libgtk-4.so` version ≥ the compile
   floor. Do it anyway (defense in depth), independent of the 1-vs-3 decision.

Decision needed: option 1 (full parity with Flatpak) vs option 3 (honest 4.14,
pinned + asserted).

