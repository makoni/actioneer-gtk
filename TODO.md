# TODO: Backlog

Status: ✅ Feature parity with the macOS client achieved — core work complete.

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

## Where to look

- UI: `src/ui/` (main_window, detail_view, job_logs_window)
- API: `src/api/` (client, models, http helpers)
- Auth & storage: `src/auth/`, `src/storage/token_storage.rs`
- Tests: `tests/` and unit tests in `src/`

## Quick validation (local)

CI runs only on `workflow_dispatch` (see `AGENTS.md`), so validate locally:

1. Format: `cargo fmt`
2. Lint: `cargo clippy --all-targets --all-features`
3. Unit & logic tests: `cargo test`
4. UI tests (requires display): `cargo test -- --ignored` (or `xvfb-run`)
5. If `Cargo.lock` changed: `scripts/regenerate-flatpak-sources.sh` then `scripts/check-flatpak-lock-sync.sh`

## Recent Updates

- 2026-08-25: Started [🔄] and completed [✅] release prep for `1.1.0`: bumped Cargo/Snap versions (`1.0.15` → `1.1.0`), updated `Cargo.lock` via `cargo update -p actioneer`, regenerated Flatpak cargo sources from the new lock, added the `1.1.0` AppStream changelog entry (translated to all 13 non-English locales, now including Italian and Japanese) with re-shot screenshot dimensions, rewrote `RELEASE.md` notes with commits since `1.0.15`, dropped the stale `v` tag prefix from `docs/flatpak.md`, the demo log, and the publish workflow input description to match real tag naming, and ran the release validation checks.
- 2026-08-10: Started [🔄] and completed [✅] Workflows view redesign (feature branch `feature/workflows-redesign`): restructured the repo detail pane into a single rounded workflows card with an accordion (workflow → runs → jobs → steps), round tinted status dots (`status-dot`), per-row mono meta lines, a pinned pane header (repo title + visibility badge + "N workflows · updated …" + segmented status-count filter), running-workflow progress bar with live elapsed timer, "Recent runs" sub-header with "shown N of M · All on GitHub" link, ghost row-action buttons (logs/trigger/cancel wired to `JobLogsWindow` and the cancel confirmation), sidebar polish (All/Favorites/Active pills, uppercase owner headers, icon+star rows, solid-accent selection), enriched demo data with relative timestamps and multi-step jobs, and 13 new translated strings across all 11 locales. Verified with `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`, Xvfb UI tests, and `scripts/check-flatpak-lock-sync.sh`.
- 2026-06-23: Started [🔄] and completed [✅] release bump to `1.0.15` (maintenance): updated version targets, added AppStream + GitHub changelog entries based on commits since `1.0.14`, refreshed gettext/Flatpak artifacts, and ran release validation checks.
- 2026-06-30: Started [🔄] and completed [✅] dependency maintenance pass: merged pending Dependabot updates, refreshed Cargo deps (`gio`, `gtk4`, `open`, `chacha20poly1305` major), and synchronized Flatpak cargo sources with `Cargo.lock`.

## Open items (optional / low priority)

These are enhancement ideas, not required work — none has been started.

- **Enhanced streaming job logs (advanced viewer)**
  - Prototype incremental log streaming in the API client (chunked transfer, retries, resume markers).
  - Build a streaming log viewer widget with live append and search affordances.
  - Add integration tests that simulate slow/partial streams so we don't regress buffering or cancellation.

- **Inline job-log drawer** — expandable from each job row instead of opening a separate window.
  - Design a row-level drawer widget (likely `AdwExpanderRow`/`AdwClamp`) that embeds the log viewer.
  - Ensure logs load lazily per row and reuse `JobLogsWindow`'s per-window log cache.
  - Add UI tests (ignored) that open/close drawers to guard against regressions.

- **Compact "Overview" page** — aggregates the last run status for pinned/favorite repositories using multi-pane cards.
  - Define the summary data structure (favorite repo -> last run digest) and extend the cache to supply it.
  - Build an `OverviewPage` with cards + refresh controls, adapting to narrow/wide layouts.
  - Add smoke tests ensuring the overview reflects cache updates and respects offline data.
