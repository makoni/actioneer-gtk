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

# Workflows of MAIN_REPO, as shown in the detail pane.
MAIN_WORKFLOWS = ["CI", "Release", "AppImage CI"]

# The main window's title.
MAIN_FRAME_TITLE = "Actioneer"

# Window GActions published on the frame node.
ACTION_ABOUT = "win.about"
ACTION_SIGN_OUT = "win.sign_out"
