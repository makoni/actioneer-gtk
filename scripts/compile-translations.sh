#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v msgfmt >/dev/null 2>&1; then
  echo "msgfmt is required (install gettext package)." >&2
  exit 1
fi

while read -r lang; do
  [ -z "$lang" ] && continue
  out_dir="po/locale/$lang/LC_MESSAGES"
  mkdir -p "$out_dir"
  msgfmt "po/$lang.po" -o "$out_dir/actioneer.mo"
  echo "Compiled $lang"
done < po/LINGUAS

# Render AppStream metainfo.xml from metainfo.xml.in + po translations.
msgfmt --xml -L MetaInfo --template=data/metainfo.xml.in -d po -o data/metainfo.xml
echo "Rendered data/metainfo.xml"
