---
name: bump-actioneer
description: Bump Actioneer to a new release version, update all required release files, generate multilingual AppStream release notes from changes since the latest git tag, generate technical GitHub release notes, and run the required release scripts.
license: MIT
---

# bump-actioneer

Use this skill when the user wants to release a new Actioneer version and expects the repository metadata to be updated consistently.

## Required input

This skill requires one argument:

- `version`: the new application version in `X.Y.Z` format, for example `1.0.9`

Optional input:

- `release_kind`: one of `normal`, `maintenance`, or `security`

Typical invocations:

- `Use bump-actioneer for version 1.0.9`
- `Bump Actioneer to 1.1.0 with bump-actioneer`

## Goal

Given a target version, this skill must:

1. Update the version everywhere Actioneer expects it.
2. Regenerate lock / packaging metadata that depends on the version or lockfile.
3. Add a new user-friendly release entry to `data/metainfo.xml.in` (the gettext-aware source template; `data/metainfo.xml` is generated and gitignored).
4. Generate AppStream release notes in **all supported application languages** via the gettext pipeline (`po/*.po`).
5. Generate technical GitHub release notes in `RELEASE.md`.
6. Run the required scripts from `scripts/`.
7. Validate the result and summarize what changed.

Use `release-context.md` in this directory as the canonical companion reference for file targets, locale mapping, script selection, and release-note conventions.

## Procedure

### 1. Gather release context

Before editing:

1. Read `release-context.md`.
2. Determine the previous tag with:
   ```bash
   git describe --tags --abbrev=0
   ```
3. Collect the changes since that tag:
   ```bash
   git --no-pager log --oneline <last-tag>..HEAD
   ```
4. Collect a diff summary:
   ```bash
   git --no-pager diff --stat <last-tag>..HEAD
   ```
5. Inspect the touched files if the commit subjects are too vague to build accurate release notes.

Do **not** invent changelog bullets from commit titles alone if the code tells a different story. Prefer grouping several technical commits into a smaller set of user-facing improvements.

### 2. Update every release-version target

Update the version in every file listed in `release-context.md`.

At minimum, that currently includes:

- `Cargo.toml`
- `Cargo.lock` (via regeneration, not hand-editing)
- `snapcraft.yaml`
- `docs/flatpak.md`
- `src/demo/logs/job-43021.log`
- `RELEASE.md`

If the repo later adds new release-version surfaces, update `release-context.md` and include them in the bump as well.

### 3. Regenerate dependent artifacts

After updating `Cargo.toml`, regenerate dependent files instead of editing them manually where possible:

```bash
cargo generate-lockfile
```

If `Cargo.lock` changes, run the required Flatpak sync scripts described below.

### 4. Add the English release entry to `data/metainfo.xml.in`

Add a new `<release>` entry at the top of the `<releases>` list in `data/metainfo.xml.in`.

Do **not** hand-edit `data/metainfo.xml` — it is gitignored and rendered from `data/metainfo.xml.in` + `po/*.po` by `msgfmt --xml` at build time.

Rules:

- Use the requested version for `version="..."`.
- Use the current date for `date="YYYY-MM-DD"`.
- Keep older release entries intact.
- Write **English only** in `<li>` elements; no inline `xml:lang` entries (translations live in `po/*.po`).
- Keep the changelog **user-friendly**:
  - describe improvements in terms of user-visible behavior
  - avoid raw implementation details, refactor notes, and dependency-noise unless it clearly benefits users
  - merge related technical changes into broader product-facing bullets
- If `release_kind` is `maintenance` or `security`, prefer a short and simple changelog instead of stretching minor technical work into feature-style bullets.
- For security-only or dependency-only releases, it is acceptable for AppStream notes to say that the release includes an important security update and refreshed bundled components, as long as the wording stays user-friendly.

### 5. Translate release notes into all supported languages via gettext

Supported languages come from `po/LINGUAS`. English is the default source in `metainfo.xml.in` and does not need a `po/en.po` entry for release bullets.

Steps:

1. Run `scripts/extract-translations.sh` to refresh `po/actioneer.pot` with the new English msgids pulled from `data/metainfo.xml.in`.
2. For every non-English locale listed in `po/LINGUAS`, append a new `msgid` + `msgstr` pair to `po/<lang>.po` covering each new bullet:
   ```
   msgid "Your new English bullet text."
   msgstr "<translated text for this locale>"
   ```
   Preserve natural, concise phrasing rather than mechanically literal translations.
3. Validate every `.po` with `msgfmt -c -o /dev/null po/<lang>.po`.
4. Run `scripts/compile-translations.sh` — this compiles `.mo` catalogs into `po/locale/<lang>/LC_MESSAGES/` and renders the fully-translated `data/metainfo.xml` via `msgfmt --xml -L MetaInfo --template=data/metainfo.xml.in -d po`.

`po/it.po` and `po/ja.po` exist in the repo but are not in `LINGUAS` — do not translate new bullets into them.

If the repository gains or removes supported locales, update `release-context.md` to match.

### 6. Generate `RELEASE.md`

Write or update `RELEASE.md` for the GitHub release body.

Rules:

- `RELEASE.md` may be more technical than the AppStream changelog.
- Include:
  - a short summary
  - what changed
  - security or dependency notes when relevant
  - the commit list since the baseline tag
  - user impact / compatibility notes
- For dependency-only releases, explicitly say that there are no feature changes.
- Keep `RELEASE.md` aligned with the actual git range used for the release.

### 7. Run the required scripts from `scripts/`

For every Actioneer version bump, always run:

```bash
scripts/extract-translations.sh        # refresh po/actioneer.pot with new msgids
scripts/compile-translations.sh        # compile .mo catalogs and render data/metainfo.xml
scripts/regenerate-flatpak-sources.sh  # regenerate flatpak/me.spaceinbox.actioneer.cargo-sources.json
scripts/check-flatpak-lock-sync.sh     # verify Cargo.lock / flatpak manifest are in sync
```

Do **not** run unrelated helper scripts such as icon maintenance or local packaging build helpers unless the task explicitly changed those assets or packaging flows.

### 8. Validate

Unless the user explicitly narrows validation, run the repository-standard checks after the bump:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored
```

If the change is documentation-only and you deliberately skip some validation, say so explicitly in the final handoff.

## AppStream changelog quality bar

When generating the new AppStream release entry:

- Prefer 3-6 bullets.
- Summarize by theme, not by commit count.
- Mention translations only when they are user-visible and material to the release.
- Do not expose internal bug names, module names, or vague items like “various fixes”.
- Exception: for `maintenance` or `security` releases with little user-visible surface area, 1-2 simple bullets are preferred over forcing extra filler.

Good examples:

- `Workflow runs stay up to date while they are still active, so jobs and steps refresh in real time.`
- `Expanded run details are easier to follow, with clearer badges and more stable updates.`

Bad examples:

- `Refactored workflow_refresh.rs and fixed race conditions in JobRefreshContext.`
- `Updated Cargo.lock and cleaned up helper functions.`

## Final handoff

At the end, report:

1. The previous tag used as the changelog baseline.
2. Which files were updated.
3. Which scripts were run.
4. Which validation commands were run.
5. Which release kind was used, if relevant.
6. Any follow-up that still requires a human, if applicable.

## Maintenance note

If the release process changes, keep this skill aligned with:

- `docs/version-bump.md`
- `release-context.md`
- the actual version-bearing files in the repository
