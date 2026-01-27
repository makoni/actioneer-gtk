# Using claw-screenshot for Copilot UI analysis

This document describes how to use the local `claw-screenshot` CLI (provided by the Makoni Claw toolset) to capture screenshots of the running Actioneer GTK UI so Copilot or other automated analysis tools can inspect the UI during development.

Goal

- Provide a repeatable, simple interface Copilot can call to request a screenshot via the desktop portal and receive the saved file path for downstream processing.

Requirements

- `claw-screenshot` binary installed at `~/.local/bin/claw-screenshot` (or available on PATH).
- Running desktop session with xdg-desktop-portal available (Wayland/X11). The first interactive use may show a permission dialog.
- `~/clawd/screenshots` directory writable by the user (binary will create it if missing).

Behavior and contract

- Invocation: `claw-screenshot`
- On success the CLI writes one line to stdout of the exact form:

  SAVED:/home/<user>/clawd/screenshots/<filename>.png

  The saved file is a copy of the portal-provided image (PNG) placed into `~/clawd/screenshots`.

- On failure the CLI returns non-zero exit code and prints diagnostic messages to stderr. The helper script expects stdout first line to match `SAVED:…` to consider the capture successful.

Integration points for Copilot

1) Direct invocation from tests or analysis scripts

- Simple synchronous call (shell):

  out=$(~/.local/bin/claw-screenshot 2>/tmp/claw-screenshot.log | sed -n '1p')
  if [[ "$out" == SAVED:* ]]; then
    path=${out#SAVED:}
    echo "Screenshot saved: $path"
    # pass $path to analysis pipeline (OCR, image-diff, UI-element detection)
  else
    echo "Screenshot failed; see /tmp/claw-screenshot.log"
  fi

2) Use in Copilot-driven UI exploration

- Copilot can call the binary whenever it wants a fresh UI capture; the saved filename is typically the portal's original filename (e.g. `Screenshot-8.png`).
- Recommend Copilot store or copy the screenshot into the workspace for long-term analysis (the claw screenshots folder is ephemeral, but stable).

3) Permission considerations

- The first run may prompt the user to allow portal access (a GNOME/xdg-desktop-portal permission dialog). For automated CI/VM runs, ensure the portal is running and permissions are already granted.

4) Recommended wrapping helper (optional)

- If Copilot needs JSON output or richer metadata, wrap `claw-screenshot` with a tiny script that prints JSON:

```bash
#!/usr/bin/env bash
out=$(~/.local/bin/claw-screenshot 2>/dev/stderr | sed -n '1p')
if [[ "$out" == SAVED:* ]]; then
  path=${out#SAVED:}
  echo "{ \"status\": \"ok\", \"path\": \"$path\" }"
  exit 0
else
  echo "{ \"status\": \"error\", \"message\": $(sed -n '1p' /tmp/claw-screenshot.log) }"
  exit 1
fi
```

5) Example in Python (subprocess)

```python
import subprocess, json
p = subprocess.run(["~/.local/bin/claw-screenshot"], capture_output=True, text=True)
if p.returncode == 0:
    first = p.stdout.splitlines()[0]
    if first.startswith('SAVED:'):
        path = first[len('SAVED:'):]
        # load image for analysis
else:
    raise RuntimeError('claw-screenshot failed: ' + p.stderr)
```

6) Expected file format

- PNG image. Use any standard image library (Pillow, OpenCV) to read it.

7) Debugging

- Logs: `claw-screenshot` prints diagnostics to stderr (request object path, code, map debug). Save stderr to a file when debugging.
- If the portal never returns a response, ensure `xdg-desktop-portal` is running in session and that the session is able to display permission dialogs.

Files added/modified by Copilot

- No mandatory changes to the Actioneer source tree are required. This doc suggests using the system-installed `claw-screenshot` binary.

Notes for maintainers

- If you prefer the tool inside the project, copy the `claw-screenshot` binary or add a small wrapper in `scripts/` and reference it in CI/test harnesses.
- Consider adding a unit test that calls `claw-screenshot` and verifies the SAVED output format (skipped on CI if portal not available).

---

If you want, I can commit this file into the repo and open a short PR where Copilot's workspace configuration references it.Otherwise, tell me where to place it or how to format it differently.