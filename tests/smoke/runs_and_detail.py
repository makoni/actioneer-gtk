#!/usr/bin/env python3
"""Journey: a selected repository shows its runs, their states and their controls.

Second half of the app's core loop. Phase 2 changes where these runs come from
and Phase 5 splits `helpers/runs/*` and `workflow_refresh.rs`, which render
them; the compiler sees neither change.
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

    wait_until(
        lambda: find_named(frame, fixtures.MAIN_REPO),
        f"{fixtures.MAIN_REPO!r} never appeared in the sidebar",
    )
    select_row_named(frame, fixtures.MAIN_REPO)

    wait_until(
        lambda: find_named(frame, fixtures.HEADER_RECENT_RUNS),
        "the RECENT RUNS section never appeared",
    )

    # Every run of the default workflow is listed, newest first.
    missing = [n for n in fixtures.MAIN_RUN_NUMBERS if find_named(frame, n) is None]
    if missing:
        raise AssertionError(
            f"Run numbers missing from the run list: {missing}. "
            f"Visible names: {visible_names(frame)}"
        )
    require_named(frame, fixtures.MAIN_RUNS_SUMMARY)

    # The fixtures cover three distinct run states, and each renders its label.
    for status in (
        fixtures.RUN_STATUS_IN_PROGRESS,
        fixtures.RUN_STATUS_SUCCESS,
        fixtures.RUN_STATUS_FAILED,
    ):
        require_named(frame, status)

    # Controls are state-dependent: a running run can be cancelled, a finished
    # one re-run, and a failed one has the failed-jobs variant.
    require_named(frame, fixtures.ACTION_CANCEL_RUN)
    require_named(frame, fixtures.ACTION_RERUN)
    require_named(frame, fixtures.ACTION_RERUN_FAILED)
    require_named(frame, fixtures.ACTION_OPEN_GITHUB)

    # Branch names reach the run rows.
    missing_branches = [b for b in fixtures.RUN_BRANCHES if find_named(frame, b) is None]
    if missing_branches:
        raise AssertionError(f"Branch labels missing from the run list: {missing_branches}")

    print(
        f"Runs-and-detail journey passed "
        f"({len(fixtures.MAIN_RUN_NUMBERS)} runs, 3 states).",
        flush=True,
    )


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
