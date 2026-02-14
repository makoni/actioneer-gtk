#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v xgettext >/dev/null 2>&1; then
  echo "xgettext is required (install gettext package)." >&2
  exit 1
fi

mkdir -p po

xgettext \
  --language=Rust \
  --from-code=UTF-8 \
  --keyword=tr \
  --sort-output \
  --output=po/actioneer.pot \
  $(find src -name '*.rs' -print)

echo "Updated po/actioneer.pot"
