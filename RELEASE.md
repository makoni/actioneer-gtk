# Actioneer 1.0.12

Maintenance release. No user-facing feature changes; the underlying
GNOME desktop bindings and HTTP stack have been refreshed.

## What's changed

- **gtk-rs-core 0.22 migration**: coordinated bump of `gtk4` 0.10 → 0.11,
  `libadwaita` 0.8 → 0.9, and `gio` 0.21 → 0.22. The `glib::source::unix_signal_add_local`
  API was dropped in glib 0.22; the Unix `SIGINT`/`SIGTERM` handler is now
  implemented on top of `tokio::signal::unix` and bridged back to the GLib
  main loop via `MainContext::invoke`. Shutdown semantics unchanged.
- **reqwest 0.12 → 0.13**: opts into the new `query` feature (no longer a
  default) and switches to the `rustls` TLS backend that is now the 0.13
  default. Live smoke against `api.github.com` verified end-to-end TLS works.
- **ashpd 0.12 → 0.13**: per-portal modules are now feature-gated; enabled
  `notification` and `secret`. `Secret::retrieve` gained a `RetrieveOptions`
  parameter — we pass `Default::default()` (per-upstream note, the `token`
  field is not yet consumed by the portal).
- **sha2 0.10 → 0.11**: transparent bump.
- **GitHub Actions**: `actions/checkout` v5 → v6, `actions/download-artifact`
  v7 → v8, `actions-rust-lang/setup-rust-toolchain` 1.15.3 → 1.16.0,
  `actions/cache` 5.0.3 → 5.0.5, `softprops/action-gh-release` v2 → v3.
- **Dependabot config**: new `.github/dependabot.yml` with a `gtk-rs` group
  for co-released crates, an `actions` group bundling all workflow updates,
  and an `ignore` rule for `version-update:semver-major` on gtk-rs-core
  crates (Dependabot cannot produce the coordinated bump on its own).
  `rand 0.8.x` is also ignored as a lockfile-only phantom dep.
- **Lock-sync CI check**: hardened `scripts/check-flatpak-lock-sync.sh` to
  use `flatpak-cargo-generator` for a content-based check rather than a
  raw git-diff heuristic, which false-positived on version-only bumps.

## Commits since `1.0.11`

```
d3fb96e chore(deps): migrate to gtk-rs-core 0.22 (gtk4 0.11, libadwaita 0.9, gio 0.22) (#25)
78dfc60 chore(dependabot): block major bumps of gtk-rs-core crates
3a06832 chore(deps): bump reqwest from 0.12.28 to 0.13.2 (#22)
17d2d85 chore(deps): bump ashpd from 0.12.3 to 0.13.10 (#23)
1939933 chore(deps): bump sha2 from 0.10.9 to 0.11.0 (#20)
e6d23f9 chore(deps): bump the actions group with 5 updates (#21)
11e9b96 chore(dependabot): group gtk-rs-core crates into a single PR
374d8b8 chore: add dependabot configuration for cargo and GitHub Actions updates
```

## User impact

- No behavioral changes, no UI changes, no new features.
- HTTP requests to GitHub now use `rustls` (was `native-tls`); platform
  trust store handling switches to `rustls-platform-verifier` on Linux.
  Verified with a live unauthenticated call to `api.github.com/rate_limit`.
- Flatpak users move to GNOME Platform 50 on next build (already in 1.0.11).

## Compatibility

- Minimum Rust toolchain for building from source: `1.95.0` stable.
- Minimum system libraries: gtk4 `4.0.0`, libadwaita `1.5` (unchanged).
- All packaging targets (AppImage, Flatpak, Snap) continue to build from
  the same `Cargo.lock` via the shared build artifact workflow.
