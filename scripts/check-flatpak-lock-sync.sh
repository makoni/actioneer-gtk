#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${1:-origin/main}"

if ! git rev-parse --verify "${BASE_REF}" >/dev/null 2>&1; then
  if git show-ref --verify --quiet refs/heads/main; then
    BASE_REF="main"
  elif git rev-parse HEAD^ >/dev/null 2>&1; then
    BASE_REF="HEAD^"
  else
    BASE_REF="HEAD"
  fi
fi

MERGE_BASE=$(git merge-base "${BASE_REF}" HEAD 2>/dev/null || echo "${BASE_REF}")

lock_changed=$(git diff --name-only "${MERGE_BASE}"..HEAD -- Cargo.lock | wc -l | tr -d ' ')
flatpak_changed=$(git diff --name-only "${MERGE_BASE}"..HEAD -- flatpak/me.spaceinbox.actioneer.cargo-sources.json | wc -l | tr -d ' ')

if [[ "${lock_changed}" -gt 0 && "${flatpak_changed}" -eq 0 ]]; then
  echo "error: Cargo.lock changed without regenerating flatpak/me.spaceinbox.actioneer.cargo-sources.json" >&2
  exit 1
fi

if [[ "${flatpak_changed}" -gt 0 && "${lock_changed}" -eq 0 ]]; then
  echo "error: flatpak/me.spaceinbox.actioneer.cargo-sources.json changed without updating Cargo.lock" >&2
  exit 1
fi

echo "Cargo.lock and flatpak manifest are in sync."
