#!/usr/bin/env bash
# Runs every smoke journey with the launch arguments it needs, failing on the
# first red. Called by the refactor plan's GATE.
#
# The script list is explicit on purpose: `lib.py` and `fixtures.py` are shared
# modules, not journeys, and globbing `*.py` would run them as smoke scripts —
# they would exit 0 having asserted nothing, quietly weakening every gate.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
runner="$repo_root/tests/smoke/run_smoke.sh"

# script:launch arguments
scripts=(
  "welcome_screen.py:"
  "demo_mode.py:--demo"
  "repos_to_workflows.py:--demo"
  "runs_and_detail.py:--demo"
  "job_logs.py:--demo"
  "filters_and_favorites.py:--demo"
  "preferences_and_signout.py:--demo"
)

failed=0
for entry in "${scripts[@]}"; do
  script="${entry%%:*}"
  args="${entry#*:}"
  echo "=== smoke: $script ${args:+($args)} ==="
  if ACTIONEER_ARGS="$args" bash "$runner" "$repo_root/tests/smoke/$script"; then
    echo "--- $script: PASS"
  else
    echo "--- $script: FAIL" >&2
    failed=1
    break
  fi
done

exit "$failed"
