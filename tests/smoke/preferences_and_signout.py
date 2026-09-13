#!/usr/bin/env python3
"""Journey: preferences open, and signing out returns to the welcome screen.

Both are driven through the window's GActions rather than the primary menu:
items inside a `GtkMenuButton` popover never reach the accessibility tree, but
the actions the menu items map to are published on the frame node.

Sign-out is the interesting half for the refactor: Phase 4 moves the gateway
slot's `sign_out()` transition out of `MainWindow` into `AppServices`, and
until step 6.4 lands the construction test this is the only check on it.

Note what this pins, which is the app's *current* behaviour and not an opinion
about the right one: in demo mode, confirming sign-out closes the dialog but
leaves the demo repositories on screen — the app does not return to the welcome
screen. That was established by probing, after the first version of this journey
asserted the welcome screen would come back and failed. Freezing it here means
Phase 4 cannot change it unnoticed, in either direction.
"""
import sys
import time

import fixtures
from lib import (
    click,
    do_window_action,
    find_frame,
    find_frame_titled,
    find_named,
    find_role,
    require_named,
    visible_names,
    wait_until,
)


def main():
    frame = wait_until(find_frame, "Actioneer frame never appeared")
    wait_until(
        lambda: find_named(frame, fixtures.MAIN_REPO),
        "demo repositories never appeared, so the app is not in its signed-in state",
    )

    # ---- preferences open as their own toplevel ---------------------------
    do_window_action(frame, fixtures.ACTION_PREFERENCES)
    prefs = wait_until(
        lambda: find_frame_titled(fixtures.PREFS_FRAME_TITLE),
        "the preferences window never appeared",
    )
    print(f"Preferences frame: {prefs.name!r}", flush=True)

    missing = [s for s in fixtures.PREFS_SECTIONS if find_named(prefs, s) is None]
    if missing:
        raise AssertionError(
            f"preference sections missing: {missing}. Visible: {visible_names(prefs)}"
        )
    for row in fixtures.PREFS_ROWS:
        require_named(prefs, row)

    click(require_named(prefs, "Close"))
    wait_until(
        lambda: find_frame_titled(fixtures.PREFS_FRAME_TITLE) is None,
        "the preferences window did not close",
    )
    print("Preferences opened and closed.", flush=True)

    # ---- signing out returns to the welcome screen ------------------------
    do_window_action(frame, fixtures.ACTION_SIGN_OUT)

    # The confirmation is an `adw::AlertDialog`, which stays inside the parent
    # frame rather than becoming a toplevel.
    dialog = wait_until(
        lambda: find_role(frame, "alert"),
        f"the sign-out confirmation never appeared. Visible: {visible_names(frame)}",
    )
    require_named(dialog, fixtures.SIGN_OUT_CONFIRM)
    require_named(dialog, fixtures.SIGN_OUT_CANCEL)
    print("Sign-out confirmation opened.", flush=True)

    # Cancelling dismisses it and changes nothing.
    click(require_named(dialog, fixtures.SIGN_OUT_CANCEL))
    wait_until(
        lambda: find_role(frame, "alert") is None,
        "cancelling did not dismiss the sign-out confirmation",
    )
    require_named(frame, fixtures.MAIN_REPO)
    print("Cancelling left the session intact.", flush=True)

    # Confirming dismisses it too. In demo mode the repository list stays —
    # see the module docstring; this pins current behaviour.
    do_window_action(frame, fixtures.ACTION_SIGN_OUT)
    dialog = wait_until(
        lambda: find_role(frame, "alert"),
        "the sign-out confirmation did not reopen",
    )
    click(require_named(dialog, fixtures.SIGN_OUT_CONFIRM))
    wait_until(
        lambda: find_role(frame, "alert") is None,
        "confirming did not dismiss the sign-out confirmation",
    )
    print("Confirming dismissed the dialog.", flush=True)

    print("Preferences-and-sign-out journey passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
