#!/usr/bin/env bash
set -euo pipefail

DEFAULT_REMOTE_REF="$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)"
if [[ -n "${DEFAULT_REMOTE_REF}" ]]; then
  DEFAULT_REMOTE_REF="origin/${DEFAULT_REMOTE_REF#origin/}"
fi

BASE_REF="${1:-${DEFAULT_REMOTE_REF:-origin/develop}}"

if ! git rev-parse --verify "${BASE_REF}" >/dev/null 2>&1; then
  if git rev-parse --verify origin/develop >/dev/null 2>&1; then
    BASE_REF="origin/develop"
  elif git show-ref --verify --quiet refs/heads/develop; then
    BASE_REF="develop"
  elif git show-ref --verify --quiet refs/heads/main; then
    BASE_REF="main"
  elif git rev-parse HEAD^ >/dev/null 2>&1; then
    BASE_REF="HEAD^"
  else
    BASE_REF="HEAD"
  fi
fi

MERGE_BASE=$(git merge-base "${BASE_REF}" HEAD 2>/dev/null || echo "${BASE_REF}")

# Primary check: if flatpak-cargo-generator is available, regenerate to a
# temp file and compare content. This is robust to version-only bumps of our
# own package (where Cargo.lock has an `actioneer` line change but the
# external-dep graph is unchanged, so cargo-sources.json wouldn't differ).
GENERATOR="${FLATPAK_CARGO_GENERATOR:-$(command -v flatpak-cargo-generator 2>/dev/null || true)}"
if [[ -z "${GENERATOR}" && -x "$HOME/.local/bin/flatpak-cargo-generator" ]]; then
  GENERATOR="$HOME/.local/bin/flatpak-cargo-generator"
fi

if [[ -n "${GENERATOR}" && -x "${GENERATOR}" ]]; then
  tmp=$(mktemp)
  trap 'rm -f "$tmp"' EXIT
  if "${GENERATOR}" -d Cargo.lock -o "$tmp" >/dev/null 2>&1; then
    if diff -q "$tmp" flatpak/me.spaceinbox.actioneer.cargo-sources.json >/dev/null 2>&1; then
      echo "Cargo.lock and flatpak manifest are in sync (content verified)."
      exit 0
    fi
    echo "error: flatpak/me.spaceinbox.actioneer.cargo-sources.json differs from what would be generated from the current Cargo.lock" >&2
    echo "Run scripts/regenerate-flatpak-sources.sh to refresh it." >&2
    exit 1
  fi
  echo "warning: flatpak-cargo-generator invocation failed; falling back to git-diff heuristic." >&2
fi

# Fallback: coarse git-diff heuristic (runs when the generator is not
# available, e.g. minimal CI environments). False-positives on version-only
# bumps; the generator-based check above is preferred.
lock_changed=$(git diff --name-only "${MERGE_BASE}" -- Cargo.lock | wc -l | tr -d ' ')
flatpak_changed=$(git diff --name-only "${MERGE_BASE}" -- flatpak/me.spaceinbox.actioneer.cargo-sources.json | wc -l | tr -d ' ')

if [[ "${lock_changed}" -gt 0 && "${flatpak_changed}" -eq 0 ]]; then
  echo "error: Cargo.lock changed without regenerating flatpak/me.spaceinbox.actioneer.cargo-sources.json" >&2
  echo "(heuristic check; install flatpak-cargo-generator for a precise content-based check)" >&2
  exit 1
fi

if [[ "${flatpak_changed}" -gt 0 && "${lock_changed}" -eq 0 ]]; then
  echo "notice: flatpak sources updated without Cargo.lock changes; ensure they were regenerated from the current lockfile." >&2
fi

echo "Cargo.lock and flatpak manifest are in sync (git-diff heuristic)."
