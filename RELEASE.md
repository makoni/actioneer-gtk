# Actioneer 1.1.0

A redesigned workflow detail pane, live durations, two new languages, and a demo mode.

## Summary

- Redesigned the repo detail pane: workflows, runs, jobs, and steps expand inline in one accordion view.
- Running jobs and steps now show live durations.
- Italian and Japanese translations added (14 languages total).
- Added a `--demo` launch mode with sample data for trying the app without an account.
- Updated bundled components to keep Actioneer reliable and up to date.

## What's changed

- **Workflows pane redesign**: workflows, runs, jobs, and steps now expand inline in the detail pane — no popup windows — with round tinted status dots, live run/job durations, and per-run actions (logs, trigger, cancel) in place.
- **Localization**: Italian and Japanese added across the app and listing metadata; 14 languages total.
- **Demo mode**: launch with `--demo` to explore the full UI with bundled sample repositories, runs, and logs.
- **Hover cleanup**: non-interactive step rows and job cards no longer light up on hover, matching GNOME guidance for non-actionable rows.
- **Dependency refresh**: updated `Cargo.lock` (base64 0.23, gettext-rs 0.8, futures 0.3.34, thiserror 2.0.20, keyring 4.1.6, open 5.4.2) and regenerated Flatpak cargo sources.

## Commits since `1.0.15`

```text
f6b42dc chore(screenshots): update English screenshots for improved visuals
b82e7bc style(ui): stop non-interactive run/job blocks from lighting up on hover
afb6e8d chore(deps): bump gettext-rs to 0.8.0 and open to 5.4.2
67a6a76 Merge pull request #64 from makoni/dependabot/cargo/keyring-4.1.6
070509f Merge pull request #65 from makoni/dependabot/cargo/open-5.4.1
352160a Merge pull request #69 from makoni/dependabot/cargo/futures-0.3.34
e734ed4 Merge pull request #70 from makoni/dependabot/cargo/thiserror-2.0.20
d0ef01d Merge pull request #66 from makoni/dependabot/cargo/base64-0.23.1
9691e54 Merge pull request #68 from makoni/feature/workflows-redesign
2edc0b4 feat(i18n): add Italian and Japanese to the language list
f248a0f style(ui): give the disclosure arrow horizontal breathing room
ad19c73 fix(ui): tick the run duration from the moment it appears
0105fa5 test(ui): guard that observing favourites leaves the button usable
8473b29 perf(ui): stop work that outlives what it was for
be09d40 chore: keep BUGS.md out of the repository
c41e1b1 fix(ui): release the detail pane when it is replaced
ecf9198 feat(ui): tick the duration of a running job or step
7eea174 fix(ui): line the sidebar favourite buttons up on one edge
ba35d64 fix(ui): centre status glyphs inside their dots
16973a7 docs: drop Copilot branding from the agent documentation
6746072 docs: bring AGENTS.md up to date and point agents at codegraph
24e8051 perf(ui): cache job logs for the life of the logs window
533a357 feat(ui): make the logs button open the run, not a guessed job
8aa1063 fix(ui): address fourth-round review findings
e8beabb test(ui): make the GTK tests actually run, and fix what that exposed
a2d2d83 fix(ui): address third-round review findings
ea5e5c3 polish(ui): clear the non-blocking review backlog
fbff004 fix(ui): address second-round review findings
502a2f9 fix(ui): address workflows redesign review findings
eeb1a65 chore(deps): bump thiserror from 2.0.19 to 2.0.20
95cf825 chore(deps): bump futures from 0.3.33 to 0.3.34
ec99bdb feat(ui): redesign Workflows view with accordion cards and status dots
dffcc79 Merge pull request #67 from makoni/feat/demo-mode
73b2614 chore(flatpak): sync cargo sources with Cargo.lock
566eb74 feat(demo): add --demo launch mode with sample data
6981c82 chore(deps): bump base64 from 0.22.1 to 0.23.1
a980461 chore(deps): bump open from 5.4.0 to 5.4.1
4f7305f chore(deps): bump keyring from 4.1.5 to 4.1.6
f8968d8 feat(ui): enrich run subtitle with branch, commit, actor, event, duration
c1cb6e8 chore(deps): update toml_parser to 1.1.3
6005126 Merge pull request #62 from makoni/chore/update-dependencies-2026-07-27
bafcddf chore(deps): update Cargo.lock dependencies
b418b5b Merge pull request #56 from makoni/dependabot/cargo/keyring-4.1.5
27880c6 Merge pull request #57 from makoni/dependabot/cargo/ashpd-0.13.13
ff44a54 Merge pull request #58 from makoni/dependabot/cargo/anyhow-1.0.104
47be95b Merge pull request #59 from makoni/dependabot/cargo/serde-1.0.229
315aa04 Merge pull request #60 from makoni/dependabot/cargo/futures-0.3.33
2a082d8 Merge pull request #61 from makoni/dependabot/github_actions/actions-7a5a078ad4
51f596f chore(deps): bump actions/checkout in the actions group
eae4384 chore(deps): bump futures from 0.3.32 to 0.3.33
7b1d683 chore(deps): bump serde from 1.0.228 to 1.0.229
2845680 chore(deps): bump anyhow from 1.0.103 to 1.0.104
7f8cdec chore(deps): bump ashpd from 0.13.12 to 0.13.13
f546fea chore(deps): bump keyring from 4.1.4 to 4.1.5
88cda7a test(ui): cover remaining migrated dialogs
9e1ffe9 test(ui): cover sign-out AlertDialog structure
1ec7093 feat(gtk): adopt gtk4 0.11.4 (v4_10) and migrate off deprecated dialogs
269e954 Merge pull request #55 from makoni/chore/update-deps
33618eb chore(deps): update non-gtk dependencies
71921cd Merge pull request #54 from makoni/dependabot/cargo/open-5.4.0
22332aa chore(flatpak): sync cargo sources for open bump
2ccae4c chore(deps): bump open from 5.3.6 to 5.4.0
e617a4f Merge pull request #53 from makoni/dependabot/cargo/gtk-rs-2556097806
612db70 chore(flatpak): sync cargo sources for libadwaita bump
0e4200a chore(deps): bump libadwaita from 0.9.1 to 0.9.2 in the gtk-rs group
07cc32d Merge pull request #52 from makoni/dependabot/cargo/keyring-4.1.4
57b58f0 chore(flatpak): sync cargo sources for keyring bump
2cdf1bd chore(deps): bump keyring from 4.1.3 to 4.1.4
4d6bf48 Merge pull request #51 from makoni/chore/ignore-gtk4-0.11.4
21da4b7 chore(dependabot): ignore gtk4 0.11.4 (breaks libadwaita 0.9.1)
dae1398 Merge pull request #49 from makoni/dependabot/cargo/keyring-4.1.3
6a27ed1 chore(flatpak): sync cargo sources for keyring bump
97a5d46 Merge pull request #48 from makoni/dependabot/cargo/open-5.3.6
abe8fc6 Merge develop into dependabot open bump
9f1ed1e chore(flatpak): sync cargo sources for open bump
381a15b chore(deps): bump keyring from 4.1.2 to 4.1.3
1b290e5 Save uncommitted changes
6c9ebaf chore(deps): bump open from 5.3.5 to 5.3.6
ae625c3 Merge pull request #46 from makoni/dependabot/cargo/anyhow-1.0.103
5ab40c6 chore(flatpak): sync cargo sources for anyhow bump
363715a Merge pull request #45 from makoni/dependabot/cargo/ashpd-0.13.12
afc47ca chore(flatpak): sync cargo sources for ashpd bump
d7f1d65 Merge pull request #44 from makoni/dependabot/github_actions/actions-b549e000f0
fb33e67 chore(deps): bump anyhow from 1.0.102 to 1.0.103
ccef5a2 chore(deps): bump ashpd from 0.13.11 to 0.13.12
bfe1a78 chore(deps): bump the actions group with 2 updates
```

## User impact / compatibility

- Existing users do not need to migrate data or settings.
- The repo detail pane looks and behaves differently: workflows, runs, jobs, and steps expand inline instead of opening popup windows. All previous capabilities are preserved.
- Italian and Japanese are available in the app language settings; existing translations carry over.
- A `--demo` launch mode is available for exploring the app without signing in.
