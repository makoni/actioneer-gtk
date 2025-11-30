# Snapcraft Build Guide for Actioneer AI Agents

This note distills the Snapcraft documentation fetched during the investigation into a checklist tailored for this repository. Follow these steps before touching the workflow or `snapcraft.yaml`.

## Environment setup
- Install `snapd` and Snapcraft locally (`sudo apt-get install -y snapd`, then `sudo systemctl enable --now snapd` and `sudo snap install snapcraft --classic`). This mirrors the CI runner configuration.
- Run all commands from the repository root. A symlink (`snapcraft.yaml → snap/snapcraft.yaml`) exposes the manifest at the root so Snapcraft sees the project layout correctly.
- Keep Rust available via `rustup` if you want to rebuild outside Snapcraft. The Snapcraft `rust` plugin still installs its own toolchain during the build step.

## Project layout expectations
- `snapcraft.yaml` lives in `snap/` and is consumed through the root-level symlink. Do **not** duplicate the file.
- `snap/.snapcraftignore` was moved to the root (`.snapcraftignore`) so the entire workspace is filtered before upload.
- Assets referenced in `override-build` live at `data/me.spaceinbox.actioneer.desktop` and `icons/icons/hicolor/scalable/apps/actioneer.svg`. Paths are relative to repository root because `source: .` is used.

## Snapcraft.yaml checkpoints
1. **Metadata** — confirm `name`, `version`, `summary`, `description`, `grade`, `confinement`, `license`, `contact`, and URLs stay accurate (see docs on "Configure package information").
2. **Base** — `core24` matches the GNOME 46 extension. Changing it requires revisiting build dependencies.
3. **Architectures** — Snapcraft 8 replaces `architectures` with `platforms`. `platforms:` already declares `amd64` and `arm64`; keep both so the matrix build succeeds.
4. **Part configuration** — the `rust` plugin plus `source: .` assumes the workspace has `Cargo.toml` at the root. Do not move the manifest.
5. **Override build hook** — `craftctl default` runs the stock Rust build (`cargo install ...`). Extra installs copy desktop and icon assets into `meta/gui`.
6. **Stage packages** — libadwaita/libgtk/libssl ship runtime GTK stack. Use `stage-packages` for runtime libraries, `build-packages` for headers + pkgconfig.
7. **Slots/plugs** — the snap no longer exposes a custom D-Bus name, and Actioneer now relies on `org.freedesktop.portal.Secret` for secret storage instead of the `password-manager-service` plug. Keep the plug list minimal and verify the portal is present on GNOME 46/base core24 images before publishing.

## Local build flow (native snapcraft)
1. `snapcraft clean actioneer --destructive-mode` to wipe `parts/`, `prime/`, and related directories when dependencies or layout change.
2. `snapcraft pack --destructive-mode --output snap-output/<arch>/actioneer_<arch>.snap` builds a release snap using the host architecture (arm64 on arm runners, amd64 on x86).
3. Outputs land in `snap-output/<arch>/`. Remove them after tests to keep the tree clean. Files are owned by your user when running locally.
4. Review lint diagnostics printed after `pack`. They are currently warnings (unused GUI libs, missing `donation` link); track regressions or new errors.

## CI workflow expectations
- Workflow runs from the repo root so the symlinked manifest is detected automatically. Each matrix job builds natively on its architecture using `snapcraft pack --destructive-mode`.
- Use separate output directories per architecture (`snap-output/<arch>/`) before uploading artifacts.
- Keep the cache step for Cargo but remember Snapcraft performs its own Rust build; the cache mainly speeds up the explicit `cargo build` step.
- Snapcraft is installed directly on the runner via `snapd`, allowing `snapcraft login`, `snapcraft whoami`, and `snapcraft upload` to execute without containers.
- Ensure the Snapcraft store credentials are passed via `SNAPCRAFT_STORE_CREDENTIALS`; `snapcraft whoami` is the quick validation.
- After `pack`, call `snapcraft upload` (alias `snapcraft push`) with `--release edge` as currently configured.

## Secret portal verification
- Launch the snap (`snap run actioneer`) on a confined session (core24 GNOME image). `ACTIONEER_LOG=info` will show whether the portal backend is selected (`Using secret portal storage...`).
- If the log falls back to "system keyring storage", inspect the host (`busctl --user list | grep portal`, `gdbus introspect --session --dest org.freedesktop.portal.Desktop ...`) and ensure `xdg-desktop-portal` plus the GNOME backend are present.
- The snap no longer declares `password-manager-service`, so portal failures will break token storage — run this check before requesting store review.

**Store review note:** include in the Snap Store request both (1) the log line showing `Using secret portal storage` and (2) the path of the encrypted token file (`$SNAP_USER_COMMON/.config/actioneer/secret-portal/github_token.portal`) after completing OAuth. Reviewers have explicitly asked for this confirmation since the portal is guaranteed in Ubuntu 20.04+.

## Troubleshooting checklist
- **Missing `prime/meta/snap.yaml`** — means the `pack` command did not consume the `prime` dir; verify `snapcraft pack` completed and inspect `prime/meta/` contents.
- **`Cargo.toml` not found** — happens when Snapcraft cannot see the repo root. Always invoke commands from the top-level directory so the root symlink remains in place.
- **Filename too long / recursive copy** — avoid symlinking the project back into `snap/`; rely on the symlinked manifest instead.
- **GTK runtime issues** — ensure `stage-packages` matches the GNOME platform (libadwaita-1-0, libgtk-4-1) and that the GNOME extension remains enabled.
- **Snapcraft cache issues** — run `snapcraft clean actioneer --destructive-mode` if builds behave inconsistently after dependency changes.

Cross-reference:
- Ubuntu Snapcraft docs > *Configure package information*, *Select a base*, *Select platforms*, *Manage build dependencies*, *Add configuration options*, *Override default build process*.
- Ubuntu Snapcraft docs > *Customize the lifecycle* for notes on `craftctl` hook usage.

Keep this guide updated whenever the snap layout or workflow changes.
