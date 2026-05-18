# Actioneer 1.0.14

Maintenance release focused on shutdown/crash-report reliability and dependency
refreshes. There are no new end-user features in this release.

## What's changed

- **More accurate crash recovery tracking**: Actioneer now tracks each running
  app instance separately, which prevents normal multi-window or multi-launch
  usage from being mistaken for a previous crash.
- **Safer shutdown handling**: Unix shutdown cleanup now covers more terminal
  and session signals, and the signal registration path no longer trips the
  release smoke test on startup.
- **Portal auth/storage resilience**: secret-portal token retrieval now fails
  with an explicit timeout instead of hanging indefinitely when desktop portal
  services are slow or unavailable.
- **Dependency refresh**: includes the merged `ashpd` 0.13.11 bump and the
  latest compatible lockfile refresh after `1.0.13`.

## Commits since `1.0.13`

```
57b1bcf Refresh Cargo lockfile dependencies
5521a35 Merge pull request #34 from makoni/dependabot/cargo/ashpd-0.13.11
0890030 Fix Unix signal handler runtime context
e120f56 Fix crash session tracking and shutdown handling
57ae9c7 chore(deps): bump ashpd from 0.13.10 to 0.13.11
```

## User impact

- Fewer false crash prompts after normal shutdowns or relaunches.
- Better recovery when portal-backed secure storage is slow to respond.
- No UI workflow changes or migration steps for existing users.

## Compatibility

- Minimum Rust toolchain for building from source remains `1.95.0` stable.
- Runtime requirements are unchanged.
- No migration steps are required for users.
