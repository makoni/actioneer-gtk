# Actioneer 1.3.0

A packaging release: the minimum desktop versions rise to GNOME 48, and the
Snap now brings its own runtime. There are no functional changes to the app.

## What's changed

- **Minimum versions.** Actioneer now needs at least GTK 4.18 and libadwaita
  1.7, which is GNOME 48. Debian 13 and Fedora 42 already ship exactly this,
  so building from source there is unaffected.
- **The Snap's runtime.** The Snap is built on a newer base, so GTK 4.24 and
  libadwaita 1.10 reach users even on Ubuntu 24.04. The runtime base belongs
  to the Snap rather than the host, so the host does not have to be updated.

The Flatpak and the AppImage are unchanged apart from the version number.

## Before you update

Your settings, favorites and sign-in carry over. On a Flatpak or AppImage
install, nothing looks different: only the Snap moves to a newer runtime.
