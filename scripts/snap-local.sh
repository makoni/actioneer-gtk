#!/usr/bin/env bash
set -euo pipefail

snapcraft pack

SNAP_FILE=$(ls -1t actioneer_*.snap | head -n1)

if [[ -z "${SNAP_FILE:-}" ]]; then
	echo "No snap package found after snapcraft pack" >&2
	exit 1
fi

echo "Installing ${SNAP_FILE}"
snap install "${SNAP_FILE}" --devmode --dangerous