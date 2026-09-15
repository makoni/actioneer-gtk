"""Shared helpers for the AT-SPI smoke scripts.

Run by `run_smoke.sh`, which executes each script with `python3 <script>`, so
the script's own directory is on `sys.path` and `import lib` resolves.

The interaction helpers here exist because the obvious approaches silently do
nothing on GTK4:

* `doAction(0)` is not "click". On a `GtkLabel` index 0 is `clipboard.copy`,
  which succeeds and changes nothing — a script that used it would assert
  against a pane that never updated and report a false pass. `click()` looks
  the action up by name.
* `ListView` rows expose no select or activate action at all, and their
  accessible name is empty (the name lives on a child label). Selecting a row
  goes through the enclosing list's `Selection` interface — `select_row()`.
* Menu items behind a `GtkMenuButton` never appear in the accessibility tree.
  Window actions are reachable on the frame node instead — `do_window_action()`.
"""

import os
import time

import pyatspi

TARGET_PID = int(os.environ["APP_PID"])
APP_NAME_MATCH = os.environ.get("APP_NAME_MATCH", "actioneer").lower()
TIMEOUT = float(os.environ.get("SMOKE_TIMEOUT", "40"))


# --------------------------------------------------------------------------
# tree walking
# --------------------------------------------------------------------------

def process_id(node):
    for attribute in ("get_process_id", "getProcessId"):
        accessor = getattr(node, attribute, None)
        if callable(accessor):
            try:
                return int(accessor())
            except Exception:
                pass
    return None


def walk(node):
    try:
        for index in range(node.childCount):
            child = node.getChildAtIndex(index)
            yield child
            yield from walk(child)
    except Exception:
        return


def visible_names(root):
    names = []
    if root.name:
        names.append(root.name)
    for child in walk(root):
        if child.name:
            names.append(child.name)
    return sorted(set(names))


def visible_roles(root):
    roles = []
    for child in walk(root):
        try:
            roles.append(child.getRoleName())
        except Exception:
            pass
    return sorted(set(roles))


def wait_until(predicate, description, timeout=TIMEOUT):
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


def find_named(root, name):
    if (root.name or "") == name:
        return root
    for child in walk(root):
        if (child.name or "") == name:
            return child
    return None


def require_named(root, name):
    node = find_named(root, name)
    if node is None:
        raise AssertionError(
            f"Could not find '{name}'. Visible names: {visible_names(root)}"
        )
    return node


def find_role(root, role_name):
    for child in walk(root):
        try:
            if child.getRoleName() == role_name:
                return child
        except Exception:
            continue
    return None


def require_role(root, role_name):
    """First descendant with this role, or AssertionError listing what is there."""
    node = find_role(root, role_name)
    if node is None:
        raise AssertionError(
            f"No node with role '{role_name}'. Roles present: {visible_roles(root)}"
        )
    return node


def find_all_roles(root, role_name):
    found = []
    for child in walk(root):
        try:
            if child.getRoleName() == role_name:
                found.append(child)
        except Exception:
            continue
    return found


# --------------------------------------------------------------------------
# frames
# --------------------------------------------------------------------------

def find_frames():
    """Every toplevel frame belonging to the app under test.

    The job-logs, preferences and about windows are real toplevels, so they
    arrive as additional siblings here — never assume the first one.
    """
    desktop = pyatspi.Registry.getDesktop(0)
    frames = []
    for index in range(desktop.childCount):
        app = desktop.getChildAtIndex(index)
        name = (app.name or "").lower()
        if APP_NAME_MATCH not in name:
            continue
        if process_id(app) != TARGET_PID:
            continue
        for child_index in range(app.childCount):
            candidate = app.getChildAtIndex(child_index)
            try:
                if candidate.getRoleName() == "frame":
                    frames.append(candidate)
            except Exception:
                continue
    return frames


def find_frame():
    """The app's first frame — the main window."""
    frames = find_frames()
    return frames[0] if frames else None


def find_frame_titled(title):
    for frame in find_frames():
        if (frame.name or "") == title:
            return frame
    return None


def find_other_frame(known_title):
    """A frame whose title differs from `known_title` — i.e. a second window."""
    for frame in find_frames():
        if (frame.name or "") != known_title:
            return frame
    return None


# --------------------------------------------------------------------------
# interaction
# --------------------------------------------------------------------------

CLICK_ACTIONS = ("click", "activate", "toggle", "press")


def _actions(node):
    try:
        action = node.queryAction()
    except Exception:
        return None, []
    names = []
    for index in range(action.nActions):
        try:
            names.append(action.getName(index))
        except Exception:
            names.append("")
    return action, names


def action_names(node):
    return _actions(node)[1]


def click(node):
    """Invoke this node's click-like action, or the first clickable descendant's.

    Never `doAction(0)`: on a label index 0 is `clipboard.copy`, and on the
    outer node of a `GtkMenuButton` there is no action 0 at all (it raises).
    """
    candidates = [node] + list(walk(node))
    for candidate in candidates:
        action, names = _actions(candidate)
        if action is None:
            continue
        for index, name in enumerate(names):
            if name in CLICK_ACTIONS:
                action.doAction(index)
                return candidate
    raise AssertionError(
        f"Nothing clickable under {node.name!r} ({node.getRoleName()}); "
        f"actions seen: {[action_names(c) for c in candidates[:8]]}"
    )


def do_window_action(frame, action_name):
    """Invoke a window GAction (e.g. `win.about`, `win.sign_out`) on the frame.

    Items inside a `GtkMenuButton` popover do not reach the accessibility tree,
    but the window's actions are published on the frame node itself.
    """
    action, names = _actions(frame)
    if action is None:
        raise AssertionError(f"Frame {frame.name!r} exposes no actions")
    for index, name in enumerate(names):
        if name == action_name:
            action.doAction(index)
            return
    raise AssertionError(
        f"Frame {frame.name!r} has no action {action_name!r}; actions: {names}"
    )


def select_row(list_node, index):
    """Select the nth row of a list through the Selection interface.

    `ListView` rows carry only `listitem.scroll-to`, so clicking them does
    nothing; `Selection.selectChild` is what fires `selection-changed`.
    """
    try:
        selection = list_node.querySelection()
    except Exception as exc:
        raise AssertionError(
            f"{list_node.name!r} ({list_node.getRoleName()}) has no Selection "
            f"interface: {exc}"
        ) from exc
    if not selection.selectChild(index):
        raise AssertionError(f"selectChild({index}) was refused by {list_node.name!r}")


def select_row_named(root, name):
    """Select the list row whose subtree carries `name`.

    `ListView` rows have an empty accessible name — the name is on a child
    label — so locate the label, walk up to the `list item`, and select it by
    its index within the enclosing list.
    """
    label = require_named(root, name)
    node = label
    while node is not None:
        try:
            role = node.getRoleName()
        except Exception:
            break
        if role == "list item":
            parent = node.parent
            select_row(parent, node.getIndexInParent())
            return node
        node = node.parent
    raise AssertionError(
        f"{name!r} is not inside a list item; roles present: {visible_roles(root)}"
    )


def text_content(node):
    """Concatenated text of every node exposing the AT-SPI Text interface.

    Log and code views render into a `GtkTextView`, whose content is not an
    accessible *name* — asserting on `visible_names` would miss it entirely.
    """
    chunks = []
    for candidate in [node] + list(walk(node)):
        try:
            text = candidate.queryText()
        except Exception:
            continue
        try:
            value = text.getText(0, -1)
        except Exception:
            continue
        if value:
            chunks.append(value)
    return "\n".join(chunks)
