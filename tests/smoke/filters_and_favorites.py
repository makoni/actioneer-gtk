#!/usr/bin/env python3
"""Journey: the status filters actually filter, and favouriting is visible.

Covers `detail_view/run_filters.rs`, `helpers/runs/filters.rs` and
`utils/favorites.rs` — all of which Phase 3 partly moves into `domain/` and
Phase 7.2 relocates. A filter that silently stops filtering is exactly the kind
of regression the compiler cannot see.
"""
import sys
import time

import pyatspi

import fixtures
from lib import (
    click,
    find_frame,
    find_named,
    require_named,
    select_row_named,
    visible_names,
    wait_until,
)


def is_pressed(node):
    """GTK4 exposes a toggled `GtkToggleButton` as PRESSED, not CHECKED."""
    return node.getState().contains(pyatspi.STATE_PRESSED)


def first_favorite(frame):
    """The sidebar renders one favourite toggle per repository row."""
    return require_named(frame, fixtures.CONTROL_TOGGLE_FAVORITE)


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

    # ---- favourite toggles state -----------------------------------------
    favorite = require_named(frame, fixtures.CONTROL_TOGGLE_FAVORITE)
    before = is_pressed(favorite)
    click(favorite)
    wait_until(
        lambda: is_pressed(first_favorite(frame)) != before,
        f"the favourite toggle never changed state (was pressed={before})",
    )
    print(f"Favourite toggled: {before} -> {not before}", flush=True)

    # Put it back so the journey leaves no persisted state behind.
    click(require_named(frame, fixtures.CONTROL_TOGGLE_FAVORITE))
    wait_until(
        lambda: is_pressed(first_favorite(frame)) == before,
        "the favourite toggle did not return to its original state",
    )

    # ---- un-pressing a status chip drops those runs from the list ---------
    # The chips are inclusive and start pressed; clicking one hides that state.
    require_named(frame, fixtures.RUNS_SUMMARY_UNFILTERED)
    chip = require_named(frame, fixtures.FILTER_FAILED)
    if not is_pressed(chip):
        raise AssertionError("the failed-runs chip should start pressed (all runs shown)")

    click(chip)
    wait_until(
        lambda: find_named(frame, fixtures.RUNS_SUMMARY_WITHOUT_FAILED) is not None,
        f"the run summary never became {fixtures.RUNS_SUMMARY_WITHOUT_FAILED!r} after "
        f"hiding failed runs. Summaries seen: "
        f"{[n for n in visible_names(frame) if n.startswith('Showing')]}",
    )
    print("Failed-runs chip narrowed the run list (8 -> 6).", flush=True)

    # ---- pressing it again restores the full run list ---------------------
    click(require_named(frame, fixtures.FILTER_FAILED))
    wait_until(
        lambda: find_named(frame, fixtures.RUNS_SUMMARY_UNFILTERED) is not None,
        "re-pressing the chip did not restore the full run list",
    )

    print("Filters-and-favourites journey passed.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as exc:
        print(f"SMOKE FAILED: {exc}", file=sys.stderr, flush=True)
        sys.exit(1)
