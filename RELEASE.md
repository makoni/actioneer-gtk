# Actioneer 1.0.8

Actioneer 1.0.8 focuses on making live GitHub Actions monitoring more reliable while using the GitHub API more efficiently.

## Highlights

- In-progress workflow runs now keep refreshing while they are still active, so expanded jobs and steps stay current before the run finishes.
- Expanded run details are more stable and easier to read, with clearer badges, preserved job context, and contiguous step numbering.
- Workflow status badges are now translated more consistently across the app's supported languages.
- Background refresh is lighter on the GitHub API, reducing duplicate requests and helping preserve rate-limit headroom.

## Important technical improvements

- Fixed several live-refresh lifecycle issues in the detail pane so switching repositories or rebuilding rows no longer disables the active refresh loop.
- Removed duplicate initial jobs fetches caused by rerender races around freshly expanded runs.
- Reduced unnecessary polling pressure by tightening selected-repo refresh ownership and job-refresh reuse.
- Refreshed `Cargo.lock` dependencies and regenerated the Flatpak cargo-sources manifest for release packaging.
