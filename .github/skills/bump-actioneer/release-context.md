# Release context for `bump-actioneer`

This file is the repository-specific reference used by the `bump-actioneer` skill.

## Version targets

Update these files for every Actioneer release bump:

- `Cargo.toml` — `[package].version`
- `Cargo.lock` — regenerate with `cargo generate-lockfile`
- `snapcraft.yaml` — `version`
- `docs/flatpak.md` — example release tag
- `src/demo/logs/job-43021.log` — example `gh release create vX.Y.Z ...` command if present
- `RELEASE.md` — GitHub release notes / draft body

## AppStream release target

Add a new release entry to:

- `data/metainfo.xml.in` — the English-only source template (do **not** hand-edit `data/metainfo.xml`; it is generated)

Rules:

- Insert the new `<release>` entry at the top of the `<releases>` section.
- Keep all previous entries unchanged.
- Write only English bullets in `<li>` elements; translations come from `po/*.po` (see below).
- Match the existing XML formatting style.
- For maintenance/security-only releases, keep AppStream notes intentionally simple and non-technical.
- For dependency/security-only releases with no visible feature work, preferred AppStream themes are:
  - important security update
  - refreshed bundled components
  - more reliable / up-to-date experience

## AppStream translation pipeline (gettext)

As of commit `1cf85a5`, AppStream release notes are translated via gettext, not hand-authored `xml:lang` lists:

1. English bullets are added to `data/metainfo.xml.in` only.
2. `scripts/extract-translations.sh` extracts strings from Rust sources and `data/metainfo.xml.in` (via `xgettext --its=/usr/share/gettext/its/metainfo.its`) into `po/actioneer.pot`.
3. Each locale in `po/LINGUAS` has a `po/<lang>.po` file containing `msgid` + `msgstr` pairs. New release bullets must be appended as new entries in every non-English `.po` file.
4. `scripts/compile-translations.sh` compiles `.mo` catalogs into `po/locale/<lang>/LC_MESSAGES/` and renders the fully-translated `data/metainfo.xml` via `msgfmt --xml -L MetaInfo --template=data/metainfo.xml.in -d po`.
5. `data/metainfo.xml` is gitignored and rendered at CI time (see `appimage-ci.yml`).

`en.po` does not receive changelog translations — English is the default in the template.

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

Note: `po/it.po` and `po/ja.po` exist but are not listed in `LINGUAS` and are not shipped; do not translate new bullets into them.

### AppStream locale mapping

`msgfmt --xml` derives `xml:lang` values automatically from the `.po` language code. Reference:

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

## Script policy

For a normal version bump, the required scripts are:

```bash
scripts/extract-translations.sh      # refresh po/actioneer.pot with new msgids
scripts/compile-translations.sh      # compile .mo catalogs and render data/metainfo.xml
scripts/regenerate-flatpak-sources.sh
scripts/check-flatpak-lock-sync.sh
```

Usually **not** required for a plain release bump:

- `scripts/update-icons.sh`
- `scripts/remove-icons.sh`
- `scripts/flathub-build.sh`
- `scripts/snap-local.sh`

Only run those when the task explicitly changes icons or local packaging builds.

## Changelog generation baseline

Always build the new release notes from the latest reachable tag:

```bash
git describe --tags --abbrev=0
git --no-pager log --oneline <last-tag>..HEAD
git --no-pager diff --stat <last-tag>..HEAD
```

If commit subjects are too technical, inspect the relevant diffs/files and collapse them into a smaller set of user-facing improvements.

## GitHub release notes target

Write or update:

- `RELEASE.md`

Guidance:

- `RELEASE.md` is allowed to be more technical than `data/metainfo.xml.in`.
- Include summary, change details, commit list, and user impact.
- For dependency-only releases, explicitly note that there are no feature changes.

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
