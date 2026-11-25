#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ACCEL_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kglobalaccel"
BINARY="$BIN_DIR/shoturscreen"
DESKTOP="shoturscreen.desktop"

META_SHIFT_S=301989971
PRINT=16777225

if [ -x ./shoturscreen ]; then
    SOURCE=./shoturscreen
else
    cargo build --release
    SOURCE=target/release/shoturscreen
fi

mkdir -p "$BIN_DIR" "$APP_DIR" "$ACCEL_DIR"
install -m 755 "$SOURCE" "$BINARY"
sed "s|@BINARY@|$BINARY|" packaging/$DESKTOP > "$APP_DIR/$DESKTOP"
ln -sf "$APP_DIR/$DESKTOP" "$ACCEL_DIR/$DESKTOP"
update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true

accel() {
    busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel "$@" >/dev/null
}

SPECTACLE=(4 org.kde.spectacle.desktop _launch Spectacle Spectacle)
SELF=(4 "$DESKTOP" _launch shoturscreen shoturscreen)

if busctl --user status org.kde.kglobalaccel >/dev/null 2>&1; then
    accel setForeignShortcut asai "${SPECTACLE[@]}" 1 "$PRINT" || true
    accel doRegister as "${SELF[@]}"
    accel setShortcut asaiu "${SELF[@]}" 1 "$META_SHIFT_S" 4
    echo "Meta+Shift+S now runs shoturscreen"
else
    echo "kglobalaccel is not running, set the Meta+Shift+S shortcut for shoturscreen in System Settings"
fi

echo "Installed to $BINARY"
