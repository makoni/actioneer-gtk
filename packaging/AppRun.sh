#!/bin/sh
set -e
APPDIR="$(dirname "$(readlink -f "$0")")"
export GSETTINGS_SCHEMA_DIR="$APPDIR/usr/share/glib-2.0/schemas"
export XDG_DATA_DIRS="$APPDIR/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GI_TYPELIB_PATH="$APPDIR/usr/lib/girepository-1.0:${GI_TYPELIB_PATH:-}"
export GTK_EXE_PREFIX="$APPDIR/usr"
export GTK_DATA_PREFIX="$APPDIR/usr"
case "$(uname -m)" in
  x86_64)
    export LD_LIBRARY_PATH="$APPDIR/usr/lib:$APPDIR/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"
    ;;
  aarch64)
    export LD_LIBRARY_PATH="$APPDIR/usr/lib:$APPDIR/usr/lib/aarch64-linux-gnu:${LD_LIBRARY_PATH:-}"
    ;;
  *)
    export LD_LIBRARY_PATH="$APPDIR/usr/lib:${LD_LIBRARY_PATH:-}"
    ;;
esac
exec "$APPDIR/usr/bin/actioneer" "$@"
