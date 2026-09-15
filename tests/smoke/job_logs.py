#!/usr/bin/env python3
"""Journey: opening a run's logs raises the job-logs window with log text.

The job-logs window is a real `adw::Window`, so it arrives as a second frame —
this is the only journey that exercises `find_other_frame`. Phase 5 splits
`job_logs_window.rs` (1300 lines) into a directory module; nothing else would
notice if the window stopped opening.
"""
import sys
import time

import fixtures
from lib import (
    click,
    find_frame,
    find_named,
    find_other_frame,
    require_named,
    select_row_named,
    text_content,
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

    # Open the logs of the first run that offers them.
    view_logs = require_named(frame, fixtures.ACTION_VIEW_LOGS)
    clicked = click(view_logs)
    print(f"Clicked: role={clicked.getRoleName()!r}", flush=True)

    logs_frame = wait_until(
        lambda: find_other_frame(fixtures.MAIN_FRAME_TITLE),
        "the job-logs window never appeared as a second frame",
    )
    print(f"Second frame: {logs_frame.name!r}", flush=True)

    if not logs_frame.name.endswith(fixtures.LOGS_WINDOW_TITLE_SUFFIX):
        raise AssertionError(
            f"unexpected job-logs window title: {logs_frame.name!r}"
        )

    # Give the log view a moment to stream in.
    time.sleep(2)

    # The run's jobs are listed, and the window's controls are present.
    missing_jobs = [j for j in fixtures.MAIN_RUN_JOBS if find_named(logs_frame, j) is None]
    if missing_jobs:
        raise AssertionError(
            f"jobs missing from the job-logs window: {missing_jobs}. "
            f"Visible names: {visible_names(logs_frame)}"
        )
    for control in fixtures.LOGS_WINDOW_CONTROLS:
        require_named(logs_frame, control)

    # The log body lives in a text view, not in an accessible name.
    body = text_content(logs_frame)
    for expected in (
        fixtures.LOG_TIMESTAMP_PREFIX,
        fixtures.LOG_GROUP_MARKER,
        fixtures.LOG_GROUP_TITLE,
        fixtures.LOG_COMMAND_LINE,
    ):
        if expected not in body:
            raise AssertionError(
                f"{expected!r} missing from the log view. "
                f"Text seen: {body[:300]!r}"
            )

    print("Job-logs journey passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
