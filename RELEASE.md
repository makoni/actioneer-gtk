# Actioneer 1.0.13

Maintenance release with one user-visible polish fix and Linux build reliability
improvements.

## What's changed

- **Localized dates for older runs**: workflow runs older than a week no longer
  fall back to raw ISO dates in the run list. They now use the active app
  locale, so older history is easier to scan.
- **Linux gettext build fix**: switched to the platform gettext/libintl path on
  Linux instead of compiling vendored GNU gettext. This avoids the upstream
  `gettext-sys` / gnulib C23 `_Generic` build failure affecting current Linux
  toolchains.
- **Dependency refresh**: includes the already-merged dependency maintenance
  updates that landed after `1.0.12`, including newer `openssl`, `tokio`,
  `getrandom`, and the Rust toolchain setup action.

## Commits since `1.0.12`

```
9178f9c fix: localize old run dates and use system gettext
ace347c chore: refresh dependencies after dependabot merges
1404581 Merge pull request #32 from makoni/dependabot/cargo/getrandom-0.4.2
f5629ff Merge pull request #31 from makoni/dependabot/github_actions/actions-3cd9f16a23
8729a4a Merge pull request #30 from makoni/dependabot/cargo/tokio-1.52.3
407042e Merge pull request #29 from makoni/dependabot/cargo/openssl-0.10.79
```

## User impact

- Older run history is easier to read in non-English locales.
- No workflow behavior changes, new features, or auth changes.
- Linux source builds and packaging are more reliable on current toolchains.

## Compatibility

- Minimum Rust toolchain for building from source remains `1.95.0` stable.
- Runtime requirements are unchanged.
- No migration steps are required for users.
