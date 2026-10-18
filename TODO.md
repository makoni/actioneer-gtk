# TODO: Backlog

This file tracks **open** work only. The full history of completed work lives in
git commits (search the log) and in the previous long-form TODO in repo history.

Where the code lives and how to validate a change are documented once, in
`AGENTS.md` and `docs/agent-guide.md` — not repeated here, where the copy only
drifts.

## Open items (optional / low priority)

- Sandbox detection is written three times and has drifted:
  `services/tokens/secret_portal.rs` knows the `/.flatpak-info` marker file
  (the environment can be stripped, the file cannot), while
  `services/notifications/env_config.rs::is_sandboxed` and `kernel/i18n.rs`
  check environment variables only. One probe in `kernel/` should serve all
  three.
- Snap: strict confinement cannot read `~/.local/state/omarchy`, so the
  Omarchy theme-follow feature is unavailable in the Snap build (its `$HOME`
  is the snap's private data directory). Exposing the host path would need a
  `personal-files` plug and a store request.
- `ci.yml` never builds the Flatpak, so a dependency that breaks only under
  the GNOME SDK's `CFLAGS` surfaces when `publish.yml` runs. aws-lc-sys 0.45.0
  did exactly that (jitterentropy refuses `-O2`) and was found at 1.2.0.
  Running `flatpak-ci.yml` from `ci.yml`, or at least after any `Cargo.lock`
  change, would catch it earlier.
