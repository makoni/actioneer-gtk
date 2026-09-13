# Testing Documentation

## Overview

This document describes the testing approach for the Actioneer GTK application.

## Test Structure

### 1. Integration tests (`tests/`)

Each file in `tests/` is its own test binary and can only reach the crate's
**public** API — which is why the crate is split into `[lib]` + `[[bin]]`. None
of these needs GTK or a display.

| File | What it covers |
|---|---|
| `logic_tests.rs` | `domain::runs` predicates and `domain::formatting` durations |
| `characterization_demo.rs` | everything the demo backend returns, frozen shape by shape |
| `characterization_api.rs` | the live client against `wiremock`: parsing, error mapping, rate-limit headers |
| `gateway.rs` | the gateway's dispatch — that live and demo route to different places |
| `common/mod.rs` | shared frozen fixtures; not a test target itself |

**Characterization tests are not ordinary tests.** Their expected values are
frozen: a refactor may change *how* a test reaches a value, never *what* it
asserts. If one starts failing, the refactor changed behaviour — fix the code,
not the constant.

```bash
cargo test --test logic_tests
cargo test --workspace          # everything headless
```

### 2. Unit Tests (in source files)

The application includes unit tests within source files for specific modules:

- `src/api/client.rs` - HTTP client creation and configuration
- `src/api/models.rs` - JSON deserialization, rate limit checks, workflow models
- `src/auth/device.rs` - OAuth device flow structures
- `src/cache.rs` - Workflow caching and cache clearing
- `src/config.rs` - Configuration validation
- `src/favorites.rs` - Favorite repository management
- `src/notifications.rs` - Notification text formatting
- `src/preferences.rs` - Preferences management
- `src/storage/token_storage.rs` - Token storage lifecycle (runs live keyring tests)

**Running all unit tests:**
```bash
cargo test
```

## Testing Challenges & Approach

### GTK UI Testing Limitations

GTK4 UI testing faces several challenges:

1. **Requires Display Server:** GTK initialization requires an X11/Wayland display server or Xvfb
2. **Main Loop Required:** Many GTK operations require a running GLib main loop
3. **Async Complexity:** GTK runs on GLib's event loop while the app uses Tokio for async operations
4. **CI/CD Complexity:** Running GTK tests in CI requires setting up virtual display servers

### Current Testing Strategy

Given these limitations, we use a **hybrid testing approach**:

1. **Logic Tests** - Test all business logic, state management, and helper functions without GTK
2. **Ignored GTK Unit Tests** - Widget-construction tests marked `#[ignore = "requires GTK display"]` run via `cargo test -- --ignored` under Xvfb + D-Bus
3. **Smoke Tests** - Black-box accessibility-tree checks under Xvfb that launch the release binary and verify user-facing elements (see `tests/smoke/`)
4. **Manual UI Testing** - UI behavior is verified through manual testing with the running application

### Smoke Tests (`tests/smoke/`)

The smoke suite launches the actual release binary under `Xvfb` + `dbus-run-session` + AT-SPI, then uses `pyatspi` (Python bindings) to inspect the accessibility tree. Requires `xvfb`, `dbus-x11`, `at-spi2-core`, and `python3-pyatspi`.

Run locally:

```bash
cargo build --release
dbus-run-session -- bash tests/smoke/run_all.sh
```

`run_all.sh` runs every journey with the launch arguments it needs and stops at
the first failure. To run one on its own:

```bash
ACTIONEER_ARGS=--demo dbus-run-session -- \
  bash tests/smoke/run_smoke.sh tests/smoke/job_logs.py
```

Seven journeys today: `welcome_screen`, `demo_mode`, `repos_to_workflows`,
`runs_and_detail`, `job_logs`, `filters_and_favorites`,
`preferences_and_signout`. `lib.py` holds the shared helpers and `fixtures.py`
the frozen accessibility names; neither is a journey, which is why `run_all.sh`
lists scripts explicitly instead of globbing.

**A release build is required** — `run_smoke.sh` defaults `ACTIONEER_BIN` to
`target/release/actioneer`. On a Wayland desktop the harness pins
`GDK_BACKEND=x11` itself; without that GTK prefers the inherited Wayland
session and reports "Failed to open display" even though Xvfb is up.

Three helpers exist because the obvious AT-SPI approaches silently do nothing on
GTK4, which makes a test pass while checking nothing:

- `click()` looks an action up **by name**. `doAction(0)` on a label is
  `clipboard.copy` — it succeeds and changes nothing.
- `select_row_named()` selects through the enclosing list's `Selection`
  interface. `ListView` rows expose no select action and have an empty
  accessible name.
- `do_window_action()` invokes a window GAction on the frame. Items inside a
  `GtkMenuButton` popover never reach the accessibility tree at all.

## Manual UI Test Checklist

When testing UI changes, verify the following:

### Main Window
- [ ] Window opens without errors
- [ ] Repository list loads and displays correctly
- [ ] Search functionality filters repositories
- [ ] Favorites section shows favorited repos
- [ ] Loading spinner appears when fetching repos
- [ ] Reload button refreshes the repository list

### Detail View (Repo Selected)
- [ ] Repo header shows correct name and privacy status
- [ ] Favorite button toggles correctly
- [ ] Workflows list displays correctly
- [ ] Workflow expanders can be expanded/collapsed
- [ ] Loading spinner shows when refreshing workflows

### Workflow Runs
- [ ] Runs display with correct status icons (success=green, failure=red, etc.)
- [ ] Run metadata shows correctly (branch, time, status, conclusion)
- [ ] Action buttons appear correctly based on run state:
  - Completed runs: show rerun button
  - Failed runs: show "rerun failed jobs" button
  - In-progress/queued runs: show cancel button
  - All runs: show "open in GitHub" button
- [ ] Clicking action buttons triggers correct API calls
- [ ] Run expanders can be expanded/collapsed to show jobs

### Jobs
- [ ] Jobs display under expanded runs
- [ ] Job status icons show correctly
- [ ] Job metadata displays (name, status, conclusion, branch)
- [ ] "Open in GitHub" button works for jobs

### State Preservation
- [ ] Expanded workflows remain expanded after refresh
- [ ] Selected repo remains selected after reload
- [ ] Favorite status persists across app restarts

### Performance
- [ ] No infinite loops or excessive API calls
- [ ] ETag headers used for conditional requests
- [ ] Rate limit display updates after each API call
- [ ] App remains responsive during data loading

## Test metrics

- **Headless:** 226 passing (`cargo test --workspace`)
- **GTK, `#[ignore]`d:** 73 tests, run under Xvfb
- **Smoke journeys:** 7
- **Manual UI verification:** still required for each release

## Running Tests

The full gate, in the order CI runs it:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
xvfb-run -a dbus-run-session -- bash -lc \
  "RUST_TEST_THREADS=1 cargo test --workspace -- --ignored --test-threads=1 \
   --skip test_token_storage_lifecycle"
cargo build --release && dbus-run-session -- bash tests/smoke/run_all.sh
```

**`--skip test_token_storage_lifecycle` is not optional.** That test writes the
developer's real system keyring, and `--ignored` runs precisely the tests marked
`#[ignore]` — so marking it ignored moves it *into* that batch rather than out
of the run.

Everyday commands:

```bash
cargo test --workspace          # headless only
cargo test --test logic_tests   # one integration binary
cargo test -- --nocapture       # with output
cargo run                       # the app, for manual testing
cargo run -- --demo             # the app with sample data
```

## Future improvements

1. **Property-based tests** for the domain predicates using `proptest`
2. **Benchmarks** for the run-list rendering path
3. **Visual regression tests** for UI consistency

## Debugging Tests

For debugging failing tests:

```bash
# Run tests with debug logging
RUST_LOG=debug cargo test -- --nocapture

# Run tests with backtrace
RUST_BACKTRACE=1 cargo test

# Run single test with full output
cargo test test_expansion_state_tracking -- --exact --nocapture
```
