# Version bump checklist

Releases are prepared directly on `develop`, with no release branch, in two
commits: `i18n(release): add X.Y.Z changelog entry with translations` (steps 3–4)
and `chore(release): bump version to X.Y.Z and sync packaging` (steps 1–2 and
`RELEASE.md`). Run the checks in step 5 and `ci.yml` before publishing.

There are two sets of release notes:

- **For users, in the repository**: the AppStream `<release>` entry (translated,
  shown by software centers) and `RELEASE.md`. Plain language, no crate names
  or commit lists.
- **Technical, on GitHub**: written into the draft release that
  `publish.yml` creates (step 6), not committed.

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
- Do not put an em dash (—) in a bullet: `scripts/extract-translations.sh` drops it from the msgid, so no catalog entry matches the extracted string. Use commas or a colon.
- Check whether the CI runners changed since the last tag:
  ```bash
  git diff <last-tag>..HEAD -- .github/workflows | grep -E '^[-+].*(runs-on|runner):'
  ```
  The standalone binary needs at least the glibc of the runner that built it
  (`build-release.yml`). If that runner moved to a newer Ubuntu, the GitHub
  release notes must say which glibc it now requires. The AppImage is exempt:
  it bundles its own glibc (`packaging/anylinux-appimage.sh`), and
  `appimage-ci.yml` runs it on Ubuntu 22.04 to prove it.

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

## 6) Publish
```bash
git push origin develop
gh workflow run ci.yml --ref develop          # must be green
gh workflow run publish.yml --ref develop -f tag=X.Y.Z
```
`publish.yml` builds the Flatpak, Snap and AppImage bundles and creates a
**draft** GitHub release with them. Its body is a placeholder: replace it with
the technical notes (what changed and why, dependency and packaging changes,
anything a packager or contributor needs, and the compare link
`https://github.com/makoni/actioneer-gtk/compare/<last-tag>...X.Y.Z`).

Pin the draft to the exact commit, so the tag lands where Flathub points even
if `develop` moves before publishing:
```bash
gh release edit X.Y.Z --target <sha>
```

## 7) Flathub
The Flathub manifest lives in `flathub/me.spaceinbox.actioneer` (one PR per
release, branch `release-X.Y.Z`). Render it from the template, pinned to the
same commit, and copy the vendored sources:
```bash
scripts/render-flatpak-manifest.sh --mode flathub --commit <sha> \
  --output <flathub-checkout>/me.spaceinbox.actioneer.yaml
cp flatpak/me.spaceinbox.actioneer.cargo-sources.json <flathub-checkout>/
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest \
  <flathub-checkout>/me.spaceinbox.actioneer.yaml
```
Open the PR against `master` and wait for the Flathub test build. Publish the
GitHub draft once that PR is green, then merge it.
