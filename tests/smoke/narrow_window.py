#!/usr/bin/env python3
"""Journey: on a phone-sized screen the app fits, and the repository list and
the detail pane become two pages a tap moves between.

`run_all.sh` launches this one on a 360x720 Xvfb screen, the narrowest GNOME
asks adaptive apps to support, in Ubuntu Sans — wider than GTK's default font.
Whatever overflows, libadwaita reports as "<widget> exceeds <container> width"
on the app's output; the journey fails on any such warning.

Translations are not covered here (the names below are the English ones):
the `phone_layout_<language>` GTK tests walk every screen in every catalog. With no window manager the window takes the
screen's width only if its minimum size allows it, so the width check below
is a check on the minimum size too.

The tap goes through XTest (`lib.tap`), not the AT-SPI selection the other
journeys use: in the collapsed layout only a tap navigates — selection also
changes when the app restores the last repository, and that must not leave
the list.
"""
import os
import time

import fixtures
from lib import (
    click,
    find_frame,
    find_named,
    require_named,
    tap,
    visible_names,
    wait_until,
)

PHONE_WIDTH = 360


def main():
    frame = wait_until(find_frame, "Actioneer frame never appeared")
    wait_until(
        lambda: find_named(frame, fixtures.MAIN_REPO),
        "demo repositories never appeared",
    )

    _, _, width, _ = frame.queryComponent().getExtents(0)
    if width > PHONE_WIDTH:
        raise SystemExit(
            f"the window is {width}px wide on a {PHONE_WIDTH}px screen: its minimum "
            "size does not let it fit a phone"
        )
    print(f"Window fits: {width}px.", flush=True)

    # The list page alone: demo mode has already opened a repository, and
    # restoring it must not have navigated away from the list.
    if find_named(frame, fixtures.BACK_BUTTON) is not None:
        raise SystemExit(
            "the detail page is showing before any tap; restoring the open "
            f"repository navigated. Visible: {visible_names(frame)}"
        )

    tap(require_named(frame, fixtures.LAB_REPO))
    wait_until(
        lambda: find_named(frame, fixtures.BACK_BUTTON)
        and find_named(frame, fixtures.HEADER_WORKFLOWS),
        f"tapping {fixtures.LAB_REPO} did not open its detail page. "
        f"Visible: {visible_names(frame)}",
    )
    print("A tap opened the detail page.", flush=True)

    click(require_named(frame, fixtures.BACK_BUTTON))
    wait_until(
        lambda: find_named(frame, fixtures.BACK_BUTTON) is None
        and find_named(frame, fixtures.MAIN_REPO),
        f"the back button did not return to the list. Visible: {visible_names(frame)}",
    )
    print("Back returned to the list.", flush=True)

    # The star toggles the favourite and leaves the list where it is: its
    # button takes the press, so the row never sees a tap.
    tap(require_named(frame, fixtures.CONTROL_TOGGLE_FAVORITE))
    time.sleep(1.5)
    if find_named(frame, fixtures.BACK_BUTTON) is not None:
        raise SystemExit("tapping a repository's star opened the repository")
    print("The star did not navigate.", flush=True)

    # The repository that is already open: selection does not change, the
    # tap alone has to bring its page back.
    tap(require_named(frame, fixtures.LAB_REPO))
    wait_until(
        lambda: find_named(frame, fixtures.BACK_BUTTON),
        "tapping the repository that was already open did not reopen it",
    )

    log_path = os.environ.get("SMOKE_APP_LOG")
    if log_path:
        with open(log_path, encoding="utf-8", errors="replace") as log:
            overflow = [line.strip() for line in log if "exceeds" in line]
        if overflow:
            raise SystemExit(
                "libadwaita reported content wider than its container:\n"
                + "\n".join(sorted(set(overflow))[:10])
            )
    print("Narrow-window journey passed.", flush=True)


if __name__ == "__main__":
    main()
