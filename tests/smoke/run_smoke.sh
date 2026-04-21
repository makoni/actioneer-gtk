#!/usr/bin/env bash
# Smoke-test harness: launches Actioneer under Xvfb + AT-SPI and verifies the
# accessibility tree against tests/smoke/*.py scripts.
#
# Requires: xvfb, dbus-x11, at-spi2-core, python3-pyatspi (installed in CI;
# on Debian/Ubuntu hosts: apt install xvfb dbus-x11 at-spi2-core python3-pyatspi).
# Assumes ACTIONEER_BIN points to a built actioneer binary; defaults to
# target/release/actioneer relative to the repo root.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

bin_path="${ACTIONEER_BIN:-$repo_root/target/release/actioneer}"
if [ ! -x "$bin_path" ]; then
  echo "Actioneer binary not found or not executable: $bin_path" >&2
  echo "Build first with: cargo build --release" >&2
  exit 1
fi

script="${1:-$repo_root/tests/smoke/welcome_screen.py}"
if [ ! -f "$script" ]; then
  echo "Smoke script not found: $script" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

if ! python3 -c "import pyatspi" >/dev/null 2>&1; then
  echo "python3-pyatspi module is required" >&2
  exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

export XDG_DATA_HOME="$workdir/data"
export XDG_STATE_HOME="$workdir/state"
export XDG_CONFIG_HOME="$workdir/config"
export XDG_CACHE_HOME="$workdir/cache"
export XDG_RUNTIME_DIR="$workdir/runtime"
mkdir -p "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CONFIG_HOME" \
  "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Enable AT-SPI accessibility bridge so pyatspi can inspect GTK widgets.
export NO_AT_BRIDGE=0
export GTK_A11Y=atspi

app_log="$workdir/app.log"

xvfb_display=":${XVFB_DISPLAY_NUM:-98}"
Xvfb "$xvfb_display" -screen 0 1280x1024x24 >"$workdir/xvfb.log" 2>&1 &
xvfb_pid=$!
sleep 1
export DISPLAY="$xvfb_display"

cleanup() {
  kill "${app_pid:-0}" 2>/dev/null || true
  kill "$xvfb_pid" 2>/dev/null || true
  sleep 0.5
  kill -9 "${app_pid:-0}" 2>/dev/null || true
  kill -9 "$xvfb_pid" 2>/dev/null || true
  wait "${app_pid:-0}" 2>/dev/null || true
  wait "$xvfb_pid" 2>/dev/null || true
}
trap 'cleanup; rm -rf "$workdir"' EXIT

# Launch the app; pyatspi will poll the accessibility tree.
"$bin_path" >"$app_log" 2>&1 &
app_pid=$!
export APP_PID="$app_pid"

set +e
python3 "$script"
status=$?
set -e

if [ "$status" -ne 0 ]; then
  echo "----- actioneer stdout/stderr -----" >&2
  cat "$app_log" >&2 || true
  echo "----- xvfb log -----" >&2
  cat "$workdir/xvfb.log" >&2 || true
fi

exit "$status"
