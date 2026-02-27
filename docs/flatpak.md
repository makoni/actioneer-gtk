Flatpak & Flathub submission plan for Actioneer-gtk

Goal

- Produce a reproducible Flatpak build and prepare the repository for submission to Flathub following the Flathub "for app authors" guidance (metainfo, requirements, quality guidelines, submission process).

Prerequisites

- Install Flatpak and flatpak-builder locally.
- Have a Flathub account (or ability to create one) for submission.
- Ensure repository contains: application desktop file (`data/me.spaceinbox.actioneer.desktop` exists), icons in `icons/`, and AppStream metadata (to be added).
- Have CI (GitHub Actions) available to run automated Flatpak builds and checks.

High-level plan (steps)

1. Choose an application ID and manifest format
   - Use the existing desktop id as the Flatpak application id: `me.spaceinbox.actioneer`.
   - Create a Flatpak build manifest in YAML or JSON. We recommend YAML, named `flatpak/me.spaceinbox.actioneer.yaml`.
  - Select an SDK/runtime. Prefer a recent, non-EOL runtime. For GNOME/libadwaita apps the recommended approach is to target the latest supported `org.gnome.Platform` branch (for example `49` at time of writing) and the matching `org.gnome.Sdk` SDK. Pin the `runtime-version` in the manifest (for example `runtime-version: "49"`) and update it periodically when a new GNOME branch is released.

2. Create a Flatpak manifest
   - The manifest should declare:
     - id: `me.spaceinbox.actioneer`
     - runtime and runtime-version
     - sdk
     - build-commands (how to run cargo build in the sandbox)
     - modules and sources (use "git" or "archive" sources; for Flathub, a GitHub URL is typical)
     - finish-args (sandbox permissions required at runtime; keep minimal and request additional permissions via portals where possible)
   - Keep the build reproducible by pinning versions for the SDK/runtime and any external modules.

   Minimal YAML manifest example (skeleton):

   ```yaml
   app-id: me.spaceinbox.actioneer
   # For a GNOME/libadwaita app target the GNOME runtime and matching SDK
   runtime: org.gnome.Platform
   runtime-version: "49"
   sdk: org.gnome.Sdk
   command: me.spaceinbox.actioneer
   modules:
     - name: actioneer
       build-system: simple
       build-commands:
         - cargo build --release
       sources:
         - type: git
           url: https://github.com/makoni/Actioneer-gtk
          tag: v1.0.6 # set to the release tag or branch for builds
   ```

  - For Rust projects it's common to use a small build helper that installs Rust toolchain inside the SDK or use the `org.freedesktop.Sdk.Extension.rust` extension if available; check the SDK docs for the current recommended approach. When targeting the GNOME SDK, keep the `org.freedesktop.Sdk.Extension.rust` extension listed in `sdk-extensions` to ensure cargo/rustup are available inside the build environment.

3. Add AppStream metadata (metainfo)
   - Flathub requires AppStream `metainfo.xml` describing the app: name, id, summary, description, developer, project-url, license, categories, release information, screenshots, and content rating.
   - Place `metainfo.xml` at `data/metainfo.xml` or under `data/` and reference it in the manifest.
   - Ensure icons are in PNG and SVG at appropriate sizes and referenced in the metadata.
   - Follow the Flathub metainfo and quality guidelines (screenshots, translations, description length).

4. Quality and sandboxing adjustments
   - Minimize finish-args (unsafe permissions). Prefer portals for:
     - File access (xdg-desktop-portal)
     - Notifications
     - Network is allowed by default in Flatpak, but be explicit if you need to allow network in a restrictive setup.
     - Secret/keyring access: Flatpak sandbox blocks direct access to the system keyring. Replace or adapt `src/storage/token_storage.rs` to use a portal-friendly solution or guard keyring tests so they don't run inside the sandbox. Consider using the `xdg-desktop-portal` secrets API or `libsecret` behind a portal-compatible interface.
   - Ensure the app uses the portal APIs where possible (notifications via libnotify and portals, file chooser via portals, etc.).
   - Make sure the app does not call into system resources without explicit user consent.

Recommended runtime management

- Always target a runtime branch that is not end-of-life (EOL). Flathub will reject submissions that use EOL runtimes. Check available runtime branches with:

```bash
flatpak remote-info flathub org.gnome.Platform
flatpak remote-info flathub org.gnome.Sdk
```

- Use the newest supported GNOME runtime (for example `49` at the time this document was updated) for GNOME/libadwaita apps. Pin the `runtime-version` in your manifest to a specific branch (do not use an unpinned/latest string). When a new GNOME runtime is released, update the manifest, run a local build and re-run AppStream validation.


5. CI: automated flatpak builds & checks
   - Add a GitHub Actions workflow that:
     - Installs flatpak and flatpak-builder in the runner or uses a container with the SDK preinstalled.
     - Runs `flatpak-builder --repo=repo build-dir flatpak/me.spaceinbox.actioneer.yaml` and optionally `flatpak build-bundle` to create bundles.
     - Verifies metainfo: `appstream-builder validate-metainfo` or similar tooling to check AppStream metadata.
     - Optionally runs `flatpak run --command=me.spaceinbox.actioneer --filesystem=host build-dir` (or equivalent) in an integration environment.
   - Ensure the workflow publishes build artifacts (bundle or repo) as needed for manual testing.

6. Local testing
  - Add Flathub remote if not present:

     ```bash
     flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
     ```

   - Build locally with `flatpak-builder`:

     ```bash
     flatpak-builder --force-clean --install --user build-dir flatpak/me.spaceinbox.actioneer.yaml
     ```

     On systems where `rofiles-fuse` is unavailable (common in virtualised hosts), use the helper script which forwards all arguments to `flathub-build` while adding `--disable-rofiles-fuse`:

     ```bash
     scripts/flathub-build.sh --install flatpak/me.spaceinbox.actioneer.yaml
     ```

  - Run the installed app:

     ```bash
     flatpak run me.spaceinbox.actioneer
     ```

   - Debug logs: run with `flatpak run --command=sh me.spaceinbox.actioneer` and inspect files, or run `journalctl --user -f` for portal and notification issues.

7. Prepare for Flathub submission
   - Ensure the repository has a stable release (tag and source accessible publicly) or prepare a bundled source archive.
   - Confirm metainfo (AppStream), icons, descriptions, and screenshots meet the Flathub guidelines.
   - Follow the Flathub submission flow: create app record on Flathub or open a PR in the Flathub apps repo (the Flathub docs linked by the maintainer explain the exact submission steps).

8. Post-submission
   - Address any feedback from Flathub reviewers (they may request metadata changes, screenshots, or runtime permission changes).
   - Add a CI job that follows the Flathub build environment more closely so future PRs are pre-validated.

AI agent instructions (what the agent should do)

Contract (inputs / outputs / success criteria)

- Inputs:
  - Repository root (this workspace).
  - Access to GitHub (optional, only if you will create tags or PRs).
- Outputs:
  - `flatpak/me.spaceinbox.actioneer.yaml` manifest checked into the repo.
  - `data/metainfo.xml` AppStream metadata.
  - A new docs page `docs/flatpak.md` (this file).
  - Optional: GitHub Actions workflow to run flatpak-builder and validate metainfo.
- Success criteria:
  - `flatpak-builder` can build the app locally (see local testing commands above).
  - App starts when run via `flatpak run me.spaceinbox.actioneer` (basic startup, no crashes).
  - Metainfo validates against AppStream tooling and includes icons/screenshots.

Steps for the agent (detailed)

1. Audit the repo for required assets
   - Confirm `data/me.spaceinbox.actioneer.desktop` exists (it does). Confirm icons exist in `icons/` and are valid.
   - Identify any runtime calls that require special sandbox permissions (keyring, host file access, dbus services).

2. Create a `flatpak/` manifest
   - Create `flatpak/me.spaceinbox.actioneer.yaml` with a pinned SDK/runtime and sources pointing to a stable tag or branch.
   - Use `build-commands` that run `cargo build --release`. If the SDK requires installing Rust, include commands to install rustup + rust toolchain or add the rust extension for the sdk.
   - Keep `flatpak/me.spaceinbox.actioneer.cargo-sources.json` in lockstep with `Cargo.lock`. Run `flatpak-cargo-generator -d Cargo.lock -o flatpak/me.spaceinbox.actioneer.cargo-sources.json` after every dependency change so `cargo --offline fetch` inside the sandbox can resolve the pinned crates.

3. Add `data/metainfo.xml`
   - Add AppStream metadata including name, summary, description, license, project URL, and screenshots. Use existing `README.md` and `docs/` to populate descriptions.
   - Add translations if available.

4. Verify portal-aware storage & sandbox behavior
   - Actioneer now ships with a `PortalTokenStore` that prefers `org.freedesktop.portal.Secret` inside sandboxes. When testing Flatpak builds, confirm the logs show `Using secret portal storage` and that sign-in tokens persist across relaunches.
   - Keep the legacy keyring path available for classic installs, but ensure unit tests that touch the system keyring are skipped or mocked when `IN_FLATPAK=1`/portal env vars are detected.
   - Review any direct file access and make sure portals or sandbox-friendly locations (XDG config/cache) are used.
   - For Flathub submissions collect evidence similar to Snap builds: run `ACTIONEER_LOG=info flatpak run me.spaceinbox.actioneer`, capture the `Using secret portal storage` log line, and note the ciphertext location `~/.var/app/me.spaceinbox.actioneer/config/actioneer/secret-portal/github_token.portal` after completing OAuth.

5. Add CI workflow (optional but recommended)
   - Add `.github/workflows/flatpak.yml` that installs flatpak and flatpak-builder on the runner and runs the build and metainfo checks.

6. Test locally
   - Run the `flatpak-builder` commands from above, iterate on finish-args until the app runs and only requests necessary permissions.

7. Prepare release and submit
   - Tag a release, ensure the source URL matches the manifest, and follow the Flathub submission docs to create the app record.

Edge cases and checks

- Keyring & secrets: tests that modify the system keyring must be disabled or mocked in Flatpak builds. Do not run destructive tests on developer machines.
- Binary size: flatpak bundles can be large; use a CI job to build and optionally produce a bundle rather than pushing large files to GitHub releases.
- Platform/runtime mismatches: pin the runtime and test on the target runtime version used by Flathub.
- Portals availability: some environments may not have portals installed; provide graceful fallbacks.

Quality gates for PRs

- `cargo test` passes on native environment for unit tests (mock out or skip integration tests that touch system resources).
- `cargo clippy -- -D warnings` passes.
- `cargo fmt` applied.
- `flatpak-builder` builds locally (or in CI) without missing dependencies.
- AppStream metainfo validates (no missing icons or required fields).

Helpful references

- Flathub submission docs (already provided by repository owner):
  - https://docs.flathub.org/docs/for-app-authors/submission
  - https://docs.flathub.org/docs/for-app-authors/requirements
  - https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines
  - https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/quality-guidelines

Next steps for you or the agent

- If you want, I can:
  - scaffold `flatpak/me.spaceinbox.actioneer.yaml` with a conservative SDK/runtime pin and basic build-commands.
  - create a `data/metainfo.xml` draft populated from `README.md` and the repo metadata.
  - add a CI workflow for automated flatpak-builder validation.

If you'd like me to proceed with any of the above, tell me which items to create and I'll implement them and run the local checks.
