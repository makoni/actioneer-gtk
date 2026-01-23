<!-- Actioneer-gtk: Copilot / AI agent instructions -->
# Quick guide for automated coding agents

This repository is a native GTK4/libadwaita desktop client for GitHub Actions written in Rust. The notes below focus on the patterns and files an AI coding agent should know to make safe, useful changes quickly.

Important local docs
- The repository contains a curated, local copy of Libadwaita 1.x (latest stable) reference documentation under `docs/libadwaita/`. Automated agents MUST consult `docs/libadwaita/` for widget and helper guidance when making UI changes. These local docs are the canonical reference for UI implementation in this repo and are preferred over remote fetches to avoid network variability and version skew.
- General project documentation is in `docs/`; use `docs/` first for any implementation or styling questions before consulting upstream web pages.

- Repo entry & runtime
  - App entry: `src/main.rs`. A Tokio runtime is spawned on a background thread and a global handle is stored via `OnceLock`. Use `crate::runtime_handle()` to run async HTTP work on the background runtime.
  - UI run-loop: GTK / libadwaita run on the GLib main loop. Never touch GTK widgets from Tokio threads.

- Key modules (big picture)
  - `src/ui/` — all UI components and glue. `src/ui/main_window.rs` plus `src/ui/main_window/` submodules show most interaction patterns (loading repos, refreshing UI, connecting buttons).
  - `src/api/` — GitHub client, endpoints and models. See `src/api/client.rs` and `src/api/models.rs` for API surface.
  - `src/auth/` — OAuth Device Flow implementation (`device` module). Changes to auth must ensure TokenStorage interaction remains compatible.
  - `src/storage/token_storage.rs` — secure token handling via system keyring. This file contains a live test of the keyring; edits can affect developer machines.
  - `src/cache.rs`, `src/preferences.rs`, `src/favorites.rs` — app-level caching and persistence helpers used throughout the UI.

- Concurrency & integration patterns (important)
  - Background HTTP and long-running work: run on Tokio via `crate::runtime_handle().spawn(async move { ... })` (see `load_repositories`, `spawn_repo_status_tasks`).
  - UI updates: schedule UI changes on GLib using `glib::MainContext::default().spawn_local(...)` or `glib::idle_add_local_once(...)`. Follow the code in `src/ui/main_window.rs` as the canonical pattern.
  - Shared state: use `Arc<Mutex<T>>` (parking_lot::Mutex) for data shared between UI and background tasks. UI-specific ownership often uses `Rc<RefCell<...>>` for widgets/panes.

## Tokio + GTK runtime best practices (specific)

This project mixes a Tokio runtime for async HTTP work with GTK's GLib main loop. Follow these rules to avoid subtle deadlocks, panics, and non-Send/`'static` issues:

- Create a single, long-lived Tokio runtime (multi-threaded) and expose its `Handle` globally (the repo already uses `OnceLock<Handle>` in `src/main.rs`). Avoid creating multiple runtimes.
  - Example startup: `Builder::new_multi_thread().enable_all().worker_threads(num_cpus::get()).build()?` and store the handle.

- Never call `Runtime::block_on` or otherwise block the GLib main thread. Blocking the main thread freezes the UI and can deadlock Tokio resource drivers. If you need to run synchronous work, use `spawn_blocking` on Tokio or schedule it on GLib's thread pool.

- Run network and heavy async work on Tokio: from the GTK/main thread, dispatch work with the runtime handle and do NOT touch GTK objects inside those tasks.
  - Pattern:

```rust
let handle = crate::runtime_handle().clone();
handle.spawn(async move {
    let repos = client.list_repos().await; // HTTP on tokio
    // Marshal results back to the GLib main loop for UI updates
    glib::MainContext::default().spawn_local(async move {
        // safe to touch GTK widgets here
    });
});
```

- Avoid awaiting Tokio JoinHandles inside GLib async contexts. Instead either:
  - Await inside Tokio tasks and then call `spawn_local` to update UI; or
  - Use channels (oneshot/mpsc) to send results back and handle them on the GLib side.

- `tokio::spawn` requires futures to be `Send + 'static`. Any non-Send state (e.g., `Rc` or GTK objects) must not be moved into Tokio tasks. Use `Arc` for shared state or keep GTK references only on the GLib side.

- For non-Send async computations that must run on the main loop, use `glib::MainContext::default().spawn_local(...)` or `glib::source::spawn_local` — these run on GLib's executor and can safely use non-Send GTK types.

- For blocking CPU work, use `tokio::task::spawn_blocking` so the async runtime's IO threads aren't blocked.

- If you ever need to run tokio-owned code on the current thread (rare), use `Handle::enter()` carefully and only from non-GLib threads; avoid entering a Tokio runtime on the GLib main thread.

- Common pitfalls to watch for:
  - Holding GTK objects or `Rc<...>` across `.await` in a Tokio-spawned task (will fail `Send`/lifetime checks).
  - Calling `Runtime::block_on` on the main thread.
  - Creating short-lived runtimes repeatedly (costly and may leak OS threads if misused).

These specifics are drawn from Tokio and gtk-rs patterns — follow them when adding async logic or refactoring existing code.

- snapcraft.yaml API reference
  - Use docs: https://documentation.ubuntu.com/snapcraft/stable/reference/project-file/snapcraft-yaml/

- Snap icon/app-id mapping (2025-12)
  - snapd rewrites desktop files to `<snap_name>_<desktop-id>.desktop`; `main.rs` resolves the runtime app id to that prefixed form so the shell/portal can match the running window. Do not revert to the bare `APP_ID` for snaps.
  - Icons are bundled under `meta/gui/` and `usr/share/icons/` in the snap; `main.rs` adds those search paths at startup. Notifications also fall back to a file icon at `$SNAP/meta/gui/me.spaceinbox.actioneer.svg` to avoid missing icons in toasts. Keep the icon name `APP_ICON_NAME` and ensure any packaging changes continue to install the SVG in `meta/gui`.

- Authentication & token handling
  - Token lifecycle lives in `TokenStorage`. `TokenStorage::new()` performs a keyring test and may return `KeyringUnavailable`. Handle that explicitly — the UI currently falls back to showing the auth window.
  - Sandboxed builds (Flatpak, Snap) now default to `PortalTokenStore`, which talks to `org.freedesktop.portal.Secret`. Keep this path intact and avoid reintroducing the `password-manager-service` snap plug; fix portal detection if you see "Using system keyring storage" inside a sandbox.
  - The OAuth device flow UI is in `src/ui/auth_window.rs` and the flow implementation in `src/auth/device.rs`.

- How to add API calls safely
  - Add or change endpoints under `src/api/` and update `client.rs` for higher-level helpers.
  - In UI code, clone the client and run HTTP calls on the Tokio runtime. After awaiting the result, marshal results back to the GLib main thread before touching GTK widgets. Example pattern used in repo:

```rust
// run HTTP on tokio
let repos_result = crate::runtime_handle().spawn(async move { client.list_repos().await }).await.unwrap();
// then update UI on glib/main thread
glib::MainContext::default().spawn_local(async move { /* refresh widgets */ });
```

- Build, test, and quality gates (must-do in PRs)
  - System deps (Ubuntu/Debian): `sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config` (see `README.md`).
  - Build: `cargo build`; Run: `cargo run` (reads `.env` when provided).
  - Tests: `cargo test` (there are unit tests such as token storage lifecycle).
  - UI tests: UI widget tests live under `src/ui/**` and use `ui::test_helpers::gtk_test_guard`. Run `cargo test -- --ignored` to execute GTK-dependent tests when a display is available.
  - Formatting & linting: `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`. The project aims for zero warnings; a PR should not introduce warnings.
  - After finishing code edits, run the same checks as `.github/workflows/ci.yml` (only fmt, clippy, build, and ignored UI tests when feasible) to ensure the project is buildable and clippy is clean.

  - Rust toolchain: prefer using `rustup` and pinning a toolchain for reproducible development (for example by adding a `rust-toolchain.toml` file in the repo). If a pinned toolchain is not available, use the latest `stable` channel. After switching toolchains run `cargo clean` then `cargo build` to ensure dependencies are rebuilt for the active toolchain.

  - Headless UI tests (one-liner): when running the ignored UI tests on a headless Linux machine, use `xvfb-run` to provide a virtual X server. Example:

    ```bash
    xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored
    ```

    In CI prefer to either run tests in a container/image that includes an X server or use the above `xvfb-run` wrapper.

  - Token/keyring safety: `src/storage/token_storage.rs` contains a live keyring test and some operations that may write to or delete entries in the system keyring. Do NOT run or modify those destructive tests on developer machines unless you understand and accept the side-effects. Prefer using mocks or a dedicated test keyring account when adding or changing tests that interact with the system keyring.

  - PR checklist additions: when creating a PR, in addition to the validation checklist above, ensure you have updated `TODO.md` per the repository's `AGENTS.md` rules (mark started items as [🔄] and completed items as [✅], add a brief note in "Recent Updates"). This repo expects `TODO.md` to be kept current by contributors and automated agents.

- Project-specific conventions
  - Prefer `parking_lot::Mutex` for shared state; code frequently clones `Arc<Mutex<T>>` before spawning tasks.
  - UI changes must use `glib::idle_add_local_once` or `spawn_local` to ensure GTK safety.
  - Token/keyring interactions are tested at runtime in `token_storage.rs` — avoid destructive cleanup in tests that run on developer machines.
  - New notes (2025-10): Recent changes added ETag caching for GET endpoints in `src/api/http.rs`. Agents should use the `ResponseHandler` for conditional requests by calling `apply_cache_headers` when building requests and passing the same cache key to `handle_response`. See `src/api/*` modules for examples.
  - The sidebar width issue was fixed by wrapping the sidebar content in `create_sidebar_clamp` (`adw::Clamp`) inside `src/ui/main_window/sidebar_panel.rs` and configuring the `gtk::Paned` in `src/ui/main_window.rs` to keep the start child at its natural size. If you change the sidebar layout, keep the clamp + viewport constraints in mind.
  - The detail pane uses a `gtk::ScrolledWindow` + `gtk::Viewport` (with `scroll_to_focus` disabled) in `src/ui/detail_view/mod.rs`, and the run list is wrapped via `create_detail_clamp` in `src/ui/detail_view/content.rs`. Preserve that structure when touching detail panes to avoid clipped content or scroll jumping.
  - Background refreshes skip redundant non-ETag endpoints: `spawn_repo_status_tasks` now tracks last-checked timestamps and avoids querying the actions-permissions endpoint more often than a TTL. If you need to force-refresh, clear the timestamps in `actions_checked_at`.
  - Flatpak packaging pins Cargo dependencies via `flatpak/me.spaceinbox.actioneer.cargo-sources.json`, generated with `flatpak-cargo-generator`. When you change Rust dependencies (including `cargo update`), regenerate this file with `~/.local/bin/flatpak-cargo-generator -d Cargo.lock -o flatpak/me.spaceinbox.actioneer.cargo-sources.json` and commit the result. Verify the manifest by running `flatpak-builder --force-clean --ccache builddir flatpak/me.spaceinbox.actioneer.yaml` so Flathub keeps an offline-complete build. Clean up any temporary `vendor/` directory after regenerating the manifest to avoid accidentally committing it.
  - Notifications (new, 2025-12):
    - Use the existing dispatcher in `src/notifications.rs`. Keep GTK work on GLib and network/background work on Tokio. Set a default action (`app.focus-main-window`) so shell clicks focus the window. Do not touch GTK objects from Tokio tasks.
    - Preserve portal/native routing: prefer portal when sandboxed and not on Snap, and when explicitly forced. Snap defaults to native unless `ACTIONEER_FORCE_PORTAL_NOTIFICATIONS` is set; do not create new runtimes or bypass the dispatcher channel.
    - Keep payloads concise (title + short body) and include the themed icon name (`APP_ICON_NAME`). Respect the user’s notification preference flag.
    - Desktop entry is required for notifications. Before testing or relying on notifications, ensure `data/me.spaceinbox.actioneer.desktop` is installed to `~/.local/share/applications/` (or the relevant XDG data dir). Agents should check for an installed `me.spaceinbox.actioneer.desktop` and install/update it if missing/outdated (copy from `data/`). Do not add runtime installation in code paths.
    - Leave the application-level `focus-main-window` action intact (`src/ui/main_window.rs`). If adding new notification actions, wire them to `app.*` actions.
  - Run filters (2025-12 fix): run-status chip toggles reuse cached runs instead of reloading. The filter path must recursively visit workflow expanders because they are nested inside boxes; see `visit_expanders` in `src/ui/detail_view/run_filters.rs` and the matching helper in `workflow_refresh.rs`. Keep the recursion if you touch list traversal, otherwise reapply will silently skip run lists. `WorkflowRunListModel::reapply_filters` operates on the cached `last_runs`; ensure `set_runs` is called when data is fetched so chips can immediately re-filter without network.

- Files to reference when making changes
  - `src/main.rs` (runtime + app bootstrap)
  - `src/ui/main_window.rs` (primary UI patterns)
  - `src/ui/` (UI widget tests via `gtk_test_guard`)
  - `src/storage/token_storage.rs` (keyring usage)
  - `src/api/client.rs` and `src/api/models.rs` (API surface)
  - `README.md` (dev setup and system deps)

## Current repo status (short)

- Feature parity with the macOS client has been achieved and is documented in `TODO.md`. Before making behavior-changing edits, consult `TODO.md` for the consolidated status and optional polish items.
- Validation checklist for PRs that change behavior:
  1. Run `cargo test` and ensure all unit and logic tests pass.
  2. Run `cargo clippy -- -D warnings` and fix any lints (the repo enforces zero warnings).
  3. Run `cargo fmt` to ensure consistent formatting.
  4. For UI changes, run GTK-dependent tests locally when possible: `cargo test -- --ignored` (requires a display or headless Xvfb in CI).

If touching API/caching code, follow the ETag/ResponseHandler pattern in `src/api/http.rs` and respect rate-limit handling.

UI testing guidance
- UI tests live under `src/ui/**` and are marked ignored by default (they use the Rust test ignore attribute and the `gtk_test_guard` helper). This avoids running UI tests headless on CI without a display. They require an X11/Wayland display or a headless Xvfb/virtual framebuffer in CI.
- To run locally with a display (Linux):

```bash
# Run unit tests
cargo test

# Run UI tests (ignored by default)
cargo test -- --ignored
```

- In CI, prefer launching a headless X server or use a Docker container with a virtual framebuffer.

Audit notes for agents
- If you modify API code, ensure `ResponseHandler` rate limit updates and caching logic remain consistent. The handler stores ETags and cached bodies in-memory; persistence is intentionally not implemented to keep code simple.
- There is a small TTL-based optimization around actions permissions checks: `src/ui/main_window.rs` tracks `actions_checked_at` to avoid calling the non-ETag permission endpoint too often. If you change the permission check path, ensure the TTL logic is respected.
- When adding large async tasks, prefer `for_each_concurrent` with a concurrency cap (see `spawn_repo_status_tasks`) to avoid DoS and rate limit spikes.

If anything here is unclear or you need examples for a particular change (adding endpoints, changing auth, updating a UI pane), tell me which area to expand and I will update this file accordingly.


# Best Practices for Developing GNOME Applications with Rust and Libadwaita

This guide provides a comprehensive set of best practices for an AI agent to develop high-quality GNOME applications using Rust and libadwaita.

## 1. Project Setup and Dependencies

### 1.1. Initial Setup

- **Use `cargo` to create a new Rust project:**
  ```bash
  cargo new my-gnome-app
  cd my-gnome-app
  ```

- **Add necessary dependencies to `Cargo.toml` (match this repo's versions when editing Actioneer):**
  ```toml
  [dependencies]
  gtk4 = { version = "0.10", package = "gtk4" }
  libadwaita = { version = "0.8", package = "libadwaita", features = ["v1_5"] }
  gio = "0.21"

  [build-dependencies]
  glib-build-utils = "0.18.0"
  ```
  *Note: Ensure `gtk4` and `libadwaita` crate versions are compatible.*

### 1.2. System Dependencies

- Install `libadwaita` development libraries.
  - **Fedora:** `sudo dnf install libadwaita-devel`
  - **Debian/Ubuntu:** `sudo apt install libadwaita-1-dev`
  - **Arch Linux:** `sudo pacman -S libadwaita`

## 2. Application Structure and Idioms

### 2.1. Application Entry Point

- Use `adw::Application` instead of `gtk::Application`. This correctly sets up styles, icons, and translations.
- The application ID must be in reverse DNS format (e.g., `org.gnome.MyCoolApp`).

**Example `main.rs`:**
```rust
use adw::prelude::*;
use adw::Application;
use gtk4::{ApplicationWindow, Builder};

fn main() {
    let application = Application::builder()
        .application_id("com.example.MyGnomeApp")
        .build();

    application.connect_startup(|_| {
        adw::init();
    });

    application.connect_activate(|app| {
        let builder = Builder::from_string(include_str!("main.ui"));
        let window: ApplicationWindow = builder.object("window").unwrap();
        window.set_application(Some(app));
        window.present();
    });

    application.run();
}
```

### 2.2. UI Definition with Composite Templates

- **Optional:** This repo currently builds UI directly in Rust (no `.ui` templates). If you introduce `.ui` files, keep logic separate and wire them with composite templates.

**Example `main.ui`:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <template class="MyApplicationWindow" parent="AdwApplicationWindow">
    <property name="title">My GNOME App</property>
    <property name="default-width">600</property>
    <property name="default-height">400</property>
    <child>
      <object class="AdwHeaderBar" id="header_bar"/>
    </child>
  </template>
</interface>
```

### 2.3. Resource Management

- **Optional:** If you add `.ui` files or additional assets, embed resources into the binary using GResource.
- Create a `gresource.xml` file:
  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <gresources>
    <gresource prefix="/com/example/MyGnomeApp">
      <file>main.ui</file>
    </gresource>
  </gresources>
  ```
- **Compile resources in `build.rs`:**
  ```rust
  fn main() {
      glib_build_utils::compile_resources(
          &["src"],
          "src/gresource.xml",
          "com.example.MyGnomeApp.gresource",
      );
  }
  ```

## 3. Adherence to GNOME HIG

### 3.1. Use Libadwaita Widgets

- Prioritize using `libadwaita` widgets over standard GTK4 widgets whenever an equivalent exists.
- Examples: `AdwApplicationWindow`, `AdwHeaderBar`, `AdwPreferencesWindow`, `AdwToastOverlay`.

### 3.2. Styling

- **Avoid inline styling.** Do not set margins, padding, or colors directly in Rust code.
- **Use CSS classes.** Add style classes to widgets and define styles in a separate CSS file.
- **Leverage built-in styles:** Libadwaita provides styles like `.card`, `.pill`, etc.

### 3.3. Responsiveness

- Use adaptive widgets like `AdwFlap` and `AdwSqueezer` to create layouts that work on different screen sizes.

## 4. Asynchronous Operations

- For long-running tasks (e.g., network requests, file I/O), use asynchronous operations to avoid blocking the UI thread.
- In this repo, prefer `glib::MainContext::default().spawn_local(...)` or `glib::idle_add_local_once(...)` on the GTK thread.

**Example:**
```rust
glib::spawn_future_local(clone!(@weak self as widget => async move {
    // Perform async operation
    let result = some_async_function().await;
    // Update UI on the main thread
    widget.update_ui(result);
}));
```

## 5. State Management

- For simple applications, manage state within your widget structs.
- For more complex applications, consider using a separate state management struct or a library like `relm4`.

## 6. Internationalization (i18n)

- Use `gettext` for translations when adding i18n.
- Mark translatable strings in your code using the `gettext()` macro.
- Extract strings into `.pot` files and create `.po` files for each language.
- Note: this repo does not currently ship translations.

## 7. Packaging and Distribution

- **Flatpak is the preferred format** for distributing GNOME apps.
- Create a Flatpak manifest (`.json` or `.yaml` file) for your application.
- Use the Flatpak Rust extension for easier integration.
- Publish on Flathub to reach a wide audience.

## 8. Code Quality and Best Practices

- **Follow Rust idioms.** Write clean, safe, and idiomatic Rust code.
- **Handle errors gracefully.** Use `Result` and `Option` to handle potential failures.
- **Use `clone!` macro** to avoid ownership issues in closures and callbacks.
- **Adhere to XDG Base Directory Specification** for storing user data, configuration, and cache.
- Prefer small clean functions over large monolithic ones for better readability and maintainability.
- Prefer small files/modules over large ones to enhance code organization. Files should ideally be under 500 lines when possible.

By following these best practices, an AI agent can create a robust, modern, and well-integrated GNOME application that provides an excellent user experience on the Linux desktop.
