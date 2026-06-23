# Actioneer 1.0.15

Maintenance release focused on sign-in compatibility and dependency refreshes.
There are no new end-user features in this release.

## Summary

- Improved compatibility of secure token storage on newer Linux environments.
- Refreshed Rust dependencies and packaging lock artifacts.
- Updated CI and maintenance docs to reflect current lock-sync and validation flow.

## What's changed

- **Keyring compatibility fix**: migrated token storage integration to the keyring 4.1 v1 API so authentication works reliably on updated Linux stacks.
- **Dependency refresh**: updated `Cargo.lock` and regenerated Flatpak cargo sources to stay aligned with current compatible crate versions.
- **Maintenance updates**: tightened CI/docs guidance around lock-sync and local validation workflows.

## Commits since `1.0.14`

```text
9951986 docs(todo): compact TODO.md to an open backlog
04e836b docs(agents): correct CI guidance and add dependency/lock-sync notes
67599fe chore(flatpak): regenerate cargo-sources from current Cargo.lock
d6cef12 chore(deps): refresh Cargo.lock to latest compatible versions
f2d9fd4 Merge pull request #42 from makoni/fix/keyring-4.1-migration
6b89ba4 Merge remote-tracking branch 'origin/develop' into fix/keyring-4.1-migration
9692786 fix(storage): migrate to keyring 4.1 v1 API
bf8b208 Merge pull request #40 from makoni/dependabot/cargo/getrandom-0.4.3
76a1e07 Merge pull request #39 from makoni/dependabot/github_actions/actions-2217aebe03
bea3e00 chore: ignore .codegraph local index
5ddb1d6 chore(deps): bump getrandom from 0.4.2 to 0.4.3
4a0ea28 chore(deps): bump actions/checkout in the actions group
8f7d1d9 Merge pull request #38 from makoni/dependabot/cargo/chrono-0.4.45
9d08cc0 Merge pull request #36 from makoni/dependabot/cargo/reqwest-0.13.4
b828e95 Merge pull request #37 from makoni/dependabot/github_actions/actions-6a98abd9ac
bdb7572 chore(deps): bump chrono from 0.4.44 to 0.4.45
4daf40e chore(deps): bump actions/checkout in the actions group
c4d1d94 chore(deps): bump reqwest from 0.13.3 to 0.13.4
abc862b Merge pull request #35 from makoni/dependabot/cargo/serde_json-1.0.150
0b6a051 chore(deps): bump serde_json from 1.0.149 to 1.0.150
```

## User impact / compatibility

- Existing users do not need to migrate data or settings.
- Authentication and token persistence are more reliable on modern Linux desktops.
- No UI behavior changes; this is a maintenance patch release.
