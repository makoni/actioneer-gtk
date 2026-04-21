# Testing Documentation

## Overview

This document describes the testing approach for the Actioneer GTK application.

## Test Structure

### 1. Logic Tests (`tests/logic_tests.rs`)

These tests verify business logic, helper functions, and state management without requiring GTK or a display server. They can run in CI/CD environments and on headless systems.

**Test Coverage:**
- ✅ Relative time formatting logic
- ✅ Status to icon name mapping (success, failure, cancelled, in_progress, queued)
- ✅ Status to CSS class mapping (success, error, warning, accent)
- ✅ Button visibility logic based on workflow run state
- ✅ Expansion state tracking with HashSet
- ✅ Auto-refresh interval parsing and conversion
- ✅ Run state checks (is_active, is_cancellable, is_rerunnable)

**Running logic tests:**
```bash
cargo test --test logic_tests
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
dbus-run-session -- bash tests/smoke/run_smoke.sh
```

The default scenario (`tests/smoke/welcome_screen.py`) verifies that the welcome screen renders with its signed-out CTAs. Additional scripts can be passed as `bash tests/smoke/run_smoke.sh path/to/script.py`.

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

## Test Metrics

Current test coverage:
- **Logic tests:** 7 tests, 100% passing
- **Unit tests:** 15 tests total in source files
- **Manual UI verification:** Required for each release

## Running Tests

```bash
# Run all tests (unit + logic)
cargo test

# Run only logic tests
cargo test --test logic_tests

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_status_icon_mapping

# Build and run the application for manual testing
cargo run
```

## Future Improvements

1. **Add integration tests** for API client with mocked responses (using `wiremock`)
2. **Add E2E tests** using dogtail or similar for automated UI testing
3. **Add property-based tests** for state management using `proptest`
4. **Add benchmarks** for performance-critical operations
5. **Add visual regression tests** for UI consistency

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
