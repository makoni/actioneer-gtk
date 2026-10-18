# Actioneer 1.2.0

Actioneer now works on phones and in narrow windows, follows the Omarchy
desktop theme, and explains more clearly when it cannot store your sign-in.

## What's new

- **Phones and narrow windows.** Every window and dialog fits down to 360
  pixels wide, in all 14 languages. On a narrow screen the repository list and
  a repository's workflows become two pages: tap a repository to open it, and
  use Back to return. Workflow details rearrange themselves to fit, the job
  list in the log viewer slides in from a button, and Preferences, About,
  Help and Keyboard Shortcuts open as dialogs that become bottom sheets on a
  phone. Actioneer is marked as phone-ready for software centers, and its store
  pages now include phone screenshots.
- **Omarchy theme.** On Omarchy, Actioneer takes on the desktop theme: its
  colors, its light or dark style, and its square corners. It changes along
  with the theme. A *Follow Omarchy theme* switch in Preferences turns this
  off, and your own light/dark choice comes back. On other desktops nothing
  changes. The Snap cannot see Omarchy's theme, so it does not offer this.
- **Clearer sign-in storage errors.** If neither a Secret portal nor the
  system keyring can store your token, sign-in now says so plainly instead of
  reporting a broken keyring. The Flatpak documentation explains how to fix
  this on desktops without a Secret portal, such as Hyprland or Sway.
- **Run lists keep loading** after GitHub's change to how it reports very
  large run counts.
- **Updated bundled components**, including a fix for a security advisory in
  the network stack.

## Before you update

The AppImage now needs **glibc 2.43 or newer**, as found in Ubuntu 26.04. It
no longer starts on Ubuntu 24.04, Debian 13, Fedora 42 or RHEL 10. On those
systems, install the Flatpak or the Snap instead; both bring their own
runtime. The 1.1.2 AppImage still works there.

Your settings, favorites and sign-in carry over; there is nothing to migrate.
