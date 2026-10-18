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
