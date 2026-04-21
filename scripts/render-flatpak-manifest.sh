#!/usr/bin/env bash
# Render flatpak/me.spaceinbox.actioneer.yaml from the .yaml.in template.
#
# Two modes:
#   local   - sources use "type: dir" (used by local dev and CI)
#   flathub - sources use "type: git" + commit (used when syncing the Flathub repo)
#
# The template is the single source of truth; the rendered yaml is not committed.

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: render-flatpak-manifest.sh [options]

Options:
  --mode MODE         local (default) or flathub
  --commit SHA        Git commit (required for --mode flathub)
  --url URL           Git URL (default: https://github.com/makoni/Actioneer-gtk.git)
  --path PATH         Local source path (default: "..") - only for --mode local
  --output FILE       Output path (default: flatpak/me.spaceinbox.actioneer.yaml)
  -h, --help          Show this help
EOF
}

mode="local"
commit=""
url="https://github.com/makoni/Actioneer-gtk.git"
path=".."
output=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode) mode="$2"; shift 2 ;;
        --commit) commit="$2"; shift 2 ;;
        --url) url="$2"; shift 2 ;;
        --path) path="$2"; shift 2 ;;
        --output) output="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
template="$repo_root/flatpak/me.spaceinbox.actioneer.yaml.in"

if [[ -z "$output" ]]; then
    output="$repo_root/flatpak/me.spaceinbox.actioneer.yaml"
fi

if [[ ! -f "$template" ]]; then
    echo "Template not found: $template" >&2
    exit 1
fi

case "$mode" in
    local)
        sources_block=$(cat <<EOF
      - type: dir
        path: $path
      - me.spaceinbox.actioneer.cargo-sources.json
EOF
)
        ;;
    flathub)
        if [[ -z "$commit" ]]; then
            echo "--commit is required for --mode flathub" >&2
            exit 1
        fi
        sources_block=$(cat <<EOF
      - type: git
        url: $url
        commit: $commit
      - me.spaceinbox.actioneer.cargo-sources.json
EOF
)
        ;;
    *)
        echo "Unknown mode: $mode (expected local or flathub)" >&2
        exit 1
        ;;
esac

mkdir -p "$(dirname "$output")"

awk -v block="$sources_block" '
    /^@SOURCES@[[:space:]]*$/ { print block; next }
    { print }
' "$template" > "$output"

echo "Rendered $output (mode=$mode)"
