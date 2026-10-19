#!/usr/bin/env bash
# Build a self-contained AppImage with quick-sharun (pkgforge-dev/Anylinux-AppImages).
#
# Unlike a linuxdeploy AppImage, this one bundles glibc and the dynamic loader
# along with GTK and libadwaita, so it does not need the host's glibc to be at
# least as new as the build machine's: it starts on old LTS releases and on
# musl distributions alike.
#
# quick-sharun deploys the AppDir; AppImage's own appimagetool packs it into a
# SquashFS image with the standard type2 runtime. quick-sharun's packer makes
# DwarFS images, which the AppImage catalog cannot mount and AppImageLauncher
# and appimaged cannot integrate.
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
APPIMAGETOOL_VERSION=1.9.1
TYPE2_RUNTIME_VERSION=20251108

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)
        APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
        RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
        ;;
    aarch64)
        APPIMAGETOOL_SHA256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
        RUNTIME_SHA256=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
        ;;
    *)
        echo "No pinned appimagetool for $ARCH" >&2
        exit 1
        ;;
esac
VERSION=$(awk -F '"' '/^\[package\]/{p=1} p && /^version/ {print $2; exit}' Cargo.toml)
if [ -z "$VERSION" ]; then
    echo "Could not determine version from Cargo.toml" >&2
    exit 1
fi

tools=$(mktemp -d)
fetch_pinned() {
    local url=$1 sha=$2 dest=$3
    curl -fsSL --retry 5 "$url" -o "$dest"
    echo "$sha  $dest" | sha256sum --check --strict
    chmod +x "$dest"
}

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
    appstream base-devel curl gettext git gtk4 libadwaita patchelf pkgconf rust strace wget \
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

# What AppImage tools look for under usr/ (sharun lays the AppDir out without
# it): appimagetool and the AppImage catalog read the AppStream data from
# usr/share/metainfo, the catalog only as *appdata.xml, and validate it against
# the desktop entry and icon beside it. The catalog also wants a PNG .DirIcon.
install -Dm644 data/metainfo.xml "$APPDIR/usr/share/metainfo/me.spaceinbox.actioneer.appdata.xml"
install -Dm644 "$DESKTOP" "$APPDIR/usr/share/applications/me.spaceinbox.actioneer.desktop"
ln -sfn ../../share/icons "$APPDIR/usr/share/icons"
install -m644 icons/icons/hicolor/256x256/apps/me.spaceinbox.actioneer.png "$APPDIR/.DirIcon"

echo "Making the AppImage..."
fetch_pinned \
    "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-${ARCH}.AppImage" \
    "$APPIMAGETOOL_SHA256" "$tools/appimagetool"
# Passed explicitly: appimagetool would otherwise fetch the runtime's
# continuous build.
fetch_pinned \
    "https://github.com/AppImage/type2-runtime/releases/download/${TYPE2_RUNTIME_VERSION}/runtime-${ARCH}" \
    "$RUNTIME_SHA256" "$tools/runtime"
mkdir -p "$OUTPATH"
# The container has no FUSE. appimagetool writes the .zsync beside the output,
# naming the AppImage's basename in its URL header.
(
    cd "$OUTPATH"
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$tools/appimagetool" \
        --runtime-file "$tools/runtime" \
        --updateinformation "$UPINFO" \
        "$APPDIR" "$OUTNAME"
)
ls -l "$OUTPATH"
