#!/usr/bin/env python3
"""Journey: pick a repository in the sidebar, see its workflows in the detail pane.

This is the first half of the app's core loop. Phase 2 rewires where the
workflow list comes from (gateway instead of the global demo switch) and
Phase 5 splits the files that render it; neither is visible to the compiler.
"""
import sys

import fixtures
from lib import (
    find_frame,
    find_named,
    require_named,
    select_row_named,
    visible_names,
    wait_until,
)


def main():
    frame = wait_until(find_frame, "Actioneer frame never appeared")
    print(f"Found frame: {frame.name!r}", flush=True)

    # Wait for the sidebar to fill, then select the repository under test.
    wait_until(
        lambda: find_named(frame, fixtures.MAIN_REPO),
        f"{fixtures.MAIN_REPO!r} never appeared in the sidebar",
    )
    row = select_row_named(frame, fixtures.MAIN_REPO)
    print(f"Selected row: role={row.getRoleName()!r}", flush=True)

    # The detail pane loads asynchronously; wait for the workflow section.
    wait_until(
        lambda: find_named(frame, fixtures.HEADER_WORKFLOWS),
        "the WORKFLOWS section never appeared after selecting a repository",
    )

    missing = [w for w in fixtures.MAIN_WORKFLOWS if find_named(frame, w) is None]
    if missing:
        raise AssertionError(
            f"Workflows missing from the detail pane: {missing}. "
            f"Visible names: {visible_names(frame)}"
        )

    # The workflow file names are rendered too — that is the row subtitle.
    missing_files = [f for f in fixtures.MAIN_WORKFLOW_FILES if find_named(frame, f) is None]
    if missing_files:
        raise AssertionError(f"Workflow file names missing: {missing_files}")

    # Each workflow row carries its controls.
    require_named(frame, fixtures.ACTION_VIEW_LOGS)
    require_named(frame, fixtures.ACTION_TRIGGER)

    print(
        f"Repos-to-workflows journey passed "
        f"({len(fixtures.MAIN_WORKFLOWS)} workflows).",
        flush=True,
    )


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
