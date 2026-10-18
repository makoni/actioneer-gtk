#!/usr/bin/env bash
set -euo pipefail

# render-mobile-screenshots.sh
# Regenerate the phone screenshots listed in data/metainfo.xml.in
# (data/screenshots/en-mobile-{1,2,3}.png): render the demo app at 360x720,
# twice the size, through the `store_screenshots` GTK test, then give each
# the rounded corners and soft shadow the desktop screenshots have.
#
# Needs xvfb-run, dbus-run-session and ImageMagick (`magick`). The Yaru icons
# and the Ubuntu Sans font are used when installed, as in the desktop shots.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$REPO_ROOT/data/screenshots"
RAW_DIR="$(mktemp -d)"
trap 'rm -rf "$RAW_DIR"' EXIT

cd "$REPO_ROOT"
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 GSK_RENDERER=cairo ACTIONEER_STORE_SHOTS="$RAW_DIR" \
  xvfb-run -a -s "-screen 0 1280x1024x24" dbus-run-session -- \
  cargo test --lib store_screenshots -- --ignored --test-threads=1

# Window corners: GNOME's 15 px radius, doubled with the rest.
RADIUS=30
# Room for the shadow around the window, as in the desktop screenshots.
MARGIN=48

for raw in "$RAW_DIR"/en-mobile-*.png; do
  name="$(basename "$raw")"
  read -r width height < <(magick identify -format "%w %h\n" "$raw")
  magick "$raw" \
    \( -size "${width}x${height}" xc:none -fill white \
       -draw "roundrectangle 0,0 $((width - 1)),$((height - 1)) $RADIUS,$RADIUS" \) \
    -compose DstIn -composite \
    \( +clone -background black -shadow 30x16+0+6 \) +swap \
    -background none -compose Over -layers merge +repage \
    -gravity center -extent "$((width + 2 * MARGIN))x$((height + 2 * MARGIN))" \
    "$OUT_DIR/$name"
  echo "$OUT_DIR/$name: $(magick identify -format "%wx%h" "$OUT_DIR/$name")"
done
