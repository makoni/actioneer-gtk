AGENTS.md

Guidance for automated coding agents working on this repository. The companion
file `.github/copilot-instructions.md` holds the runtime, UI and concurrency
rules plus a long-form GNOME/libadwaita style guide; read it before your first
edit. This file covers how to work here: what to read, how to validate, and the
traps that cost the most time.

## Explore with codegraph before reading files

This repo is indexed by the **codegraph** MCP server. Its single tool,
`codegraph_explore`, is Read-equivalent: given a question or a bag of symbol
names it returns the verbatim, line-numbered source of the relevant symbols
grouped by file, the call path among them, and a blast-radius list of what
depends on them (including "no covering tests found" warnings).

- Call it **first** for almost anything: "how does X work", "where is X",
  surveying an area, and — just as important — before editing a symbol, so you
  see the callers you are about to break.
- One call usually answers the whole question. Do **not** follow it with a
  grep/Read sweep over the same files: the source it printed is current on-disk
  content, already read.
- The index lives in `.codegraph/` (git-ignored, ~10 MB, local to each machine)
  and trails writes by about a second, so it reflects edits you just made.
- Fall back to Grep/Read only when the server is unavailable or when you need a
  file it does not cover (workflows, `po/`, scripts, docs).

Use the normal file tools for editing — codegraph is for finding and
understanding, not for writing.

## Read next

- `.github/copilot-instructions.md` — runtime, UI, concurrency and API rules.
- `docs/libadwaita/` — widget behaviour and constraints. Prefer these in-repo
  docs over external web pages when working on UI.
- `TODO.md` — the open backlog (see "Progress tracking" below).

## Validation

CI does not run on `push` or `pull_request` (see "Continuous integration"), so
nothing validates a PR unless you do it locally. Run all of these:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
xvfb-run -a dbus-run-session -- bash -lc "RUST_TEST_THREADS=1 cargo test --workspace -- --ignored --test-threads=1"
```

**`-D warnings` is not optional.** CI installs the toolchain with
`actions-rust-lang/setup-rust-toolchain`, whose `rustflags` input defaults to
`-D warnings`, and no workflow overrides it. Every CI step — `build`, `test`,
`clippy` — therefore fails on any warning, including `dead_code` on a function
you just orphaned. Locally that flag is off by default, so a clean local run
proves nothing unless you pass it yourself.

Add or update tests for functional changes. Unit tests live beside the code in
`#[cfg(test)]` modules; `tests/` holds the logic-level integration tests.

### GTK tests

UI tests are marked `#[ignore = "requires GTK display"]` and must go through
`crate::ui::test_helpers::run_gtk_test`. GTK objects are bound to the thread
that initialised them, but libtest gives every `#[test]` its own thread even at
`--test-threads=1`, so the helper runs every body on one shared GTK worker.
Never hand-roll a "skip if GTK is not initialised" guard: an earlier version of
that guard silently skipped 30 of 31 UI tests while reporting them as passed.

If a headless run needs to render — screenshots, or GL errors from a bare Xvfb —
set `GSK_RENDERER=cairo`.

### Smoke tests

`tests/smoke/run_smoke.sh <script.py>` drives the built binary through AT-SPI
(`pyatspi`) under Xvfb. It starts Xvfb but **not** a session bus, so wrap it:

```bash
dbus-run-session -- bash tests/smoke/run_smoke.sh
```

Without a bus the a11y bridge comes up half-initialised: the app frame is
visible but its widget tree reads as almost empty, which looks like a UI bug and
is not one. Note also that GTK4 buttons surface with role name `button`, not
`push button` — filtering on the latter silently finds nothing.

## Progress tracking

`TODO.md` is the **open backlog**, not a session journal: it tracks work that is
not yet done, plus a short "Recent Updates" log. The full history of completed
work lives in git commits.

- Track your own in-session steps with your task/todo tooling, not by editing
  `TODO.md` on every step.
- Update `TODO.md` when the *backlog* changes: an open item is finished, a new
  one appears, or a landed change is worth a "Recent Updates" line.
- Keep its "Where to look" pointers accurate when you move or delete modules.

## Threading and reference cycles

- Network calls run on Tokio via `crate::runtime_handle().spawn(...)`.
- GTK widget updates happen on the GLib main thread — via the project's
  `MainContextChannelExt` channel helper, `glib::MainContext::default()
  .spawn_local(...)`, or `glib::idle_add_local_once(...)`.
- Shared state passed to Tokio tasks uses `Arc<parking_lot::Mutex<T>>`.

**Signal handlers must not hold a strong reference to any ancestor of the widget
they are attached to.** The widget owns the handler, the handler owns the
ancestor, and the ancestor owns the widget: the subtree is then never finalized,
which also defeats weak-ref guards on refresh timers, so they tick forever. This
repo does not use `glib::clone!`; capture `widget.downgrade()` and `upgrade()`
inside the closure instead (27 call sites do this today).

Guard new widget trees with a release test — see
`run_row_is_released_when_dropped` (`src/ui/detail_view/helpers/runs/row.rs`) and
`window_is_released_once_closed` (`src/ui/job_logs_window.rs`). Both use
`test_helpers::collect_widget_weaks` to assert the *whole* subtree died, not just
its root: a cycle pinning one inner widget passes a root-only check. When you add
such a test, prove it can fail by reintroducing the cycle once.

## Toolchain and dependencies

- Rust `stable`; no toolchain file is pinned, and CI installs `stable`. After
  switching toolchains run `cargo clean` before `cargo build`.
- `gtk4` 0.11 with feature `v4_10`, `libadwaita` 0.9 with `v1_5`. Because CI
  denies warnings, deprecated APIs are effectively banned — e.g. use
  `adw::AlertDialog`/`adw::Dialog`, not `gtk::MessageDialog`. Under `v4_10`,
  `ListItem` factory closures need an explicit downcast of the list item.
- When anything edits `Cargo.lock` (including `cargo update`), regenerate the
  Flatpak vendored-sources manifest with `scripts/regenerate-flatpak-sources.sh`
  and commit `flatpak/me.spaceinbox.actioneer.cargo-sources.json` alongside it;
  confirm with `scripts/check-flatpak-lock-sync.sh`. Skipping this fails the
  `lock-sync` CI job — which every other job depends on — and is **not** caught
  by `cargo check`/`clippy`/`test`.

## Internationalization

User-facing strings go through `tr(...)`. There are 14 catalogs in `po/`.
`scripts/extract-translations.sh` regenerates the `.pot`; when adding a string,
insert the new `msgid`/`msgstr` pair into each `.po` surgically. Do not run a
blanket `msgmerge` over the catalogs: it rewrites every file wholesale and buries
your change in a five-figure diff.

## Continuous integration

No workflow runs on `push` or `pull_request`. `ci.yml` (lock-sync → fmt/clippy →
build+test on x86_64/aarch64 → UI tests + welcome-screen smoke test) is
`workflow_dispatch` only; the package and release workflows are manual or
`workflow_call`. To exercise the real pipeline:

```bash
gh workflow run ci.yml --ref <branch>
gh run watch <id> --exit-status
```

## Keyring safety

`src/storage/token_storage.rs` contains a live keyring test and operations that
may write to or delete entries in the system keyring. Do not run or modify those
destructive tests on a developer machine unless you accept the side effects.
Prefer mocks or a dedicated test keyring account.

## API and caching notes

`src/api/http.rs` implements ETag caching through `ResponseHandler`; reuse it
when adding endpoints, and keep rate-limit updates intact. For large async fan-
out, use `for_each_concurrent` with a concurrency cap (see
`spawn_repo_status_tasks`) rather than spawning per item.

## Hand-off to Copilot Coding Agent

To have an asynchronous agent continue a large task, add the hashtag
`#github-pull-request_copilot-coding-agent` to the PR description together with
the task body. The agent will create a branch and follow the instructions.

## Contact

If something here is unclear or wrong, open an issue or ping the repo owner.
