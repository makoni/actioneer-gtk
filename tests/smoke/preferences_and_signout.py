#!/usr/bin/env python3
"""Journey: preferences open, and signing out returns to the welcome screen.

Both are driven through the window's GActions rather than the primary menu:
items inside a `GtkMenuButton` popover never reach the accessibility tree, but
the actions the menu items map to are published on the frame node.

Sign-out is the interesting half for the refactor: Phase 4 moves the gateway
slot's `sign_out()` transition out of `MainWindow` into `AppServices`, and
until step 6.4 lands the construction test this is the only check on it.

Confirming sign-out must return to the welcome screen and clear the repository
list. The first version of this journey asserted exactly that, found the app
did not do it, and froze the broken behaviour instead — which was the wrong
call. The cause turned out to be environmental rather than a refactor
regression: demo-mode sign-out went through `TokenStorage`, and where that fails
to open (no session keyring, a locked one, a sandboxed harness) the error branch
only logged, leaving demo data on screen. Demo mode now skips the token path
entirely, and a failed deletion no longer swallows the sign-out.
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

    # Confirming signs out: the welcome screen returns and the demo data goes.
    do_window_action(frame, fixtures.ACTION_SIGN_OUT)

    # Wait for a dialog that actually carries its buttons, not merely for an
    # `alert` node: right after the cancel above, the dismissed dialog lingers
    # in the tree for a moment with its children already gone, and grabbing it
    # makes this journey flake.
    dialog = wait_until(
        lambda: next(
            (
                node
                for node in [find_role(frame, "alert")]
                if node is not None
                and find_named(node, fixtures.SIGN_OUT_CONFIRM) is not None
            ),
            None,
        ),
        "the sign-out confirmation did not reopen with its responses",
    )
    click(require_named(dialog, fixtures.SIGN_OUT_CONFIRM))
    wait_until(
        lambda: find_role(frame, "alert") is None,
        "confirming did not dismiss the sign-out confirmation",
    )

    wait_until(
        lambda: find_named(frame, fixtures.WELCOME_HEADING),
        f"the welcome screen did not return after signing out. "
        f"Visible: {visible_names(frame)}",
    )
    require_named(frame, fixtures.WELCOME_SIGN_IN)

    if find_named(frame, fixtures.MAIN_REPO) is not None:
        raise AssertionError("repositories are still listed after signing out")
    print("Signed out: welcome screen returned and the repository list cleared.", flush=True)

    print("Preferences-and-sign-out journey passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
