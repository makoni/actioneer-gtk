# Version bump checklist

## 1) Update versions
- `Cargo.toml` — `[package].version`.
- `Cargo.lock` — update only the crate's own entry with `cargo update -w`. Do not use `cargo generate-lockfile`: it re-resolves every dependency to its newest compatible version, which slips an unreviewed dependency refresh into the release commit.
- `snapcraft.yaml` — `version`.
- `docs/flatpak.md` — example tag (`vX.Y.Z`).
- `src/demo/logs/job-43021.log` — update the release command example if the version appears.

## 2) Update Flatpak sources
```bash
scripts/regenerate-flatpak-sources.sh
scripts/check-flatpak-lock-sync.sh
```

## 3) Update the AppStream changelog
- Add a new `<release>` in `data/metainfo.xml.in` (English only; `data/metainfo.xml` is gitignored and rendered at build time).
- Build the change list from commits after the latest tag:
  ```bash
  git describe --tags --abbrev=0
  git log --oneline <last-tag>..HEAD
  ```
- Keep older entries intact.
- Check whether the CI runners changed since the last tag:
  ```bash
  git diff <last-tag>..HEAD -- .github/workflows | grep -E '^[-+].*(runs-on|runner):'
  ```
  The AppImage and the standalone binary need at least the glibc of the runner
  that built them (`appimage-ci.yml`, `build-release.yml`). If that runner moved
  to a newer Ubuntu, the release notes (`metainfo.xml.in`, `RELEASE.md`, the
  GitHub release) must say which glibc is now required and which distributions
  lose the AppImage, pointing them to the Flatpak or the Snap; update the
  AppImage section of `README.md` to match. The first release after 1.1.2 is
  such a release: 24.04 → 26.04, glibc 2.39 → 2.43.

## 4) Translate the new changelog bullets
- Run `scripts/extract-translations.sh` to refresh `po/actioneer.pot` with the new msgids (needs `xtr`: `cargo install xtr`). Check that the msgid set grew by exactly the new bullets; the rest of the diff is `#:` line references shifting.
- For every locale in `po/LINGUAS` (except `en`), append a `msgid` + `msgstr` pair to `po/<lang>.po`.
- Run `scripts/compile-translations.sh` to compile `.mo` catalogs and render the final `data/metainfo.xml`.

## 5) Recommended checks
```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
scripts/check-flatpak-lock-sync.sh
```
