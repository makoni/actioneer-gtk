"""Frozen accessibility names the smoke journeys assert on.

Every value here was observed in a live accessibility tree, never invented: the
discovery procedure is to write the assertion with a placeholder, run the
script, read the `Visible names: [...]` list that `require_named` prints on
failure, and copy the real name in.

Keep the repository names in step with `tests/common/mod.rs`.
"""

DEMO_REPOS = [
    "demo-org/actioneer-demo-app",
    "demo-labs/workflow-lab",
    "demo-team/device-edge",
]

MAIN_REPO = DEMO_REPOS[0]
LAB_REPO = DEMO_REPOS[1]
EDGE_REPO = DEMO_REPOS[2]

# Workflows of MAIN_REPO, as shown in the detail pane (all eight).
MAIN_WORKFLOWS = [
    "CI",
    "Release",
    "AppImage CI",
    "Snap CI",
    "Lockfile Sync",
    "Publish Release",
    "Copilot code review",
    "Copilot coding agent",
]

# Workflow file names shown beneath each workflow.
MAIN_WORKFLOW_FILES = ["ci.yml", "release.yml", "appimage.yml", "snap.yml"]

# Section headers in the detail pane.
HEADER_WORKFLOWS = "WORKFLOWS"
HEADER_RECENT_RUNS = "RECENT RUNS"

# The CI workflow has eight runs in the demo fixtures.
MAIN_RUNS_SUMMARY = "Showing 8 of 8"

# Per-run controls.
ACTION_VIEW_LOGS = "View logs"
ACTION_TRIGGER = "Trigger workflow"

# The main window's title.
MAIN_FRAME_TITLE = "Actioneer"

# Window GActions published on the frame node.
ACTION_ABOUT = "win.about"
ACTION_SIGN_OUT = "win.sign_out"

# Run numbers of the CI workflow, newest first.
MAIN_RUN_NUMBERS = ["#134", "#133", "#132", "#131", "#130", "#129", "#128", "#127"]

# Status labels the run list renders.
RUN_STATUS_IN_PROGRESS = "In Progress"
RUN_STATUS_SUCCESS = "Success"
RUN_STATUS_FAILED = "Failed"

# Per-run controls, by run state.
ACTION_CANCEL_RUN = "Cancel run"
ACTION_RERUN = "Re-run workflow"
ACTION_RERUN_FAILED = "Re-run failed jobs"
ACTION_OPEN_GITHUB = "Open in GitHub"

# Branch labels appearing in the run list.
RUN_BRANCHES = ["main", "feature/login", "feature/refactor", "release/hotfix"]

# Jobs of the newest CI run, and the job-logs window controls.
MAIN_RUN_JOBS = ["prepare", "bundle"]
LOGS_WINDOW_CONTROLS = ["Copy logs to clipboard", "Refresh logs", "Save logs to file"]
LOGS_WINDOW_TITLE_SUFFIX = "- Logs"

# What the log view renders. The fixtures carry GitHub's `##[group]` markers;
# `src/ui/ansi.rs` turns them into the disclosure triangle, so asserting on the
# rendered form also proves the log parser ran.
LOG_GROUP_MARKER = "\u25be"  # BLACK DOWN-POINTING SMALL TRIANGLE
LOG_GROUP_TITLE = "Lint"
LOG_COMMAND_LINE = "cargo clippy --all-targets"
LOG_TIMESTAMP_PREFIX = "2026-01-24T11:28:01Z"

# Sidebar and detail-pane filter controls (all `toggle button`).
CONTROL_TOGGLE_FAVORITE = "Toggle favorite"
FILTER_FAILED = "Show failed runs \u00b7 1 workflows in this state"
FILTER_SUCCESS = "Show successful runs \u00b7 4 workflows in this state"
SIDEBAR_TAB_FAVORITES = "Favorites"
SIDEBAR_TAB_ALL = "All"

# The status chips are inclusive and start pressed (everything shown). Clicking
# one un-presses it and drops those runs from the run list, which the summary
# line reports. Filtering acts on runs, not on the workflow list.
RUNS_SUMMARY_UNFILTERED = "Showing 8 of 8"
RUNS_SUMMARY_WITHOUT_FAILED = "Showing 6 of 6 matching filters"
