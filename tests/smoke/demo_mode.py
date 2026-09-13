#!/usr/bin/env python3
"""Headless smoke check: verifies demo mode reaches the repository sidebar.

Launched with ACTIONEER_ARGS=--demo, so the app skips the welcome screen and
populates the sidebar from the demo fixtures. This is the only end-to-end check
that the demo data path still works; the architecture refactor rewires it twice
(Phase 2 replaces the global switch with a gateway, Phase 4 moves the service
construction out of MainWindow), and neither move is visible to the compiler.
"""
import os
import sys
import time

import pyatspi

TARGET_PID = int(os.environ["APP_PID"])
APP_NAME_MATCH = os.environ.get("APP_NAME_MATCH", "actioneer").lower()
TIMEOUT = float(os.environ.get("SMOKE_TIMEOUT", "40"))

# Frozen against src/demo/data.rs — keep in step with tests/common/mod.rs.
DEMO_REPOS = [
    "demo-org/actioneer-demo-app",
    "demo-labs/workflow-lab",
    "demo-team/device-edge",
]


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

    # The sidebar is populated asynchronously once the demo backend answers, so
    # poll for the repository rather than reading the tree once.
    first = DEMO_REPOS[0]
    _wait_until(
        lambda: _find_named(frame, first),
        f"Demo repository {first!r} never appeared in the sidebar. "
        f"Visible names: {_visible_names(frame)}",
    )

    missing = [repo for repo in DEMO_REPOS if _find_named(frame, repo) is None]
    if missing:
        raise AssertionError(
            f"Demo repositories missing from the sidebar: {missing}. "
            f"Visible names: {_visible_names(frame)}"
        )

    print(f"Demo mode smoke test passed ({len(DEMO_REPOS)} repositories).", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
