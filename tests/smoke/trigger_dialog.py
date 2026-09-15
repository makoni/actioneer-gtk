#!/usr/bin/env python3
"""Journey: the "Trigger workflow" dialog opens and is fully rendered.

This exists because of a specific risk. The dialog is built by ~550 lines that
were extracted out of a 1055-line row builder during the architecture refactor,
and the compiler only proves the captures still type-check — it cannot prove the
dialog still populates. Nothing else in the suite opens it.
"""
import sys

import fixtures
from lib import (
    click,
    find_frame,
    find_named,
    find_role,
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
        lambda: find_named(frame, fixtures.HEADER_WORKFLOWS),
        "the WORKFLOWS section never appeared",
    )

    click(require_named(frame, fixtures.ACTION_TRIGGER))

    dialog = wait_until(
        lambda: find_role(frame, "alert"),
        f"the trigger dialog never opened. Visible: {visible_names(frame)}",
    )
    print(f"Dialog opened: {fixtures.TRIGGER_DIALOG_TITLE!r}", flush=True)

    # The dialog's own chrome.
    for name in (
        fixtures.TRIGGER_DIALOG_TITLE,
        fixtures.TRIGGER_DIALOG_BRANCH_LABEL,
        fixtures.TRIGGER_DIALOG_INPUTS_HEADER,
        fixtures.TRIGGER_DIALOG_CONFIRM,
        fixtures.TRIGGER_DIALOG_CANCEL,
    ):
        require_named(dialog, name)

    # The workflow_dispatch inputs the backend advertised, rendered as fields.
    # This is the part the extraction could have silently broken: the inputs are
    # fetched and built inside the moved closure.
    missing = [i for i in fixtures.TRIGGER_DIALOG_INPUTS if find_named(dialog, i) is None]
    if missing:
        raise AssertionError(
            f"dispatch inputs missing from the dialog: {missing}. "
            f"Visible: {visible_names(dialog)}"
        )

    # Leave without triggering anything.
    click(require_named(dialog, fixtures.TRIGGER_DIALOG_CANCEL))
    wait_until(
        lambda: find_role(frame, "alert") is None,
        "cancelling did not dismiss the trigger dialog",
    )

    print(
        f"Trigger-dialog journey passed ({len(fixtures.TRIGGER_DIALOG_INPUTS)} inputs).",
        flush=True,
    )


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
