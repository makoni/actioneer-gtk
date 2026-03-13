# Release context for `bump-actioneer`

This file is the repository-specific reference used by the `bump-actioneer` skill.

## Version targets

Update these files for every Actioneer release bump:

- `Cargo.toml` — `[package].version`
- `Cargo.lock` — regenerate with `cargo generate-lockfile`
- `snapcraft.yaml` — `version`
- `docs/flatpak.md` — example release tag
- `src/demo/logs/job-43021.log` — example `gh release create vX.Y.Z ...` command if present
- `TODO.md` — add a `Recent Updates` entry documenting the release bump work

## AppStream release target

Add a new release entry to:

- `data/metainfo.xml`

Rules:

- Insert the new `<release>` entry at the top of the `<releases>` section.
- Keep all previous entries unchanged.
- Use user-friendly bullets derived from the code/commits since the latest reachable git tag.
- Match the existing XML formatting style.

## Supported locales

Actioneer currently supports English plus the locales in `po/LINGUAS`.

Current language list:

- `en`
- `de`
- `nl`
- `zh_Hans`
- `hi`
- `es`
- `fr`
- `ar`
- `bn`
- `pt_BR`
- `ru`
- `ur`

### AppStream locale mapping

Use these `xml:lang` values in `data/metainfo.xml`:

- English: default `<li>` with no `xml:lang`
- `de` -> `de`
- `nl` -> `nl`
- `zh_Hans` -> `zh-Hans`
- `hi` -> `hi`
- `es` -> `es`
- `fr` -> `fr`
- `ar` -> `ar`
- `bn` -> `bn`
- `pt_BR` -> `pt-BR`
- `ru` -> `ru`
- `ur` -> `ur`

Preserve the same locale order already used in existing release entries.

## Script policy

For a normal version bump, the required scripts are:

```bash
scripts/regenerate-flatpak-sources.sh
scripts/check-flatpak-lock-sync.sh
```

Conditional scripts:

- `scripts/compile-translations.sh` — run only if gettext catalogs under `po/` changed.

Usually **not** required for a plain release bump:

- `scripts/extract-translations.sh`
- `scripts/update-icons.sh`
- `scripts/remove-icons.sh`
- `scripts/flathub-build.sh`
- `scripts/snap-local.sh`

Only run those when the task explicitly changes translations source extraction, icons, or local packaging builds.

## Changelog generation baseline

Always build the new release notes from the latest reachable tag:

```bash
git describe --tags --abbrev=0
git --no-pager log --oneline <last-tag>..HEAD
```

If commit subjects are too technical, inspect the relevant diffs/files and collapse them into a smaller set of user-facing improvements.

## Validation default

Default validation after the bump:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored
```

If a future release bump only changes metadata/documentation and validation is intentionally narrowed, that should be called out explicitly in the final summary.
