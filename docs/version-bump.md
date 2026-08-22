# Version bump checklist

## 1) Update versions
- `Cargo.toml` — `[package].version`.
- `Cargo.lock` — regenerate via `cargo generate-lockfile` (the `actioneer` entry should update).
- `snapcraft.yaml` — `version`.
- `docs/flatpak.md` — example tag (`vX.Y.Z`).
- `src/demo/logs/job-43021.log` — update the release command example if the version appears.
- `TODO.md` — add a Recent Updates entry.

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

## 4) Translate the new changelog bullets
- Run `scripts/extract-translations.sh` to refresh `po/actioneer.pot` with the new msgids.
- For every locale in `po/LINGUAS` (except `en`), append a `msgid` + `msgstr` pair to `po/<lang>.po`.
- Run `scripts/compile-translations.sh` to compile `.mo` catalogs and render the final `data/metainfo.xml`.

## 5) Recommended checks
```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
scripts/check-flatpak-lock-sync.sh
```
