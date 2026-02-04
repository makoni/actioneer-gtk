#!/usr/bin/env bash
# Regenerate the Flatpak cargo sources manifest from the current Cargo.lock.
# Requires flatpak-cargo-generator (https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo)
# Tool is expected at $HOME/.local/bin/flatpak-cargo-generator unless overridden via FLATPAK_CARGO_GENERATOR.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_FILE="$ROOT/Cargo.lock"
OUTPUT_JSON="$ROOT/flatpak/me.spaceinbox.actioneer.cargo-sources.json"
DEFAULT_GENERATOR="$HOME/.local/bin/flatpak-cargo-generator"
GENERATOR="${FLATPAK_CARGO_GENERATOR:-$(command -v flatpak-cargo-generator 2>/dev/null || true)}"
if [[ -z "${GENERATOR}" ]]; then
  GENERATOR="$DEFAULT_GENERATOR"
fi

if [[ ! -x "$GENERATOR" ]]; then
  echo "flatpak-cargo-generator not found at '$GENERATOR'." >&2
  echo "Install it (e.g. pip install flatpak-cargo-generator) or set FLATPAK_CARGO_GENERATOR to its path." >&2
  exit 1
fi

if [[ ! -f "$LOCK_FILE" ]]; then
  echo "Cargo.lock not found at $LOCK_FILE" >&2
  exit 1
fi

cd "$ROOT"

echo "Generating $OUTPUT_JSON from $LOCK_FILE using $GENERATOR"
"$GENERATOR" -d "$LOCK_FILE" -o "$OUTPUT_JSON"

# Note: flatpak/vendor/ may need to be refreshed after generation.
# If you regenerate vendored crates, remove any temporary vendor dir you don't intend to commit.

echo "Done. Review and commit $OUTPUT_JSON (and refreshed vendor assets if applicable)."
