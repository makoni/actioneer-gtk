#!/usr/bin/env bash
# Build a self-contained AppImage with quick-sharun (pkgforge-dev/Anylinux-AppImages).
#
# Unlike a linuxdeploy AppImage, this one bundles glibc and the dynamic loader
# along with GTK and libadwaita, so it does not need the host's glibc to be at
# least as new as the build machine's: it starts on old LTS releases and on
# musl distributions alike.
#
# Runs as root inside ghcr.io/pkgforge-dev/archlinux (x86_64 or aarch64), from
# the repository root. Writes dist/actioneer-<version>-<arch>.AppImage and its
# .zsync.

set -euo pipefail

# quick-sharun pins the sha256 of everything it downloads itself (sharun,
# appimagetool, ...); pin quick-sharun and get-debloated-pkgs the same way.
ANYLINUX_COMMIT=36ad2459e9cedc970cbd44da404c5547beaf9f3c
QUICK_SHARUN_SHA256=b38ac233f0b56a03bb10565f6b074a43ce016939faee44ae8bb23d6e66c2d553
GET_DEBLOATED_PKGS_SHA256=217ff68aafe085d2d4ab34a50e267805af43736ad8ef880c01ef88e52db8589f

ARCH=$(uname -m)
VERSION=$(awk -F '"' '/^\[package\]/{p=1} p && /^version/ {print $2; exit}' Cargo.toml)
if [ -z "$VERSION" ]; then
    echo "Could not determine version from Cargo.toml" >&2
    exit 1
fi

tools=$(mktemp -d)
fetch_tool() {
    local name=$1 sha=$2
    curl -fsSL --retry 5 \
        "https://raw.githubusercontent.com/pkgforge-dev/Anylinux-AppImages/${ANYLINUX_COMMIT}/useful-tools/${name}.sh" \
        -o "$tools/$name"
    echo "$sha  $tools/$name" | sha256sum --check --strict
    chmod +x "$tools/$name"
}

echo "Installing build dependencies..."
pacman-key --init
pacman -Syy --noconfirm archlinux-keyring
pacman -Syu --needed --noconfirm \
    base-devel curl gettext git gtk4 libadwaita patchelf pkgconf rust strace wget \
    xorg-server-xvfb zsync

# Must run after every `pacman -S`, or pacman puts the full packages back.
# The debloated GTK and Mesa are what keep the AppImage small.
fetch_tool get-debloated-pkgs "$GET_DEBLOATED_PKGS_SHA256"
"$tools/get-debloated-pkgs" --add-common --prefer-nano

echo "Building actioneer $VERSION..."
cargo build --release --locked
bash scripts/compile-translations.sh

echo "Deploying the AppDir..."
fetch_tool quick-sharun "$QUICK_SHARUN_SHA256"
export APPDIR="$PWD/AppDir"
export OUTPATH="$PWD/dist"
export OUTNAME="actioneer-$VERSION-$ARCH.AppImage"
export UPINFO="gh-releases-zsync|makoni|actioneer-gtk|latest|actioneer-*$ARCH.AppImage.zsync"
export DESKTOP="$PWD/data/me.spaceinbox.actioneer.desktop"
export ICON="$PWD/icons/icons/hicolor/scalable/apps/me.spaceinbox.actioneer.svg"
# Strace mode starts the app to see what it dlopens; demo mode needs no token.
export STRACE_FLAGS="--demo"
rm -rf "$APPDIR" "$OUTPATH"
"$tools/quick-sharun" target/release/actioneer

# The app's own data. Outside a Snap or Flatpak the app finds its catalogs
# only through these variables; sharun expands ${SHARUN_DIR} to the AppDir.
install -d "$APPDIR/share/actioneer/po" "$APPDIR/share/metainfo" "$APPDIR/share/licenses/actioneer"
cp po/*.po "$APPDIR/share/actioneer/po/"
for mo in po/locale/*/LC_MESSAGES/actioneer.mo; do
    lang=${mo#po/locale/}
    lang=${lang%%/*}
    install -Dm644 "$mo" "$APPDIR/share/locale/$lang/LC_MESSAGES/actioneer.mo"
done
install -Dm644 data/metainfo.xml "$APPDIR/share/metainfo/me.spaceinbox.actioneer.metainfo.xml"
install -Dm644 LICENSE "$APPDIR/share/licenses/actioneer/LICENSE"
cp -a icons/icons/hicolor "$APPDIR/share/icons/"
# GLib would send notifications through org.gtk.Notifications, which GNOME
# Shell rejects for an app ID with no installed desktop file - and an
# AppImage installs none. The freedesktop protocol needs no desktop file.
cat >> "$APPDIR/.env" <<'EOF'
ACTIONEER_PO_DIR=${SHARUN_DIR}/share/actioneer/po
ACTIONEER_LOCALE_DIR=${SHARUN_DIR}/share/locale
GNOTIFICATION_BACKEND=freedesktop
EOF

echo "Making the AppImage..."
"$tools/quick-sharun" --make-appimage
ls -l "$OUTPATH"
