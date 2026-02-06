---
applyTo: "**/*"
---
# Copilot: version bump checklist

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
- Add a new `<release>` in `data/metainfo.xml`.
- Build the change list from commits after the latest tag:
  ```bash
  git describe --tags --abbrev=0
  git log --oneline <last-tag>..HEAD
  ```
- Keep older entries intact.

## 4) Recommended checks
```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
scripts/check-flatpak-lock-sync.sh
```
