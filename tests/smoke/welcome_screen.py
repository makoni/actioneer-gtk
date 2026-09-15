#!/usr/bin/env python3
"""Headless smoke check: verifies Actioneer's welcome screen renders."""
import sys

from lib import require_named, wait_until, find_frame


def main():
    frame = wait_until(find_frame, "Actioneer frame never appeared")
    print(f"Found frame: {frame.name!r}", flush=True)

    require_named(frame, "Welcome to Actioneer")
    require_named(frame, "Sign in with GitHub")
    require_named(frame, "Quit")
    print("Welcome screen smoke test passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
