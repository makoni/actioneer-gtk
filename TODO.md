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
- Persist cache to disk (optional)
- [✅] Small UI micro-optimizations or accessibility checks
- [✅] Add a CI/preflight check that ensures `Cargo.lock` and `flatpak/me.spaceinbox.actioneer.cargo-sources.json` stay in sync (fail fast when either changes without the other).
- [✅] Factor shared `ClampScrollable`/layout helpers for detail panes so future list sections inherit the correct sizing behavior automatically.
- [✅] Documented sandbox keyring/portal expectations (Snap + Flatpak) in the README and packaging guides so reviewers know how the secret portal is verified.
- Refactor `src/ui/detail_view/helpers/runs/load.rs::load_workflow_runs` (currently ~325 lines) into smaller helpers/structs so error handling, digest comparison, notification dispatch, and UI updates are testable in isolation; the monolithic function makes it hard to reason about background vs foreground refresh paths.
- Replace the manual `gtk::Box` run list rebuild (clearing and re-adding rows on every refresh) with a `gio::ListStore` + `gtk::ListView` factory. That would eliminate repeated widget construction, cut down diff churn, and prevent scroll jumps when only a single run changes.
- [✅] Drop the duplicate cache writes when runs are fetched. Today we call `store_runs_async` when digest changes and then call `cache.store_runs` again right after the HTTP fetch completes (same data). Consolidate into a single write so we avoid extra spawn + clone work per refresh.
- [✅] Add lightweight run-status filter chips (Success/Failed/Running/Branch) above the detail pane so users can quickly scope the list without scrolling. Persist the selection in Preferences so the view restores across sessions.
- Introduce an inline job-log drawer that can be expanded from each job row instead of opening a separate window; mirror the logs view styling with syntax-colored sections and search highlighting for faster triage.
- Provide a compact “Overview” page that aggregates the last run status for pinned repositories (favorites) using multi-pane cards, giving users a bird’s-eye view before diving into a specific repo.
- Modernize the repo sidebar and detail lists to use `gio::ListStore` + `gtk::ListView` (`gtk::SelectionModel`) instead of `gtk::ListBox` rebuilds. This enables row recycling, reduces widget churn on large orgs (100+ repos), and unlocks smooth kinetic scrolling.
- Rework `DataCache` to store `Arc<[WorkflowRun]>` / `Arc<[Workflow]>` snapshots or `Arc<Vec<T>>` so cache hits hand out cheap references instead of cloning the entire vec on every read/write. This should shrink allocations during rapid refresh loops.

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
- [✅] 2025-12-03 — Added a lockfile/Flatpak sync check to CI and a reusable script for local preflight.
- [✅] 2025-12-03 — Added run-status filter chips with persisted preferences, shared ClampScrollable helpers, accessibility touch-ups, and removed duplicate run cache writes for workflows.
- [✅] 2025-11-30 — Added explicit sandbox secret-portal documentation to the README, Snapcraft, and Flatpak guides so reviewers know how to verify the encrypted token flow.
- [✅] 2025-11-30 — Swapped run digests to a HashMap diff so workflow refreshes ignore row ordering and notifications only process truly changed runs (`src/ui/detail_view/helpers/runs/load.rs`).
- [✅] 2025-11-28 — Implemented portal-first token storage (secret_portal + PortalTokenStore), migrated existing keyring secrets automatically, and documented the new snap portal verification checklist.
- [✅] 2025-11-28 — Ensured the OAuth dialog completion automatically initializes the GitHub client so the welcome screen transitions to the main UI without restarting (auth_window.rs, main_window.rs).
- [✅] 2025-11-13 — Resolved the Snap icon regression by pointing the desktop entry icon at `/snap/actioneer/current/meta/gui/me.spaceinbox.actioneer.svg`; GNOME now shows the icon in the shell and dock.
- [✅] 2025-11-13 — Added an env-gated secret portal detector so we can validate the GNOME 49 portal without shipping it yet; remains off until the snap plug is auto-connected.
- [✅] 2025-11-13 — Confirmed `org.freedesktop.portal.Secret` is live on GNOME 49 by wiring a `secret-test` helper that pipes secrets back from the portal without additional deps.
- [🔄] 2025-11-13 — Snap: letting the GNOME extension supply GTK/libadwaita again and adding the password-manager-service plug so keyring access works under confinement.
- [✅] 2025-11-13 — Researched XDG portal docs for an Activation interface; none exists yet, so we’ll keep the NON_UNIQUE multi-instance approach and leave Snap auto-review satisfied without extra glue.
- [✅] 2025-11-13 — Disabled GApplication D-Bus ownership with `NON_UNIQUE`, kept the new snap icon metadata, and verified `cargo fmt`/`cargo check` so the Snap avoids manual review yet still launches cleanly.
- [✅] 2025-11-06 — Cleaned residual D-Bus activation artifacts across packaging configs after dropping the snap slot.
- [✅] 2025-11-06 — Removed the unused Snap D-Bus slot to unblock store auto-review and reran the packaging tests.
- [✅] 2025-11-06 — Regenerated the Flatpak cargo sources after dropping the direct zbus dependency and reran the Rust test suites.
- [✅] 2025-11-06 — Stopped ignoring `Cargo.lock` so the lockfile can be committed alongside the application code for reproducible builds.
- [✅] 2025-11-06 — Swapped the Flatpak vendoring tarball for a `flatpak-cargo-generator` JSON manifest and pointed the manifest at it so the build stays offline-compliant for Flathub.
- [✅] 2025-11-05 — Finished the Flathub “Before submission” checklist locally: rebuilt the vendored Flatpak, fixed screenshot hosting to use the public develop branch, and cleared the repo linter (only optional caption warnings remain).
- [✅] 2025-11-04 — Replaced the arm64 apt source rewrite with explicit archive/ports lists and pinned the GNOME extension channel to edge so Snap CI stops failing (`.github/workflows/snap-ci.yml`).
- [🔄] 2025-11-04 — Reworked the single-runner Snap CI to install arm64 multi-arch GTK/libadwaita toolchains directly on ubuntu-latest and switched cargo builds away from `cross` so Snapcraft can target both amd64/arm64 with `--build-for`.
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