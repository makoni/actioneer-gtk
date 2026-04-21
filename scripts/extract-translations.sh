#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

for tool in xtr xgettext msgcat; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required. Install with: cargo install xtr (for xtr) or apt install gettext (for xgettext/msgcat)." >&2
    exit 1
  fi
done

mkdir -p po

src_pot=po/actioneer-src.pot
meta_pot=po/actioneer-metainfo.pot

xtr \
  --default-domain=actioneer \
  --keywords=tr \
  --output="$src_pot" \
  src/main.rs
echo "Extracted Rust strings"

xgettext \
  --from-code=UTF-8 \
  --its=/usr/share/gettext/its/metainfo.its \
  --omit-header \
  --output="$meta_pot" \
  data/metainfo.xml.in
echo "Extracted metainfo strings"

msgcat --use-first "$src_pot" "$meta_pot" -o po/actioneer.pot
rm -f "$src_pot" "$meta_pot"

echo "Updated po/actioneer.pot"
