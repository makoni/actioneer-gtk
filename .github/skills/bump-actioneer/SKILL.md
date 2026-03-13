---
name: bump-actioneer
description: Bump Actioneer to a new release version, update all required release files, generate multilingual AppStream release notes from changes since the latest git tag, and run the required release scripts.
license: MIT
---

# bump-actioneer

Use this skill when the user wants to release a new Actioneer version and expects the repository metadata to be updated consistently.

## Required input

This skill requires one argument:

- `version`: the new application version in `X.Y.Z` format, for example `1.0.9`

Typical invocations:

- `Use bump-actioneer for version 1.0.9`
- `Bump Actioneer to 1.1.0 with bump-actioneer`

## Goal

Given a target version, this skill must:

1. Update the version everywhere Actioneer expects it.
2. Regenerate lock / packaging metadata that depends on the version or lockfile.
3. Add a new user-friendly release entry to `data/metainfo.xml`.
4. Generate AppStream release notes in **all supported application languages**.
5. Run the required scripts from `scripts/`.
6. Validate the result and summarize what changed.

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
4. Inspect the touched files if the commit subjects are too vague to build accurate release notes.

Do **not** invent changelog bullets from commit titles alone if the code tells a different story. Prefer grouping several technical commits into a smaller set of user-facing improvements.

### 2. Update every release-version target

Update the version in every file listed in `release-context.md`.

At minimum, that currently includes:

- `Cargo.toml`
- `Cargo.lock` (via regeneration, not hand-editing)
- `snapcraft.yaml`
- `docs/flatpak.md`
- `src/demo/logs/job-43021.log`
- `TODO.md` (Recent Updates entry)

If the repo later adds new release-version surfaces, update `release-context.md` and include them in the bump as well.

### 3. Regenerate dependent artifacts

After updating `Cargo.toml`, regenerate dependent files instead of editing them manually where possible:

```bash
cargo generate-lockfile
```

If `Cargo.lock` changes, run the required Flatpak sync scripts described below.

### 4. Generate `data/metainfo.xml` release notes

Add a new `<release>` entry at the top of the `<releases>` list.

Rules:

- Use the requested version for `version="..."`.
- Use the current date for `date="YYYY-MM-DD"`.
- Keep older release entries intact.
- Follow the existing formatting and locale order already used in `data/metainfo.xml`.
- Keep the changelog **user-friendly**:
  - describe improvements in terms of user-visible behavior
  - avoid raw implementation details, refactor notes, and dependency-noise unless it clearly benefits users
  - merge related technical changes into broader product-facing bullets

### 5. Generate release notes for all supported languages

Supported languages come from `po/LINGUAS`, plus English as the default non-`xml:lang` entry.

Current AppStream locale mapping is documented in `release-context.md`.

For each bullet:

1. Write the English source bullet first.
2. Add translated `<li xml:lang="...">...</li>` entries for every supported locale.
3. Preserve the existing locale order.
4. Keep wording natural and concise rather than mechanically literal.

If the repository gains or removes supported locales, update `release-context.md` to match.

### 6. Run the required scripts from `scripts/`

For a normal Actioneer version bump, always run the release-related scripts documented in `release-context.md`.

Today that means:

```bash
scripts/regenerate-flatpak-sources.sh
scripts/check-flatpak-lock-sync.sh
```

Additionally:

- Run `scripts/compile-translations.sh` only if the bump also changed gettext catalogs under `po/`.
- Do **not** run unrelated helper scripts such as icon maintenance or local packaging build helpers unless the task explicitly changed those assets or packaging flows.

### 7. Validate

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

When generating the new release entry:

- Prefer 3-6 bullets.
- Summarize by theme, not by commit count.
- Mention translations only when they are user-visible and material to the release.
- Do not expose internal bug names, module names, or vague items like “various fixes”.

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
5. Any follow-up that still requires a human, if applicable.

## Maintenance note

If the release process changes, keep this skill aligned with:

- `.github/instructions/version-bump.instructions.md`
- `release-context.md`
- the actual version-bearing files in the repository
