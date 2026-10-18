# Actioneer 1.2.1

A bug-fix release: signing in works properly on a desktop again, and the
AppImage runs on older systems again, in your language, with notifications.

## What's fixed

- **Signing in.** Since 1.2.0, on a desktop the sign-in dialog kept the size
  it opened with, so once GitHub sent the code, the "Open GitHub in Browser"
  and Cancel buttons were hidden below a scrollbar. The dialog now grows to
  show the code and both buttons in full.
- **The AppImage runs on older distributions again.** The 1.2.0 AppImage
  needed glibc 2.43 and did not start on Ubuntu 24.04, Debian 13, Fedora 42
  or older systems. The AppImage now brings its own C library, so it runs on
  Ubuntu 22.04 and later, Debian 12, Fedora, and other x86_64 and aarch64
  distributions.
- **The AppImage speaks your language.** Earlier AppImages always came up in
  English; this one includes all 14 translations.
- **Notifications from the AppImage.** Workflow notifications now appear when
  you run the AppImage. If you integrate it with a tool such as Gear Lever,
  your dock shows Actioneer's name and icon for its window.

## Before you update

If you use the AppImage and update it with a tool such as Gear Lever or
AppImageUpdate, this first update downloads nearly the whole file: the
AppImage is packaged in a new format. Later updates download only what
changed again.

The Flatpak and the Snap update as usual. Your settings, favorites and
sign-in carry over.
