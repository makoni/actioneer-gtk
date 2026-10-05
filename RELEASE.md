# Actioneer 1.1.2

A maintenance release: a sign-out fix, refreshed dependencies, and a large
internal restructuring with much broader test coverage behind it.

## Summary

- Signing out now always returns to the welcome screen right away, including in demo mode and when the system keyring is unavailable.
- Refreshed bundled components (dependencies) to keep Actioneer reliable and up to date.
- The codebase was reorganised into clear layers and is now covered by headless, GTK, and end-to-end accessibility tests.
- A security policy describes how to report vulnerabilities privately.

## What's changed

- **Sign-out fix**: confirming sign-out could leave the signed-in UI on screen, or hang, when the system keyring was locked, missing, or unreachable (the secret service call blocks instead of failing), and in demo mode. Sign-out now leaves the signed-in UI immediately and deletes the stored token in the background, so no token is left behind either.
- **Dependency refresh**: thiserror 2.0.21, reqwest 0.13.5, open 5.4.4, dirs 7.0 (its only breaking change is Windows-specific), gio 0.22.10, and the gtk4 family 0.11.5, with the Flatpak cargo sources regenerated. CI moved to `setup-rust-toolchain` v2, which still denies warnings, now through `CARGO_BUILD_WARNINGS`.
- **Architecture**: the crate is split into a library and a thin binary, with `kernel`, `runtime`, `domain`, `services`, `demo`, and `ui` layers. Demo data goes through a gateway instead of the HTTP client, real services are built in one composition root, and errors become user-facing text in one place.
- **Testing**: characterization tests freeze the demo data and the API client's behaviour (against `wiremock`); GTK tests run on a single shared worker and check that widget trees are released; eight AT-SPI smoke journeys drive the release binary end to end. GUI tests no longer depend on the window manager's sizing, and the smoke harness also works on distributions whose accessibility bus uses dbus-broker.
- **Security policy**: `SECURITY.md` defines the supported versions and the reporting channel (GitHub private vulnerability reporting).

## Commits since `1.1.1`

```text
a84ce93 docs: add a security policy
b1f4eb3 chore(deps): finish the dependency bumps from #73-#81
3998bd7 test: make GUI and smoke tests independent of the desktop they run on
40ad1ad chore(deps): bump thiserror from 2.0.20 to 2.0.21
c3a5c1a chore(deps): bump the gtk-rs group with 2 updates
d4cb72f chore(deps): bump actions-rust-lang/setup-rust-toolchain
29f913b chore(deps): bump reqwest from 0.13.4 to 0.13.5
9ad4562 chore(deps): bump open from 5.4.2 to 5.4.4
47be8d2 chore(deps): bump dirs from 6.0.0 to 7.0.0
e416a7a docs: state the gtk4 layering invariant correctly (#79)
84d80be test(ui): close the coverage gap step 6.6 left open (#78)
894d4d9 fix: address the review of #77
e0c482d fix(auth): sign out of demo mode without going through the keyring
b2e6009 docs: describe the new layers and record the settled decisions
6f4fe71 refactor(phase-7): one place where an error becomes text
eb6d1d3 refactor(phase-7): one module layout convention across the tree
71ae661 refactor(phase-7): file the single-owner helpers next to their owners
31eb390 refactor(phase-5): finish splitting the workflow row builder
2597378 refactor(phase-5): extract the trigger dialog, and cover it with a journey
3a14a8d refactor(phase-5): split main_window and workflow_refresh
a63c638 refactor(phase-5): split the run list, sidebar and job-logs window
26a01ab refactor(phase-5): move test modules into their own files
c773d3a test: extend the logic tests over domain::filters and domain::counts
6cbe8b3 test(ui): construct MainWindow from fake services
b2893c3 refactor(phase-4): composition root and services/
61c8904 refactor(phase-3): finish domain/ with filters and repository counts
d976c4f refactor(phase-1): give the kernel its own i18n and value types
151c6a3 test: make tests/ test this crate, and document the runs
3723a4b refactor(domain): extract the pure run and duration logic
891245f refactor(domain): move the models out of api/ and the display strings out of the models
dae09ea refactor(phase-2): route demo data through a gateway, not through the client
074b3cb test(ui): cover the untested widget files
ead9e67 test(ui): unit-test the pure logic Phase 3 will move into domain/
9b627ca test(smoke): add the preferences and sign-out journey
200d542 test(smoke): add the filters-and-favourites journey
7e74abf test(smoke): add the run-list and job-logs journeys
cede51a test(smoke): add the repos-to-workflows journey
1649007 test(smoke): factor the AT-SPI harness into a shared library
df22671 test(api): add a base-URL override and characterize the client
e642604 test(smoke): add the demo-mode journey
a396611 test(demo): characterize the demo data surface
f780ecb test(demo): expose the demo surface to integration tests
71d2f89 fix(smoke): pin the X11 backend and forward launch arguments
c503aa2 refactor(phase-0): split the crate into [lib] + [[bin]]
3df1c1b test(storage): ignore the destructive live-keyring test
```

## User impact / compatibility

- Existing users do not need to migrate data or settings.
- The only behavioural change is the sign-out fix above; everything else is internal or packaging.
- The release commit itself (version bump and translations) is not listed above; it lands after this file is written.
