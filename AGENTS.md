AGENTS.md

This repository includes guidance for automated coding agents (Copilot-style) in `.github/copilot-instructions.md`.

This file documents the responsibilities and workflow for agents working on this repo.

Agent responsibilities
- Read `.github/copilot-instructions.md` before making changes. It contains the project's runtime, UI, and concurrency rules.
- Prefer in-repo docs under `docs/` for widget usage instead of external web pages.
- When changing UI code, consult `docs/libadwaita/` for widget behavior and constraints.
- After edits run `cargo fmt`, `cargo check`, `cargo clippy --all-targets --all-features`, and `cargo test` (the same checks CI runs).
- Add or update tests for functional changes. Unit tests live in `src/` files using `#[cfg(test)]`.

Development workflow for agents
1. Read `.github/copilot-instructions.md` and this `AGENTS.md`.
2. **Use TODO.md as the single source of truth for progress tracking.** Update TODO.md with:
   - Mark items as [🔄] when starting work
   - Mark items as [✅] when completed
   - Add detailed notes in the "Recent Updates" section
   - Keep the file current throughout the session
3. Make minimal, well-scoped edits. Prefer edits that are small and testable.
4. Run format and build checks locally in the workspace. Use the project's cargo toolchain.
5. When adding HTTP caching or background work, ensure:
   - Network calls run on Tokio via `crate::runtime_handle().spawn(...)`.
   - GTK widget updates happen on GLib main thread via `glib::MainContext::default().spawn_local(...)` or `glib::idle_add_local_once(...)`.
6. Push changes and open a PR. Validate **locally** — no workflow triggers on `push` or `pull_request` (see "Continuous integration" below), so a green PR is not automatic. The CI clippy gate runs `cargo clippy --all-targets --all-features`.

Additional guidance (matching `.github/copilot-instructions.md`)

- Rust toolchain: prefer using `rustup` and pinning a toolchain for reproducible development (for example by adding a `rust-toolchain.toml` file in the repo). If a pinned toolchain is not available, use the latest `stable` channel. After switching toolchains run `cargo clean` then `cargo build` to ensure dependencies are rebuilt for the active toolchain.

- Headless UI tests: when running the ignored UI tests on a headless Linux machine, use `xvfb-run` to provide a virtual X server. Example:

   ```bash
   xvfb-run -s "-screen 0 1280x1024x24" cargo test -- --ignored
   ```

   In CI prefer to either run tests in a container/image that includes an X server or use the above `xvfb-run` wrapper.

- Token/keyring safety: `src/storage/token_storage.rs` contains a live keyring test and some operations that may write to or delete entries in the system keyring. Do NOT run or modify those destructive tests on developer machines unless you understand and accept the side-effects. Prefer using mocks or a dedicated test keyring account when adding or changing tests that interact with the system keyring.

- Continuous integration: no workflow runs on `push`/`pull_request`. `ci.yml` (lock-sync → fmt/clippy → build+test on x86_64/aarch64 → UI smoke + integration tests) is `workflow_dispatch` only; the package/release workflows are manual or `workflow_call`. Validate locally, and to exercise the real pipeline trigger it by hand: `gh workflow run ci.yml --ref develop` then `gh run watch <id> --exit-status`.

- Dependency changes: when anything edits `Cargo.lock` (including `cargo update`), regenerate the Flatpak vendored-sources manifest with `scripts/regenerate-flatpak-sources.sh` and commit `flatpak/me.spaceinbox.actioneer.cargo-sources.json` alongside it; confirm with `scripts/check-flatpak-lock-sync.sh`. Skipping this fails the `lock-sync` CI job (which skips every downstream job) and is NOT caught by `cargo check/clippy/test`. See `.github/copilot-instructions.md` for the full Flatpak packaging workflow.

Hand-off to Copilot Coding Agent
- If you want an asynchronous agent to continue implementing a large task, add the hashtag `#github-pull-request_copilot-coding-agent` to the PR description and include the task body. The agent will create a branch and follow the instructions.

Notes
- The project uses `parking_lot::Mutex` for shared state in many places. Use `Arc<Mutex<T>>` for state passed to tokio tasks.
- There is an existing ETag caching implementation in `src/api/http.rs`. If you update API endpoints, consider reusing that mechanism.

Contact
- If something is unclear, open an issue or ping the repo owner in GitHub.