# TODO: Backlog

Status: ✅ Feature parity with the macOS client achieved — core work complete.

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

## Where to look

- UI: `src/ui/` (main_window, detail_view, run_jobs_window, job_logs_window)
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
  - Ensure logs load lazily per row and reuse the existing log-cache/code paths.
  - Add UI tests (ignored) that open/close drawers to guard against regressions.

- **Compact "Overview" page** — aggregates the last run status for pinned/favorite repositories using multi-pane cards.
  - Define the summary data structure (favorite repo -> last run digest) and extend the cache to supply it.
  - Build an `OverviewPage` with cards + refresh controls, adapting to narrow/wide layouts.
  - Add smoke tests ensuring the overview reflects cache updates and respects offline data.
