# Actioneer for Linux

<p align="center">
  <img src="https://spaceinbox.me//images/actioneer-cyberpunk.webp">
</p>

Actioneer is a native GNOME desktop client for GitHub Actions. It combines a GTK4/libadwaita interface with a Tokio-powered API client so you can browse repositories, inspect workflow runs, watch job logs, and receive notifications without leaving your desktop.

<a href="https://snapcraft.io/actioneer"><img alt="Get it from the Snap Store" src="https://snapcraft.io/en/dark/install.svg" /></a> <a href="https://flathub.org/en/apps/me.spaceinbox.actioneer"><img height="56" alt="Get it on Flathub" src="https://flathub.org/api/badge?locale=en"/></a>

## Feature Highlights
- GitHub sign-in via OAuth device flow with tokens stored securely (system keyring on classic installs, encrypted via the xdg-desktop-portal Secret interface inside sandboxes)
- Repository browser with live search, favorites, caching, and adaptive layout
- Detailed workflow, run, and job views with real-time log streaming and status indicators
- Background refreshes with rate-limit awareness and optional notifications
- Preference pane for window state, refresh cadence, and favorites
- Works on both Wayland and X11 thanks to libadwaita’s adaptive widgets and GTK4 renderers

## Installation

### Flatpak (local build)
The repository ships with a Flatpak manifest. To build and install a local bundle:

```bash
scripts/render-flatpak-manifest.sh --mode local
flatpak-builder --user --install --force-clean builddir flatpak/me.spaceinbox.actioneer.yaml
flatpak run me.spaceinbox.actioneer
```

The manifest is rendered from `flatpak/me.spaceinbox.actioneer.yaml.in`; the rendered file is not committed.

### Snap (local build)
Build and install an unsigned snap locally:

```bash
snapcraft
sudo snap install --dangerous actioneer_*.snap
```

Once the store listing is published you will be able to install with:

```bash
sudo snap install actioneer
```

### Build From Source

Install the build dependencies first:

- **Ubuntu / Debian**

  ```bash
  sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config
  ```

- **Fedora**

  ```bash
  sudo dnf install gtk4-devel libadwaita-devel pkg-config
  ```

Then build and run in release mode:

```bash
cargo build --release
cargo run --release
```

## Configuration

Actioneer ships with default OAuth credentials for developer testing. To use your own GitHub OAuth application:

1. Copy the example environment file:

   ```bash
   cp .env.example .env
   ```

2. Edit `.env` with your client ID and secret:

   ```bash
   GITHUB_CLIENT_ID=your_client_id
   GITHUB_CLIENT_SECRET=your_client_secret
   ```

3. Run the app with your credentials:

   ```bash
   cargo run
   ```

## Secret storage & sandbox expectations

Actioneer picks a token backend automatically:

- **Classic/cargo builds (non-sandboxed):** tokens live in the desktop keyring (`me.spaceinbox.actioneer` entry via libsecret/keyring). Nothing extra is required beyond an unlocked keyring.
- **Snap (strict confinement):** the app always uses `org.freedesktop.portal.Secret` and stores the encrypted token at `$SNAP_USER_COMMON/.config/actioneer/secret-portal/github_token.portal`. This requires the GNOME portal stack that ships with Ubuntu 20.04+ (GNOME keyring ≥ 3.35.1).
- **Flatpak:** the same portal backend is used, with the ciphertext stored under `~/.var/app/me.spaceinbox.actioneer/config/actioneer/secret-portal/github_token.portal`.

Packaging/review checklist:

1. Launch the sandboxed build with logging enabled (`ACTIONEER_LOG=info snap run actioneer` or `flatpak run me.spaceinbox.actioneer`). The log must contain `Using secret portal storage` before you start the OAuth device flow.
2. Complete sign-in once, restart the app, and confirm the encrypted file mentioned above exists and the session stays authenticated. If the log falls back to `system keyring storage`, the host portal stack is missing or disabled—fix that before submitting to the Snap Store or Flathub.

Set `ACTIONEER_ENABLE_SECRET_PORTAL=1` to force the portal on desktop builds for testing, or `ACTIONEER_DISABLE_SECRET_PORTAL=1` to compare behavior without the portal.

## Desktop Integration (from source builds)

Install the desktop entry and icons so Actioneer shows up in GNOME Shell search:

```bash
mkdir -p ~/.local/share/applications ~/.local/share/icons
cp data/me.spaceinbox.actioneer.desktop ~/.local/share/applications/
cp -r icons/icons/hicolor ~/.local/share/icons/
gtk-update-icon-cache ~/.local/share/icons/hicolor
```

Log out/in or restart GNOME Shell to refresh the cache. For system-wide installs, copy the files to `/usr/share/applications` and `/usr/share/icons/hicolor` instead.

## Development Workflow

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The GTK tests (`#[ignore]`d) and the AT-SPI smoke tests need Xvfb and a session bus; the exact commands are in [`AGENTS.md`](AGENTS.md#validation). Do not run a bare `cargo test -- --ignored`: it includes a test that writes to your real system keyring.

See `docs/` for API, caching, and UI guidelines, and [`docs/TESTING.md`](docs/TESTING.md) for the test layout. The project enforces zero warnings in `cargo build` and `cargo clippy`.

### Localization

Actioneer uses gettext catalogs under `po/`, one per language listed in `po/LINGUAS` (14: `en`, `de`, `nl`, `zh_Hans`, `hi`, `es`, `fr`, `ar`, `bn`, `pt_BR`, `ru`, `ur`, `it`, `ja`).

```bash
scripts/extract-translations.sh   # requires xtr (cargo install xtr) and gettext
scripts/compile-translations.sh   # requires msgfmt (gettext package)
```

## Packaging Notes

- **Flatpak**: The manifest template is `flatpak/me.spaceinbox.actioneer.yaml.in`; render it with `scripts/render-flatpak-manifest.sh` (`--mode local` for local and CI builds, `--mode flathub --commit <sha>` for the Flathub repository). Cargo dependencies are vendored offline through `flatpak/me.spaceinbox.actioneer.cargo-sources.json`: whenever `Cargo.lock` changes (including `cargo update`), run `scripts/regenerate-flatpak-sources.sh` and confirm with `scripts/check-flatpak-lock-sync.sh`. When `rofiles-fuse` is unavailable (for example in virtualised hosts), build with `scripts/flathub-build.sh --install flatpak/me.spaceinbox.actioneer.yaml`. See “Secret storage & sandbox expectations” for the required portal verification steps before shipping a Flatpak build.
- **Snap**: `snapcraft.yaml` builds a strictly confined snap using the GNOME extension. Test locally with `snapcraft pack` or push to the Snap Store once the snap is registered. The snap no longer plugs `password-manager-service`; instead it depends on the xdg-desktop-portal Secret interface documented above, so capture the portal log line and encrypted file path mentioned in “Secret storage & sandbox expectations” when requesting store review.

## Architecture Overview

The crate is a library (`src/lib.rs`) with a thin binary on top (`src/main.rs`), split into layers:

- `src/main.rs` – composition root: sets up the application, icon theme, and display
- `src/kernel/` – app identity, i18n, and shared value types; depends on nothing
- `src/runtime/` – the Tokio runtime handle and the bridge onto the GLib main context
- `src/domain/` – pure rules with no GTK: models, runs, filters, counts, formatting
- `src/services/` – adapters: the GitHub API client (`api/`, with ETag caching), the OAuth device flow (`auth/`), token storage (`tokens/`: keyring on classic installs, xdg-desktop-portal Secret inside sandboxes), the gateway that picks the live or demo backend, cache, favourites, preferences, notifications, and crash reports; `app_services.rs` builds the real services in one place
- `src/demo/` – the demo backend and its bundled sample data
- `src/ui/` – GTK4/libadwaita widgets only
- `tests/` – integration and characterization tests against the library; `tests/smoke/` holds the AT-SPI journeys

The layering rules and the reasoning behind them are in [`AGENTS.md`](AGENTS.md) and [`docs/agent-guide.md`](docs/agent-guide.md).

## License

Actioneer is available under the [MIT License](LICENSE).
