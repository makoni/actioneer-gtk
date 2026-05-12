## TODO: Project status (short)

Status: ✅ Feature parity with macOS achieved — main work complete.

Keep this short. Purpose: quick status, where to look, and how to validate.

Key achievements
- Feature parity with macOS (runs, jobs, actions, trigger, auto-refresh)
- Cache-first loading for workflows/runs/jobs (in-memory DataCache)
- Job logs viewer, desktop notifications, and toast feedback
- Robust error handling, retries, and headless-safe UI tests

Where to look
- UI: `src/ui/` (main_window, detail_view, run_jobs_window, job_logs_window)
- API: `src/api/` (client, models, http helpers)
- Auth & storage: `src/auth/`, `src/storage/token_storage.rs`
- Tests: `tests/` and unit tests in `src/`

Quick validation (local)
1. Unit & logic tests: `cargo test`
2. UI tests (requires display): `cargo test -- --ignored`
3. Lint: `cargo clippy -- -D warnings`
4. Format: `cargo fmt`

## UI safety/stability remediation plan (2026-03-03)

Goal: address the GTK/Tokio/threading issues identified in the deep review, with safety and UI responsiveness first.

### Phase 1 — must-fix stabilization (ship first)

1. [✅] **Auth cancel/poll race hardening** (`src/ui/auth_window.rs`)
   - Track spawned polling task handle(s) and abort on:
     - Cancel button click
     - Dialog close/hide
   - Add an auth-attempt generation id so stale `PollSuccess`/`PollError` messages are ignored.
   - Ensure canceled auth can never call `save_token_and_close` or `on_success`.
   - Validation:
     - Manual: open auth, cancel, then complete device flow in browser; app must stay signed out.
     - Add unit/integration coverage for stale message suppression.

2. [✅] **Move secure token I/O off GTK main thread** (`src/ui/main_window.rs`, `src/ui/auth_window.rs`, `src/storage/*`)
   - Introduce an async auth/storage service boundary used by UI code.
   - Replace direct synchronous calls in signal/focus handlers (`TokenStorage::new/get_token/delete_token/save_token`) with Tokio-side work and GLib UI handoff.
   - Keep GTK object access strictly on GLib main context.
   - Validation:
     - Manual sign-in/sign-out/focus checks while interacting with UI; no visible freezes.
     - Logging confirms storage work runs off main thread.

3. [✅] **Fix disabled-refresh busy loop** (`src/ui/main_window/refresh.rs`)
   - Respect `refresh_interval == 0` as disabled without looping/sleep(0).
   - Re-arm background refresh only when preferences change to non-zero.
   - Validation:
     - Set refresh to disabled and verify no hot loop/high CPU.
     - Re-enable refresh and verify timer resumes.

4. [✅] **Remove unsafe runtime-time env mutation pattern** (`src/i18n.rs`, `src/notifications.rs`, `src/main.rs`)
   - Stop mutating process env after runtime startup; initialize once during startup or pass explicit runtime config.
   - Keep behavior for snap/portal app-id resolution unchanged.
   - Validation:
     - Notification routing still works (native + portal path).
     - Language preference and startup locale behavior unchanged.

### Phase 2 — concurrency/perf hardening

5. [✅] **Timer lifecycle ownership cleanup** (`src/ui/detail_view/helpers/workflows.rs`, `src/ui/detail_view/workflow_refresh.rs`)
   - Replace ad-hoc expander timer data management with explicit timer ownership in pane state.
   - Ensure timers are canceled when rows/panes are rebuilt or destroyed.
   - Validation: no stale follow-up refreshes after pane switch/close.

6. [✅] **Bound or coalesce UI channels where producers can burst** (`src/ui/utils/channel.rs` callers)
   - Replace unbounded channels for burst-prone paths or add coalescing/debouncing.
   - Validation: stress refresh paths and confirm stable memory behavior.

7. [✅] **Make preference writes non-blocking for async contexts** (`src/preferences.rs`)
   - Move `fs::write` to `tokio::fs` or `spawn_blocking`.
   - Validation: preference changes remain responsive and persistent.

### Phase 3 — architecture/testability follow-through

8. [✅] **Extract high-risk large files into focused modules**
   - Prioritize: `src/ui/main_window.rs`, `src/ui/job_logs_window.rs`, `src/ui/detail_view/helpers/workflows.rs`, `src/ui/detail_view/workflow_refresh.rs`, `src/notifications.rs`.
   - Proposed boundaries:
     - Auth/session controller
     - Focus/activation handlers
     - Refresh scheduler + lifecycle
     - Workflow row/render helpers
     - Notification dispatch adapters
   - Validation: equivalent behavior, smaller units, targeted tests per module.

9. [✅] **Reduce unsafe widget-data patterns where feasible**
   - Add typed wrappers for widget data keys and centralize lifecycle assumptions.
   - Preserve existing behavior while shrinking unsafe surface area.

### Exit criteria before marking this plan complete

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo test -- --ignored` (with display or `xvfb-run`)
- Manual smoke:
  - Sign in/out flow
  - Window focus auth recovery path
  - Auto-refresh enable/disable transitions
  - Notification test action on supported environments

Optional / next steps (low priority)
- [✅] Review and merge open dependency-update PRs (#29-#32), then run a full `cargo update` refresh with the standard validation suite.
- Enhanced streaming job logs (advanced viewer)
  - Prototype incremental log streaming in the API client (chunked transfer, retries, resume markers).
  - Build a streaming log viewer widget with live append and search affordances.
  - Add integration tests that simulate slow/partial streams so we don’t regress buffering or cancellation.
- Introduce an inline job-log drawer that can be expanded from each job row instead of opening a separate window.
  - Design a row-level drawer widget (likely `AdwExpanderRow`/`AdwClamp`) that embeds the log viewer.
  - Ensure logs load lazily per row and reuse the existing log-cache/code paths.
  - Add UI tests (ignored) that open/close drawers to guard against regressions.
- Provide a compact “Overview” page that aggregates the last run status for pinned repositories (favorites) using multi-pane cards.
  - Define the summary data structure (favorite repo -> last run digest) and extend the cache to supply it.
  - Build an `OverviewPage` with cards + refresh controls, adapting to narrow/wide layouts.
  - Add smoke tests ensuring the overview reflects cache updates and respects offline data.
- [✅] Rework `DataCache` to store `Arc<[WorkflowRun]>` / `Arc<[Workflow]>` snapshots or `Arc<Vec<T>>` so cache hits hand out cheap references instead of cloning entire vecs.
  - [✅] Introduce snapshot types and adjust cache setters/getters to clone `Arc` handles only.
  - [✅] Update downstream call sites (filters, overview, notifications) to accept shared slices instead of owned `Vec`s.
  - [✅] Add tests verifying we hand back shared snapshots (strong-count checks guard against Vec reallocations during refresh loops).

## Secret portal migration plan

Context: Snap reviewers requested that we rely on `org.freedesktop.portal.Secret` (available since Ubuntu 20.04) instead of the `password-manager-service` plug. The secret portal’s `RetrieveSecret` contract is documented in the Flatpak portal reference and XML spec ([docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Secret.html), [schema](https://github.com/flatpak/xdg-desktop-portal/blob/master/data/org.freedesktop.portal.Secret.xml)). It returns a per-app secret via a pipe fd and emits the usual `org.freedesktop.portal.Request` response, so our GLib code must watch that handle.

1. [✅] Default-enable portal detection and add sandbox heuristics in `src/storage/secret_portal.rs`/`TokenStorage::new` (env vars + sandbox env vars) so we automatically attempt the portal on confined builds, with opt-out logging for hosts without it.
2. [✅] Implement a real `PortalTokenStore` that talks to `org.freedesktop.portal.Secret` via `gio` (fd passing + request handling) to derive the secret, encrypt tokens, and integrate it into `TokenStorage` so sandboxed builds prefer the portal while classic builds continue using `keyring`.
3. [✅] Migrate existing credentials: when the portal becomes available inside a sandbox, read any existing host keyring token once, copy it into the portal collection, and clean up the legacy entry to avoid drift.
4. [✅] Update packaging and docs (`snapcraft.yaml`, `docs/snapcraft_ai_guide.md`, `README.md`) to document the portal requirement, remove `password-manager-service`, and describe manual verification steps for Snap reviewers.
5. [✅] Extend test coverage: add unit tests for the portal path, document a manual regression matrix in `TEST_INSTRUCTIONS.md`, and ensure portal failures surface in logs during CI/manual runs.

Notes
- Full history and detailed session notes are preserved in git commits.
- For larger changes, run the quick validation steps above and ensure `cargo clippy -- -D warnings` passes.

---

-Recent Updates
- [✅] 2026-05-12 — Reviewed and merged Dependabot PRs #29-#32 (`openssl` 0.10.78 → 0.10.79, `tokio` 1.52.1 → 1.52.3 with `Cargo.toml` floor raised to 1.52, `actions-rust-lang/setup-rust-toolchain` 1.16.0 → 1.16.1, and `getrandom` 0.3.4 → 0.4.2), then ran `cargo update`, which refreshed `Cargo.lock` to newer Rust 1.95-compatible transitive releases (including `keyring` 4.0.1, `open` 5.3.5, `tower-http` 0.6.10, `zvariant` 5.11.0, and the `turso*` pre.30 set). Regenerated `flatpak/me.spaceinbox.actioneer.cargo-sources.json`, fixed an order-dependent i18n unit test in `src/ui/detail_view/helpers/formatting.rs` by serializing it with the existing i18n test guard and asserting against `tr("Success")`, and revalidated with `scripts/regenerate-flatpak-sources.sh`, `scripts/check-flatpak-lock-sync.sh`, `cargo fmt --all -- --check`, `scripts/compile-translations.sh`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-05-03 — Updated the remaining direct Cargo dependencies that still had newer adoptable releases: migrated secure storage from `keyring` 3.6 to the split `keyring` 4.0 / `keyring-core` 1.0 model, replaced the legacy direct `rand_core` usage with `getrandom` 0.3 for nonce generation, regenerated `Cargo.lock` plus `flatpak/me.spaceinbox.actioneer.cargo-sources.json`, and revalidated with `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `scripts/regenerate-flatpak-sources.sh`, and `scripts/check-flatpak-lock-sync.sh`.
- [✅] 2026-05-03 — Merged Dependabot PRs #28 (`reqwest` 0.13.2 → 0.13.3) and #27 (gtk-rs patch updates), then fixed a follow-up CI regression on `develop`: the combined post-merge `Cargo.lock` no longer matched `flatpak/me.spaceinbox.actioneer.cargo-sources.json`, so the Flatpak cargo-sources manifest was regenerated to restore `scripts/check-flatpak-lock-sync.sh` / CI lock-sync parity.
- [✅] 2026-04-23 — Bumped Actioneer to `1.0.12` across release metadata and packaging files (`Cargo.toml`, `snapcraft.yaml`, `docs/flatpak.md`, `src/demo/logs/job-43021.log`), regenerated `Cargo.lock` with `cargo generate-lockfile`, added a new single-bullet maintenance release entry to `data/metainfo.xml.in` covering the gtk-rs-core 0.22 migration + reqwest 0.13 + sha2 0.11 + ashpd 0.13 + GitHub Actions bumps since tag `1.0.11`, extracted the new msgid with `scripts/extract-translations.sh` and added translations for all 11 non-English locales in `po/*.po`, rendered `.mo` catalogs and `data/metainfo.xml` with `scripts/compile-translations.sh`, and drafted `RELEASE.md`; additionally hardened `scripts/check-flatpak-lock-sync.sh` to compare cargo-sources content via `flatpak-cargo-generator` (falling back to the old git-diff heuristic) — the heuristic produced false positives on version-only bumps where Cargo.lock only touches the `actioneer` self-entry — and added the generator install to `.github/workflows/lock-sync.yml`. Ran `scripts/regenerate-flatpak-sources.sh` and `scripts/check-flatpak-lock-sync.sh`, plus full validation with `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-04-23 — Merged 4 Dependabot PRs across ecosystem updates: #20 sha2 0.10.9 → 0.11.0 (transparent bump), #21 GitHub Actions group (checkout v5→v6, download-artifact v7→v8, setup-rust-toolchain 1.15.3→1.16.0, cache 5.0.3→5.0.5, gh-release v2→v3), #22 reqwest 0.12.28 → 0.13.2 (added `query` feature which is no longer in 0.13 defaults; validated TLS via live smoke against api.github.com returning 200, rustls is now the default backend), and #23 ashpd 0.12.3 → 0.13.10 (opted into `notification` + `secret` features that are now gated in 0.13; adapted `Secret::retrieve` to the new two-argument signature with `Default::default()` options). Closed #19 + #24 as `gio`-only bumps that cannot compile without coordinated peers. Added `.github/dependabot.yml` grouping `gtk-rs-core` crates into one `gtk-rs` PR and blocking `version-update:semver-major` for all of them; dismissed the phantom `rand 0.8.6` security alert with rationale (keyring 3.6's unused `async-secret-service` optional dep is the only source).
- [✅] 2026-04-23 — Migrated to gtk-rs-core 0.22 (`gtk4` 0.10 → 0.11, `libadwaita` 0.8 → 0.9, `gio` 0.21 → 0.22) as a single coordinated commit that Dependabot can never produce alone. Reimplemented the Unix `SIGINT`/`SIGTERM` handler in `src/main.rs`: `glib::source::unix_signal_add_local` was dropped in glib 0.22, replaced with `tokio::signal::unix` + `MainContext::invoke` bridge that preserves the original `mark_current_session_clean` + `app.quit()` semantics. System libs on Linux support both 0.10 and 0.11 generations (minimum gtk4 4.0.0); our local dev host and CI runners both qualify. Validated with full `cargo fmt/clippy/test/build/xvfb-run --ignored` plus the AT-SPI smoke test on the release binary.
- [✅] 2026-04-21 — Bumped Actioneer to `1.0.11` across release metadata and packaging files (`Cargo.toml`, `snapcraft.yaml`, `docs/flatpak.md`, `src/demo/logs/job-43021.log`), regenerated `Cargo.lock` with `cargo generate-lockfile`, added a new single-bullet maintenance release entry to `data/metainfo.xml.in` covering the Cargo.lock refresh since tag `1.0.10`, extracted the new msgid with `scripts/extract-translations.sh` and added translations for all 11 non-English locales in `po/*.po`, rendered `.mo` catalogs and `data/metainfo.xml` with `scripts/compile-translations.sh`, updated `.github/skills/bump-actioneer/release-context.md` to reflect the new gettext-based AppStream pipeline, and drafted `RELEASE.md`; ran `scripts/regenerate-flatpak-sources.sh` and `scripts/check-flatpak-lock-sync.sh`, plus full validation with `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-23 — Bumped Actioneer to `1.0.10` across release metadata and packaging files, regenerated `Cargo.lock`, refreshed `flatpak/me.spaceinbox.actioneer.cargo-sources.json`, added a new simple multilingual AppStream release entry in `data/metainfo.xml` based on the dependency/security-only changes since tag `1.0.9`, and created a technical GitHub release note draft in `RELEASE.md`; ran the required release scripts (`scripts/regenerate-flatpak-sources.sh` and `scripts/check-flatpak-lock-sync.sh`) plus full validation with `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Continued the GitHub Actions Node 24 cleanup in `publish.yml`: bumped all release artifact download steps from `actions/download-artifact@v6` to `actions/download-artifact@v7` after confirming upstream `v7` ships `runs.using: node24`. Follow-up online audit shows the remaining warnings now come from third-party actions that still publish `node20` runtimes upstream (`softprops/action-gh-release@v2`, `canonical/action-build@v1`, and `flatpak/flatpak-github-actions/flatpak-builder@v6` / current `master`), so those cannot be eliminated from our workflows until their maintainers release Node 24-compatible versions or we replace the actions entirely.
- [✅] 2026-03-13 — Finished the remaining GitHub Actions Node 24 migration in `.github/workflows/`: updated all still-warning artifact upload steps from `actions/upload-artifact@v5` to `actions/upload-artifact@v7` in `appimage-ci.yml`, `flatpak-ci.yml`, and `snap-ci.yml`, removing the last known Node 20 deprecation path from repository-owned workflow files.
- [✅] 2026-03-13 — Fixed a false-positive crash-reporter path for terminal shutdowns: Actioneer now listens for Unix `SIGINT`/`SIGTERM` on the GLib main loop, marks the current session clean, and then quits through the normal application shutdown path instead of leaving an “abnormal exit” marker behind. This prevents reports from appearing on the next launch after closing the app with `Ctrl+C` in a terminal; validated with `cargo fmt --all`, a targeted isolated `SIGINT` run using a temporary `XDG_STATE_HOME`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Reduced Actioneer's default Tokio runtime size for desktop usage: startup now caps the background runtime to at most 4 worker threads by default (while keeping an `ACTIONEER_TOKIO_WORKER_THREADS` override for profiling/tuning), logs the chosen runtime sizing at startup, and adds unit tests for the default/override parsing. On a 24-core host this cut the live process thread count from 36 to 16 during startup/idle checks, while RSS/PSS stayed roughly flat (~172 MB RSS, ~78 MB PSS before vs ~171 MB RSS, ~77 MB PSS after); validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Bumped Actioneer to `1.0.9` across release metadata and packaging files, regenerated `Cargo.lock` plus the Flatpak cargo-sources manifest, added a new multilingual AppStream release entry in `data/metainfo.xml` based on changes since tag `1.0.8`, and ran the required release scripts (`scripts/regenerate-flatpak-sources.sh` and `scripts/check-flatpak-lock-sync.sh`); validated with `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Added a new repository skill at `.github/skills/bump-actioneer/`: the skill now defines a reusable release/version-bump workflow that takes a target version, updates all known release-version files, generates a user-friendly multilingual AppStream changelog in `data/metainfo.xml` from commits since the latest reachable git tag, and runs the required release scripts from `scripts/`; also added `release-context.md` with the current file targets, locale mapping, script policy, and validation defaults. Validated with `cargo fmt --all` and `cargo check`.
- [✅] 2026-03-13 — Started the GitHub REST API migration to version `2026-03-10`: the shared `reqwest` client now sends explicit `X-GitHub-Api-Version` and canonical `Accept: application/vnd.github+json` headers, supports a temporary `ACTIONEER_GITHUB_API_VERSION` override for rollback/testing, logs the effective API version at client startup, hardens workflow-file loading so `contents` directory/non-file responses fail explicitly instead of surfacing as opaque parse errors, and adds compatibility coverage for trimmed repository/run payloads plus real header-propagation tests; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`. Manual live-GitHub smoke testing still remains as a final follow-up.
- [✅] 2026-03-13 — Fixed the remaining valid issues from the external bug report: HTTP `403` responses are no longer misclassified as auth failures/sign-out events (`401` still is), the custom UI channel sender now uses a non-poisoning `parking_lot::Mutex`, workflow load/reset paths in `workflow_refresh.rs` now recover even if the background worker drops before replying, and transient workflow-load errors no longer wipe the last successfully rendered workflow list; added regression tests and revalidated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Hardened the jobs Retry path against rerenders: when a preserved error box is retried after a run-row rebuild, the handler now resolves the current `expander` / `jobs_box` / `badges_box` from `JobRefreshContext` for that run instead of reusing stale captured widgets from an older row instance; added an ignored GTK regression test and revalidated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Extended run badge caching from a single workflow-row instance to the whole detail pane: per-run job-summary badges are now stored in a pane-scoped cache and reused across workflow-row rebuilds, so collapsed run counters survive not only run-row rerenders but also full workflow-list refresh/rebuild cycles; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Fixed another completed-run auto-reexpand race after external/manual runs: collapsing a run now always clears its matching `JobRefreshContext` even if the row gets detached/rebuilt before the deferred cleanup runs, so stale collapsed contexts can no longer survive and re-seed expanded-run preservation during the next 5-second background refresh; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Preserved run job-summary badges after collapsing rows: the detail view now keeps a lightweight per-run `JobSummary` cache separate from `JobRefreshContext`, so background rerenders can still restore queued/running/completed badges for collapsed runs even after the expanded jobs widget is torn down; added an ignored GTK regression test for cached badge restoration and stabilized the notification conclusion unit test with an explicit i18n test guard/language reset; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Fixed a GLib timer teardown panic in the detail view: follow-up refresh timers and the main auto-refresh timer now remove `glib::SourceId`s through a non-panicking helper, and follow-up timers clear their stored handle when they stop themselves so stale source IDs cannot be removed a second time later; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Hardened post-dispatch refresh lifecycle after auditing the detail view: the per-workflow follow-up timer now has real stop conditions, is protected against duplicate timer registration races, preserves status badges during trigger follow-up refreshes, no longer reuses stale queued expanded-run snapshots, and active workflow expanders now keep their expansion state even after being marked `_ACTIVE` during list rebuilds; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-13 — Fixed a run auto-reexpand race after completion: background/follow-up refreshes no longer treat every surviving `JobRefreshContext` as a signal to reopen a run; only contexts whose expander is still genuinely expanded and mounted may preserve expansion now, so a user-collapsed completed run stays collapsed during the final refresh cycles; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-12 — Audited `.github/workflows/` for the GitHub Node 20 deprecation warning: bumped `actions/cache` from `v4` to `v5.0.3` in CI and pinned `actions-rust-lang/setup-rust-toolchain` to `v1.15.3` while disabling its built-in cache everywhere we use it, because the upstream composite action still embeds `Swatinem/rust-cache@v2.8.2` on Node 20; this removes the warning path without waiting for an upstream `setup-rust-toolchain` release.
- [✅] 2026-03-12 — Polished sidebar activation after the filter-selection fix: activating a repository row now acts as an authoritative open action, so clicking an already highlighted repo can still resynchronize the detail pane if selection visuals and right-pane content ever drift apart again; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-12 — Fixed the sidebar search/selection mismatch: when filtering hides the previously selected repository, the sidebar now synchronizes `SingleSelection`, `selected_repo_id`, and the detail pane to the first visible repository (or clears the detail view if nothing matches), so the right pane no longer stays stuck on stale repo data; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-12 — Bumped the app to 1.0.8 across release metadata and packaging files, regenerated the Flatpak cargo-sources manifest from the refreshed lockfile, added localized AppStream 1.0.8 release notes for all supported languages, and prepared `RELEASE.md` with GitHub release notes based on commits since `1.0.7`.
- [✅] 2026-03-12 — Restored live job/step refresh after the initial job-load dedupe by fixing a detail-pane activation bug: the selected pane was being deactivated when its widget was finally attached, which killed the main auto-refresh loop and left only the follow-up run poller; the detail view now keeps the currently active pane alive while still rejecting stale deferred attachments from older selections; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-12 — Removed the duplicate initial jobs fetch for freshly expanded runs by storing a provisional per-run job refresh context before the API response arrives, so rerendered expanded rows reuse the in-flight jobs box instead of starting a second request; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed stale detail-pane polling after repo switches by explicitly deactivating the previous `RepoDetailPane`, teaching auto-refresh timers and preference listeners to stop once a pane becomes inactive, and hardening the detail favorite observer to release dead buttons instead of holding them through a long-lived channel subscription; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed three regressions after the polling refactor: the top rate-limit label now refreshes again during active runs using cached response metadata, expanded run job summary badges are restored immediately from cached job data instead of disappearing between rerenders, and job-step rows now use contiguous user-facing numbering rather than raw GitHub step numbers with gaps; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Cut GitHub API bursts at startup and run trigger time: sidebar repo status polling is now lazy and selected-repo-first instead of fanning out across many repos immediately after login, the detail pane owns selected-repo workflow polling, and duplicate in-flight job fetches for the same run are now suppressed; validated with `cargo fmt --all`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed the sidebar `Gtk-CRITICAL` by replacing `gtk::ListBoxRow` items in the repo `gtk::ListView` with plain `gtk::Widget` rows that carry selection metadata via widget data, and stopped stale detail-pane preference listeners from re-arming duplicate auto-refresh timers after pane teardown; added regression coverage and revalidated with `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed the deeper live-refresh race for in-progress runs: run-list rerenders now preserve expansion from the current UI state and active job contexts instead of stale request-time snapshots, and detail-pane auto-refresh now tracks preference changes live without being torn down by temporary `RepoDetailPane` clones; validated with `cargo fmt --all`, `cargo test`, `cargo check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed live job refresh for in-progress runs: preserved/rebound `JobRefreshContext` across run-list rerenders, tied context removal to the currently mounted `Expander` instead of stale teardown callbacks, and added GTK regression tests so expanded runs keep fetching updated jobs/steps while the workflow is still running; validated with `cargo fmt --all`, `cargo test`, `cargo check`, and targeted ignored GTK tests under `xvfb-run`.
- [✅] 2026-03-11 — Fixed live job-step visibility in expanded workflow runs: the app now parses the GitHub Actions jobs API `steps` array, renders all available steps immediately under each job, and updates their statuses during background refresh instead of waiting until the whole run completes; validated with `cargo fmt --all`, `cargo test`, `cargo check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run ... cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed a conditional-request edge case in `src/api/http.rs`: when GitHub returned `304 Not Modified`, the app now reuses the cached body even if the local cache TTL expired after `If-None-Match` was attached but before the response was processed; added a regression test for this 304/TTL race and revalidated with `cargo test` + `cargo check`.
- [✅] 2026-03-11 — Reduced GitHub API pressure in three places: added selective opt-in ETag handling for low-churn REST GETs (`list_repos`, `list_workflows`, `list_branches`, workflow-file contents), switched sidebar repo summaries to a repo-wide Actions runs query with targeted fallback only for truncated missing workflows, and upgraded workflow dispatch to request `return_run_details` so newly triggered runs can appear immediately in the detail list; validated with `cargo fmt --all`, `cargo test`, `cargo check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `xvfb-run ... cargo test -- --ignored`.
- [✅] 2026-03-11 — Fixed untranslated workflow status badges near workflow headers: the UI now reuses existing localized status strings (`Success`, `Failed`, `In Progress`, `Queued`, `Cancelled`, `Unknown`) across all supported languages, with regression coverage.
- [✅] 2026-03-04 — Replaced startup crash `MessageDialog` with a translated crash-report popover containing a read-only text field with report contents and three actions: “Copy diagnostics”, “Report Issue”, and “Close”; only “Close” dismisses the popover (and clears pending marker).
- [✅] 2026-03-03 — Implemented full crash-recovery reporting flow: panic hook now persists sanitized crash reports to XDG state, session lifecycle writes/clears markers and generates abnormal-exit reports, next launch shows a recovery dialog (Report Issue / Copy diagnostics / Open crash folder / Dismiss), and GitHub issue links are prefilled from pending crash metadata; validated with fmt, clippy, translation compile, unit tests, and ignored GTK tests.
- [✅] 2026-03-03 — Added a debug-build-only app-menu Debug group containing “Send test notification” and “Trigger test crash”; wired `win.trigger_test_crash` to an intentional panic path so panic-hook/backtrace logging can be verified end-to-end.
- [✅] 2026-03-03 — Localized sign-out confirmation button labels across all shipped locale catalogs (`po/LINGUAS`): translated `Yes`/`No` for `de`, `nl`, `zh_Hans`, `hi`, `es`, `fr`, `ar`, `bn`, `pt_BR`, `ru`, and `ur` (plus existing `it`/`ja` catalogs), and revalidated with `scripts/compile-translations.sh`.
- [✅] 2026-03-03 — Updated menu/dialog UX polish: shortcuts dialog now keeps the Close button anchored at the bottom-right, sign-out confirmation uses localized app strings for Yes/No buttons, and Donate was moved from About details into the app menu (right before Quit) with a dedicated `app.donate` action.
- [✅] 2026-03-03 — Fixed a crash when closing Keyboard Shortcuts by replacing the `gtk::ShortcutsWindow` composition path with a stable modal `AdwWindow` shortcuts dialog in `src/ui/main_window/window_actions.rs`, and added an ignored GTK regression test (`shortcuts_window_builds_and_closes`) to exercise open/close lifecycle.
- [✅] 2026-03-03 — Fixed non-English sign-out dialog stability by setting dialog text via `set_text` (avoiding format-string parsing), switched About links to top-level `website` + `support_url` in `AdwAboutWindow`, added panic hook logging with captured backtraces in `main.rs`, and updated locale application to call `setlocale` with explicit locale codes so libadwaita About UI strings follow the selected app language.
- [✅] 2026-03-03 — Installed gettext-dependent translation validation flow: removed duplicate `msgid` entries from all `po/*.po` catalogs so `scripts/compile-translations.sh` succeeds, and updated `.github/workflows/ci.yml` to install `gettext` in CI test/lint/UI jobs plus run translation compilation in the build/test job.
- [✅] 2026-03-03 — Refined the 1.0.7 AppStream changelog after reviewing commits since `v1.0.6` (including explicit translation coverage updates) and added a donation link (`https://nowpayments.io/donation/makoni`) to both `data/metainfo.xml` and the About window (`src/ui/main_window/window_actions.rs`), with translated “Donate” labels across all supported `po/LINGUAS` locales.
- [✅] 2026-03-03 — Bumped release to 1.0.7 across `Cargo.toml`/`Cargo.lock`, `snapcraft.yaml`, `docs/flatpak.md`, and demo release logs; regenerated `flatpak/me.spaceinbox.actioneer.cargo-sources.json` via the required scripts and added localized, user-friendly AppStream 1.0.7 release notes in `data/metainfo.xml` for all supported languages.
- [✅] 2026-03-03 — Completed Phase 3 modularization follow-through: split job log rendering into `src/ui/job_logs_window/render.rs`, moved workflow follow-up timer lifecycle into `src/ui/detail_view/helpers/workflow_follow_up.rs`, extracted workflow expander scanning helpers to `src/ui/detail_view/workflow_refresh/scan.rs`, and split notification icon resolution into `src/notifications/icon.rs`; full validation passed (`cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo test -- --ignored`).
- [✅] 2026-03-03 — Continued Phase 3 modularization: moved main-window action/help/preferences/about/notification window logic into `src/ui/main_window/window_actions.rs` and split notification sandbox/env/app-id helpers into `src/notifications/env_config.rs`.
- [✅] 2026-03-03 — Started Phase 3: extracted main-window auth/session/focus/sign-out logic into `src/ui/main_window/auth.rs` and introduced typed widget-data helpers (`src/ui/utils/widget_data.rs`), replacing unsafe widget-data access in sidebar/detail modules.
- [✅] 2026-03-03 — Implemented Phase 1+2 remediation: auth attempt generation + poll cancellation, token storage moved off GTK thread paths, refresh=0 loop fix, startup-only portal/i18n env setup, follow-up timer teardown on pane drop, bounded UI channels, and async preference writes (with new regression tests).
- [✅] 2026-03-03 — Added a detailed, phased remediation TODO plan for GTK UI-thread safety, auth/task races, refresh-loop fixes, and architecture follow-up from the deep code review.
- [✅] 2026-03-02 — Updated `data/metainfo.xml` with German and Dutch localized summary/description/features/screenshot captions and 1.0.6 release notes; AppStream validation now passes locally and via `org.flatpak.Builder`.
- [✅] 2026-03-02 — Added German and Dutch locale support in preferences/system-locale detection, enabled `de`/`nl` catalogs in `po/LINGUAS`, added `po/nl.po`, and backfilled missing German translations.
- [✅] 2026-02-27 — Bumped app version to 1.0.6 across configs/docs, added AppStream release notes, and refreshed Flatpak cargo-sources metadata.
- [✅] 2026-02-16 — Ran a full UI localization audit across all `tr(...)` strings, fixed remaining code-level hardcoded run status/time/log-title text, and backfilled key high-visibility translations (Rate Limit block, sidebar labels, filter tooltips, run status/time labels) across all shipped locale catalogs.
- [✅] 2026-02-16 — Re-audited localization wiring, fixed a remaining non-localized run-title fallback in `format_run_title`, and translated Chinese sidebar/filter-chip strings (including hover tooltips) that were still in English.
- [✅] 2026-02-14 — Fixed live language switching end-to-end and translated additional UI surfaces (sidebar section headers/meta labels, welcome screen, auth/status dialogs, run/workflow placeholders/tooltips); synchronized new msgids across all locale catalogs.
- [✅] 2026-02-14 — Stabilized localization test isolation by restoring the previous effective language in i18n tests; `cargo test` no longer depends on execution order.
- [✅] 2026-02-14 — Localized `data/metainfo.xml` (AppStream summary/description/captions) for supported locales and renamed screenshot assets to `en-*.png` for shared use across all languages.
- [✅] 2026-02-14 — Localized the AppStream feature list (`description > ul`) in `data/metainfo.xml` for all supported locales.
- [✅] 2026-02-14 — Localized all AppStream release notes (`data/metainfo.xml` `<releases>`) for supported locales.
- [✅] 2026-02-16 — Ran locale-by-locale Welcome screen screenshot audit and fixed missing Welcome translations in `po/{zh_Hans,hi,es,fr,ar,bn,pt_BR,ur}.po`.
- [✅] 2026-02-16 — Fixed one-session language switching regression by rebinding app actions (`app.preferences`/etc.) to the current `MainWindow` after reload and hardening app lookup in the preferences language-change handler.
- [✅] 2026-02-14 — Ran ignored GTK UI tests with `xvfb-run`; all ignored UI tests passed after localization updates.
- [✅] 2026-02-14 — Fixed language-switching bug: app now loads `po/*.po` catalogs at runtime in dev mode, so selected locale applies on-the-fly and after restart.
- [✅] 2026-02-14 — Localized workflow completion notification text/status labels and added Preferences controls for theme + language (system fallback to English, runtime window reload on language switch).
- [✅] 2026-02-14 — Translated previously empty localization entries across current PO catalogs (including top-10 locales) so all non-header msgstr values are populated.
- [✅] 2026-02-14 — Added gettext catalogs for target top-10 locales (`en`, `zh_Hans`, `hi`, `es`, `fr`, `ar`, `bn`, `pt_BR`, `ru`, `ur`) and updated `po/LINGUAS`.
- [✅] 2026-02-14 — Started gettext localization: wired runtime i18n setup, localized key main-window/preferences strings, and added initial de/it/ja translation catalogs plus extraction/compile scripts.
- [✅] 2026-02-13 — Fixed Help UX: removed preselected text and pinned the Close button to the bottom-right corner.
- [✅] 2026-02-13 — Added Help menu/action with embedded in-app help text and aligned shortcuts to best-practice bindings (F1 Help, Ctrl+? Shortcuts).
- [✅] 2026-02-13 — Added keyboard shortcuts window and “Report Issue” menu action linking to https://github.com/makoni/actioneer-gtk/issues.
- [✅] 2026-02-13 — Added standard GNOME app actions/accelerators and About window.
- [✅] 2026-02-09 — Bumped app version to 1.0.5 across configs and AppStream metadata.
- [✅] 2026-02-08 — Added workflow_dispatch input support for manual workflow triggers.
- [✅] 2026-02-06 — Bumped app version to 1.0.4 across configs and AppStream metadata.
- [✅] 2026-02-06 — Added Copilot version bump checklist under .github/instructions/.
- [✅] 2026-02-06 — Added test coverage for job log timestamp parsing (BOM + edge cases).
- [✅] 2026-02-04 — Polished job log formatting (timestamp padding, command prefix normalization, error highlighting).
- [✅] 2026-02-04 — Bumped app version to 1.0.3 across configs and refreshed Flatpak lock sync tooling.
- [✅] 2026-02-04 — Fixed Flatpak CI to pass the correct architecture to flatpak-builder for arm64 runs.
- [✅] 2026-01-24 — Expanded CI demo runs to cover success/failed/running states and moved demo job logs into dedicated files.
- [✅] 2026-01-24 — Refreshed demo job logs to showcase timestamps, groups, workflow commands, ANSI colors, and secret masking.
- [✅] 2026-01-24 — Removed `[command]` background highlight to improve dark theme readability.
- [✅] 2026-01-23 — Added ANSI parsing and tests for job logs, with styled rendering in the log viewer.
- [✅] 2026-01-23 — Aligned .github/copilot-instructions.md with current codebase (Libadwaita 1.x latest docs, UI layout notes, test locations, dependency versions).
- [✅] 2026-01-23 — Updated local Libadwaita docs to track the latest 1.x documentation URLs (1-latest) and refreshed headers.
- [✅] 2026-01-19 — Bumped app version to 1.0.2 in Cargo.toml, snapcraft.yaml, and Flatpak docs.
- [✅] 2026-01-19 — Added AppStream release entry for 1.0.2 using the 1.0.1 notes.
- [✅] 2026-01-19 — Removed org.freedesktop.secrets access from the Flatpak manifest to avoid non-portal service warnings.
- [✅] 2026-01-19 — Updated AppStream summary/description to meet Flathub quality guidelines and mention 1.0.1 highlights.
- [✅] 2026-01-19 — Added the 1.0.1 changelog entries to AppStream metadata for store release notes.
- [✅] 2026-01-19 — Updated the lockfile/Flatpak sync script to compare against the working tree so uncommitted Cargo.lock changes are detected.
- [✅] 2026-01-17 — Fixed clippy warnings in run action handlers by replacing `is_err` + `unwrap_err` with `if let Err(err)`.
- [✅] 2026-01-17 — Wrapped detail pane content in a viewport with scroll-to-focus disabled to stop click-to-row scroll jumps.
- [✅] 2026-01-17 — Disabled single-click activation on workflow/run list views to stop click-to-scroll jumps in the detail pane.
- [✅] 2026-01-17 — Disabled focus-on-click for detail-view action buttons to prevent scroll jumps on click.
- [✅] 2026-01-17 — Attached job logs/jobs windows to the GTK application to prevent immediate teardown after clicks.
- [✅] 2026-01-17 — Updated the lockfile sync script to use the repo default branch (origin/HEAD) and fall back to develop instead of main.
- [✅] 2025-12-10 — Matched the snap runtime app ID to snapd’s prefixed desktop file and added a packaged-icon fallback for notifications so the shell and toasts keep the Actioneer icon when installed as a snap.
- [✅] 2025-12-09 — Added a launch-time `--test-notification` flag and routed portal delivery through the Tokio runtime so snap builds can emit notifications without panicking on startup.
- [✅] 2025-12-06 — Wired workflow refresh contexts (manual + auto) to pass `workflows_last_loaded` so the run-load debounce covers all paths and further reduces duplicate background fetches.
- [✅] 2025-12-05 — Prevented duplicate workflow run fetches by coalescing in-flight loads and restored native-first notification fallback when portal delivery is unavailable.
- [✅] 2025-12-05 — Added a "Public" label to sidebar repo rows (alongside the existing private label) to keep visibility explicit without relying on section grouping.
- [✅] 2025-12-05 — Hardened workflow completion notifications by atomically updating digests and marking notified conclusions before dispatch, with tests to ensure a single notification per run and clippy clean.
- [✅] 2025-12-05 — Removed the redundant sidebar "Workflows enabled/disabled" label on repo rows since sections already group by status, keeping rows cleaner.
- [✅] 2025-12-05 — Stabilized GTK UI tests by initializing GTK/Adwaita once per process via `gtk_test_guard`, guarding ignored tests with the helper, and relaxing a favorite button opacity assertion to avoid runner-specific rounding.
- [✅] 2025-12-05 — Resolved the CI fmt failure by reordering the run loader imports and rerunning `cargo fmt` so the lint job passes again.
- [✅] 2025-12-06 — Swapped the run-status filter chips for compact icon toggles and reused cached run lists so filter changes immediately hide/show runs without waiting on GitHub responses.
- [✅] 2025-12-05 — Fixed the debug “Send test notification” action by dispatching through the running GApplication channel and retaining the portal fallback when no default app is registered.
- [✅] 2025-12-05 — Re-investigated workflow auto-refresh so follow-up polling continues after triggered runs are detected (extra attempts keep the UI updating until the run stabilizes).
- [✅] 2025-12-05 — Continued notification debugging and added a portal fallback when no desktop entry is installed, so the debug test action and workflow completion alerts appear again.
- [✅] 2025-12-05 — Fixed workflow auto-refresh to honor saved preferences (including disabling at 0s) and reschedule timers safely per pane.
- [✅] 2025-12-05 — Restored GNOME desktop notifications by routing dispatches through the app-owned GLib channel (the debug test action now surfaces notifications again).
- [✅] 2025-12-04 — Reworked the `DataCache` to return `Arc<Vec<_>>` snapshots so workflow/run/job cache hits share data without cloning entire `Vec`s.
  - [✅] 2025-12-04 — Updated workflow/run/job refresh pipelines to pass `Arc` handles through GTK/Tokio channels and reuse them when persisting caches.
  - [✅] 2025-12-04 — Added unit tests that assert `Arc::ptr_eq` for workflows/runs/jobs to ensure future refactors keep cache snapshots zero-copy.
- [✅] 2025-12-03 — Persisted the workflow/run cache to disk (JSON snapshots under the app cache dir) so cold starts can reuse offline data in sandboxed builds.
  - [✅] 2025-12-03 — Added async hydrate/save plumbing with TTL-based invalidation and sandbox-safe XDG cache discovery.
  - [✅] 2025-12-03 — Covered persistence with unit tests for cold-start hits, TTL expiry, and corruption fallback, plus auto-warmed the cache at startup.
- [✅] 2025-12-03 — Replaced the manual `gtk::Box` run list rebuild with a `WorkflowRunListModel` + `gtk::ListView` pipeline so filters and refreshes reuse row widgets without flicker.
  - [✅] 2025-12-03 — Added `WorkflowRunListModel` wrapping a `gio::ListStore` with stateful placeholders and retry handling.
  - [✅] 2025-12-03 — Wired the workflow pane to reuse a single `gtk::ListView` factory per workflow and persisted expansion state + job context IDs across diff updates.
  - [✅] 2025-12-03 — Covered the new filtering summary helpers with unit tests to ensure visible/filtered counts stay accurate.
- [✅] Break `src/ui/main_window.rs` (~1.3K LOC) into dedicated modules (app state, repo list pane, async loaders) to unblock further readability improvements. (Sidebar panel + header controls + repo loader helpers + selection/background refresh logic extracted into `ui/main_window/` submodules; next up: remaining async/state helpers.)
  - [✅] 2025-12-03 — Moved demo-mode activation/teardown into `ui/main_window/demo_mode.rs`, reducing `main_window.rs` by ~80 LOC and isolating the mock-data entrypoint.
  - [✅] 2025-12-03 — Extracted repository list search/selection wiring into `ui/main_window/repo_list.rs` with unit tests covering repo resolution logic.
  - [✅] 2025-12-03 — Isolated refresh scheduling/abort handling into `ui/main_window/refresh.rs`, keeping GTK updates and rate-limit plumbing contained.
  - [✅] 2025-12-03 — Promoted `RepoActionsState` + `WorkflowStatusCounts` into `ui/state/` (with tests) so UI modules share typed snapshots without cloning logic inline.
- [✅] 2025-12-03 — Modernized the repo sidebar and workflow detail panes to `gio::ListStore` + `gtk::ListView`, keeping selection/filter state stable and logging rebuild timings for >100-row lists to guard large-org performance.
  - [✅] 2025-12-03 — Migrated the sidebar to a single `gtk::ListView` + `gtk::FilterListModel`, carried favorites/selection metadata on each row, and re-used widgets via reparenting to avoid churn.
  - [✅] 2025-12-03 — Swapped the workflow pane’s `gtk::ListBox` rebuild for a persistent `gio::ListStore`, updated refresh/filter plumbing to iterate the store, and instrumented rebuild durations for large workflow sets.
  - [✅] 2025-12-03 — Added tracing-based benchmarks that log rebuild timings whenever repo counts exceed 100 (or workflows exceed 50) so we can spot regressions when testing large orgs.
- [✅] 2025-12-03 — Rebuilt workflow run lists atop `WorkflowRunListModel` + `gtk::ListView`, preserved expansion/scroll state, and removed the ad-hoc `runs_box` churn.
- [✅] 2025-12-03 — Broke the detail view header, filter chips, favorites controls, and run-list layout into dedicated modules (`run_filters.rs`, `workflow_list.rs`, `favorite_controls.rs`, `content.rs`) so each stays under 300 LOC and gains targeted tests.
- [✅] 2025-12-03 — Split workflow refresh plumbing into `workflow_refresh.rs` and parser helpers, plus added `digest.rs`/`filters.rs` coverage to keep HTTP/UI wiring isolated.
- [✅] 2025-12-03 — Trimmed demo fixtures by extracting `src/demo/{mod,data,state}.rs`, enabling focused unit tests for mock run injection.
- [✅] 2025-12-03 — Added a lockfile/Flatpak sync check to CI and a reusable script for local preflight.
- [✅] 2025-12-03 — Added run-status filter chips with persisted preferences, shared ClampScrollable helpers, accessibility touch-ups, and removed duplicate run cache writes for workflows.
- [✅] 2025-11-30 — Added explicit sandbox secret-portal documentation to the README, Snapcraft, and Flatpak guides so reviewers know how to verify the encrypted token flow.
- [✅] 2025-11-30 — Swapped run digests to a HashMap diff so workflow refreshes ignore row ordering and notifications only process truly changed runs (`src/ui/detail_view/helpers/runs/load.rs`).
- [✅] 2025-11-28 — Implemented portal-first token storage (secret_portal + PortalTokenStore), migrated existing keyring secrets automatically, and documented the new snap portal verification checklist.
- [✅] 2025-11-28 — Ensured the OAuth dialog completion automatically initializes the GitHub client so the welcome screen transitions to the main UI without restarting (auth_window.rs, main_window.rs).
- [✅] 2025-11-13 — Resolved the Snap icon regression by pointing the desktop entry icon at `/snap/actioneer/current/meta/gui/me.spaceinbox.actioneer.svg`; GNOME now shows the icon in the shell and dock.
- [✅] 2025-11-13 — Added an env-gated secret portal detector so we can validate the GNOME 49 portal without shipping it yet; remains off until the snap plug is auto-connected.
- [✅] 2025-11-13 — Confirmed `org.freedesktop.portal.Secret` is live on GNOME 49 by wiring a `secret-test` helper that pipes secrets back from the portal without additional deps.
- [✅] 2025-11-13 — Snap: letting the GNOME extension supply GTK/libadwaita again and relying on the portal-based keyring fix; no additional `password-manager-service` plug required.
- [✅] 2025-11-13 — Researched XDG portal docs for an Activation interface; none exists yet, so we’ll keep the NON_UNIQUE multi-instance approach and leave Snap auto-review satisfied without extra glue.
- [✅] 2025-11-13 — Disabled GApplication D-Bus ownership with `NON_UNIQUE`, kept the new snap icon metadata, and verified `cargo fmt`/`cargo check` so the Snap avoids manual review yet still launches cleanly.
- [✅] 2025-11-06 — Cleaned residual D-Bus activation artifacts across packaging configs after dropping the snap slot.
- [✅] 2025-11-06 — Removed the unused Snap D-Bus slot to unblock store auto-review and reran the packaging tests.
- [✅] 2025-11-06 — Regenerated the Flatpak cargo sources after dropping the direct zbus dependency and reran the Rust test suites.
- [✅] 2025-11-06 — Stopped ignoring `Cargo.lock` so the lockfile can be committed alongside the application code for reproducible builds.
- [✅] 2025-11-06 — Swapped the Flatpak vendoring tarball for a `flatpak-cargo-generator` JSON manifest and pointed the manifest at it so the build stays offline-compliant for Flathub.
- [✅] 2025-11-05 — Finished the Flathub “Before submission” checklist locally: rebuilt the vendored Flatpak, fixed screenshot hosting to use the public develop branch, and cleared the repo linter (only optional caption warnings remain).
- [✅] 2025-11-04 — Replaced the arm64 apt source rewrite with explicit archive/ports lists and pinned the GNOME extension channel to edge so Snap CI stops failing (`.github/workflows/snap-ci.yml`).
- [✅] 2025-11-04 — Reworked the single-runner Snap CI to install arm64 multi-arch GTK/libadwaita toolchains directly on ubuntu-latest and switched cargo builds away from `cross` so Snapcraft can target both amd64/arm64 with `--build-for`. (Arm64 builds remain temporarily disabled in CI due to GitHub Actions runner issues.)
- [✅] 2025-11-07 — Fixed the missing welcome-screen icon by aligning icon lookups with installed assets and adding icon theme search paths for dev/Flatpak builds.
- [✅] 2025-11-04 — Added a `publish` workflow to orchestrate packaging jobs, gather artifacts, and draft GitHub releases from the aggregated outputs.
- [✅] 2025-11-03 — Added a CI vendoring step for Flatpak builds (dynamic `vendor/` + in-sandbox cargo config) and switched the manifest to offline cargo commands to avoid network failures.
- [✅] 2025-11-03 — Replaced manual Flatpak CI setup with the official `flatpak-builder` GitHub Action, enabled temporary network access via the action’s `run-tests` toggle, and exposed the Rust SDK extension (PATH/env tweaks) so cargo-based builds succeed without a prebuilt binary.
- [✅] 2025-11-03 — Enabled D-Bus activation for notification clicks (desktop entry + service file), updated Flatpak permissions/install steps, aligned icon naming with the app id, and trimmed unused Snap ICU libs while keeping legacy assets for Snap.
- [✅] 2025-10-30 — Replaced Docker guidance with the native Snapcraft workflow in `docs/snapcraft_ai_guide.md` to match the updated CI (`snap-publish.yml`).
- [✅] 2025-10-29 — Switched Snapcraft builds to the `ghcr.io/canonical/snapcraft:8_core24` Docker image, updated CI to drop LXD membership hacks, refreshed `docs/snapcraft_ai_guide.md`, and kept the root symlink + `.snapcraftignore` layout intact.
- [✅] 2025-10-28 — Hardened demo mode (hide release toggle, mock rate limits, skip token checks).
- [✅] 2025-10-28 — Re-enabled welcome screen demo mode toggle and ensured switching to real auth exits demo (`src/ui/main_window.rs`, `src/ui/welcome_screen.rs`).
- [✅] 2025-10-28 — Added a `gtk::Viewport` inside the sidebar `adw::ClampScrollable` (`src/ui/main_window.rs`) to eliminate GTK warnings about missing scrollable properties.
- [✅] 2025-10-28 — Added MIT license, refreshed README, and introduced Snapcraft packaging (`LICENSE`, `README.md`, `snap/snapcraft.yaml`).

Recent: This file was compacted to keep only the essentials. For deep dive history, search commits or the previous long-form TODO in the repo history.
