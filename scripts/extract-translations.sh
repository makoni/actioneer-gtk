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

lib_pot=po/actioneer-lib.pot
bin_pot=po/actioneer-bin.pot
meta_pot=po/actioneer-metainfo.pot

# `xtr` walks the module tree from the crate root it is given, so both crate
# roots have to be scanned: `src/lib.rs` owns every module, and `src/main.rs`
# has its own strings (the CLI option help texts). Scanning only `main.rs` — as
# this script did while the crate was binary-only — now yields eight strings
# instead of several hundred.
xtr \
  --default-domain=actioneer \
  --keywords=tr \
  --output="$lib_pot" \
  src/lib.rs

xtr \
  --default-domain=actioneer \
  --keywords=tr \
  --output="$bin_pot" \
  src/main.rs
echo "Extracted Rust strings"

xgettext \
  --from-code=UTF-8 \
  --its=/usr/share/gettext/its/metainfo.its \
  --omit-header \
  --output="$meta_pot" \
  data/metainfo.xml.in
echo "Extracted metainfo strings"

msgcat --use-first "$lib_pot" "$bin_pot" "$meta_pot" -o po/actioneer.pot
rm -f "$lib_pot" "$bin_pot" "$meta_pot"

echo "Updated po/actioneer.pot"
