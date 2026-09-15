#!/usr/bin/env python3
"""Headless smoke check: demo mode populates the repository sidebar.

Launched with ACTIONEER_ARGS=--demo. This is the only end-to-end check that the
demo data path works; the architecture refactor rewires it twice (Phase 2
replaces the global switch with a gateway, Phase 4 moves service construction
out of MainWindow) and neither move is visible to the compiler.
"""
import sys

import fixtures
from lib import find_frame, find_named, visible_names, wait_until


def main():
    frame = wait_until(find_frame, "Actioneer frame never appeared")
    print(f"Found frame: {frame.name!r}", flush=True)

    # The sidebar fills asynchronously once the backend answers, so poll.
    first = fixtures.MAIN_REPO
    wait_until(
        lambda: find_named(frame, first),
        f"Demo repository {first!r} never appeared in the sidebar. "
        f"Visible names: {visible_names(frame)}",
    )

    missing = [r for r in fixtures.DEMO_REPOS if find_named(frame, r) is None]
    if missing:
        raise AssertionError(
            f"Demo repositories missing from the sidebar: {missing}. "
            f"Visible names: {visible_names(frame)}"
        )

    print(f"Demo mode smoke test passed ({len(fixtures.DEMO_REPOS)} repositories).", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
