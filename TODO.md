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

Optional / next steps (low priority)
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
