---
name: Prepare release
description: Prepare an Actioneer release end to end - pick the version, write the user-facing AppStream changelog and its translations, bump versions and packaging, commit, push, run CI, open the GitHub draft release with technical notes, and open the Flathub PR. Use this whenever the user wants to cut a release, bump the version, prepare a changelog, publish a draft GitHub release, or update the Flathub manifest for a new version, even if they only name one part of the process.
metadata:
  opencode/autoinvoke: true
---

# Preparing an Actioneer release

The authoritative checklist is `docs/version-bump.md`. Read it first and follow it;
this skill adds the concrete decisions and gotchas that checklist leaves implicit,
learned from past releases. Everything is done on `develop`, no release branch, and
there are two sets of notes: the user-facing AppStream changelog (committed,
translated) and the technical notes (written into the GitHub draft, not committed).

## 1. Pick the version

List commits since the last tag (`git describe --tags --abbrev=0`, then
`git log --oneline <tag>..HEAD`) and decide the bump. Actioneer follows semver
loosely:

- new user-facing feature → minor;
- bug fix or packaging-only change → patch;
- a change that alters what the user needs or how a channel is built (a runtime
  base, a GTK/libadwaita feature floor, a minimum distribution) counts as a user
  change, so bump **minor**, not patch. A floor that every channel must meet
  changes what runs on older systems, and that is visible in a software centre
  even when the app itself behaves the same.

State the chosen version and the reasoning before touching files. Check whether the
CI runners moved since the tag (`git diff <tag>..HEAD -- .github/workflows | grep
runs-on`); if a standalone-binary runner moved, the technical notes must name the
glibc it now needs (the AppImage is exempt, it bundles its own).

## 2. User-facing changelog (metainfo + translations)

- Add a new `<release>` entry at the top of `data/metainfo.xml.in`, English only.
  `data/metainfo.xml` is gitignored and rendered at build time, so it is not
  committed.
- Bullets are plain user language: no crate names, no commit lists. **Never put an
  em dash (—) in a bullet**; `scripts/extract-translations.sh` drops it from the
  msgid, so no catalog entry matches. Use a comma or a colon instead.
- Run `scripts/extract-translations.sh` (needs `xtr`, `xgettext`, `msgcat`). Sanity
  check the msgid *set* grew by exactly the new bullets, not the diff size.
- For every locale in `po/LINGUAS` except `en`, append a `msgid`/`msgstr` pair to
  `po/<lang>.po`. Translate them yourself; `en` has no entry (the msgid is the
  English text). Keep the change surgical - do not run a blanket `msgmerge`.
- Update the msgid count in `AGENTS.md` ("N today") to the new number, as past
  i18n release commits did.
- Run `scripts/compile-translations.sh`; the compiled `po/locale/<lang>/.../actioneer.mo`
  files are committed.

This is commit `i18n(release): add X.Y.Z changelog entry with translations`.

## 3. Version bump and packaging sync

- `Cargo.toml` `[package].version`.
- `Cargo.lock`: update only the crate's own entry with `cargo update -w`. Do not use
  `cargo generate-lockfile` (it re-resolves every dependency and slips an unreviewed
  refresh into the release commit).
- `snapcraft.yaml` version, `docs/flatpak.md` example tag, and the release command in
  `src/demo/logs/job-43021.log` if the version appears.
- `RELEASE.md`: rewrite the top section for the new version, plain user language,
  mirroring the style of the previous entry.
- Flatpak `cargo-sources.json` only changes if dependencies changed, not for a
  version-only bump of the crate itself. The `check-flatpak-lock-sync.sh` git-diff
  heuristic false-positives on a version-only bump; the content check (the primary
  path, used in CI) is correct. Run the check with `flatpak-cargo-generator` available
  so it does the content comparison, and expect "content verified".

This is commit `chore(release): bump version to X.Y.Z and sync packaging` - the i18n
commit comes first. Run the step-5 checks (`cargo fmt`, clippy `-D warnings`,
`cargo test`, `check-flatpak-lock-sync.sh`) before committing.

## 4. Publish the draft

Push, run `ci.yml` and wait for green, then `publish.yml` with the tag. `gh`
workflow_dispatch needs a **ref** (`develop`), not a commit SHA. `publish.yml`
creates a **draft** release with the built bundles; its body is a placeholder.

- Pin the draft to the exact release commit so the tag lands where Flathub points
  even if `develop` moves: `gh release edit <tag> --target <full-sha>`.
- Replace the placeholder with technical notes: what changed and why, dependency and
  packaging changes, anything a packager or contributor needs, and the compare link
  `https://github.com/makoni/actioneer-gtk/compare/<last-tag>...X.Y.Z`. Do not claim
  `flatpak-builder-lint` results you did not actually run.

## 5. Flathub

The clone lives at `~/Work/me.spaceinbox.actioneer` (one PR per release). Branch
`release-X.Y.Z` from `origin/master`, then render the manifest pinned to the **same**
release commit and copy the vendored sources:

```bash
scripts/render-flatpak-manifest.sh --mode flathub --commit <full-sha> \
  --output ~/Work/me.spaceinbox.actioneer/me.spaceinbox.actioneer.yaml
cp flatpak/me.spaceinbox.actioneer.cargo-sources.json ~/Work/me.spaceinbox.actioneer/
```

`flatpak-builder-lint` runs in the Flathub CI when the runtime is not installed
locally. Commit as `Release X.Y.Z` (body mirrors past commits: pin, whether sources
changed, what this release is for the Flatpak). Open the PR against `master`, wait
for the Flathub test build, publish the GitHub draft once it is green, then merge.

Stop before publishing the draft - publishing waits for the Flathub PR to be green.
