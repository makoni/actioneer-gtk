# Actioneer 1.1.1

A maintenance release: refreshed dependencies and improved AppImage packaging.

## Summary

- Refreshed bundled components (dependencies) to keep Actioneer reliable and up to date.
- AppImage packages now embed zsync update information, so downstream package managers can update the AppImage incrementally.
- AppImage packaging CI now reports what it found before failing, making packaging failures easier to diagnose.

## What's changed

- **Dependency refresh**: updated `Cargo.lock` (keyring 4.1.6 -> 4.2.0, gio / gtk-rs family -> 0.22.9, secret-service 5.1 -> 5.2, zbus 5.18 -> 5.19, plus a broad set of transitive updates) and regenerated the Flatpak cargo sources.
- **AppImage packaging**: the AppImage bundle now carries zsync update information for downstream package managers, and the packaging workflow prints the checks it found before failing.

## Commits since `1.1.0`

```text
b0899e3 chore(release): bump version to 1.1.1 and sync packaging
26cdb37 i18n(release): add 1.1.1 changelog entry with translations
cfbe039 chore(deps): bump crates.io dependencies
274450e ci(appimage): say what the AppImage checks found before failing
abf4504 ci(appimage): embed zsync update information for package managers
```

## User impact / compatibility

- Existing users do not need to migrate data or settings.
- No UI or behavioral changes — this is a maintenance release focused on dependencies and packaging.
- AppImage users on package managers that support zsync can now update the app incrementally.
