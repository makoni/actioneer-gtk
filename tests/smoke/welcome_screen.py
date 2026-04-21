#!/usr/bin/env python3
"""Headless smoke check: verifies Actioneer's welcome screen renders."""
import os
import sys
import time

import pyatspi


TARGET_PID = int(os.environ["APP_PID"])
APP_NAME_MATCH = os.environ.get("APP_NAME_MATCH", "actioneer").lower()
TIMEOUT = float(os.environ.get("SMOKE_TIMEOUT", "40"))


def _process_id(node):
    for attribute in ("get_process_id", "getProcessId"):
        accessor = getattr(node, attribute, None)
        if callable(accessor):
            try:
                return int(accessor())
            except Exception:
                pass
    return None


def _walk(node):
    try:
        for index in range(node.childCount):
            child = node.getChildAtIndex(index)
            yield child
            yield from _walk(child)
    except Exception:
        return


def _visible_names(root):
    names = []
    if root.name:
        names.append(root.name)
    for child in _walk(root):
        if child.name:
            names.append(child.name)
    return sorted(set(names))


def _wait_until(predicate, description, timeout=TIMEOUT):
    deadline = time.time() + timeout
    last_error = None
    while time.time() < deadline:
        try:
            value = predicate()
            if value:
                return value
        except Exception as exc:
            last_error = exc
        time.sleep(0.25)
    if last_error is not None:
        raise AssertionError(f"{description}: {last_error}")
    raise AssertionError(description)


def _find_named(root, name):
    if (root.name or "") == name:
        return root
    for child in _walk(root):
        if (child.name or "") == name:
            return child
    return None


def _require_named(root, name):
    node = _find_named(root, name)
    if node is None:
        raise AssertionError(
            f"Could not find '{name}'. Visible names: {_visible_names(root)}"
        )
    return node


def _find_frame():
    desktop = pyatspi.Registry.getDesktop(0)
    for index in range(desktop.childCount):
        app = desktop.getChildAtIndex(index)
        name = (app.name or "").lower()
        if APP_NAME_MATCH not in name:
            continue
        if _process_id(app) != TARGET_PID:
            continue
        for child_index in range(app.childCount):
            candidate = app.getChildAtIndex(child_index)
            if candidate.getRoleName() == "frame":
                return candidate
    return None


def main():
    frame = _wait_until(_find_frame, "Actioneer frame never appeared")
    print(f"Found frame: {frame.name!r}", flush=True)

    _require_named(frame, "Welcome to Actioneer")
    _require_named(frame, "Sign in with GitHub")
    _require_named(frame, "Quit")
    print("Welcome screen smoke test passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
