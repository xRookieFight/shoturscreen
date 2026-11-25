#!/usr/bin/env bash
set -euo pipefail

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ACCEL_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kglobalaccel"
DESKTOP="shoturscreen.desktop"

META_SHIFT_S=301989971
PRINT=16777225

accel() {
    busctl --user call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel "$@" >/dev/null
}

if busctl --user status org.kde.kglobalaccel >/dev/null 2>&1; then
    accel setShortcut asaiu 4 "$DESKTOP" _launch shoturscreen shoturscreen 0 4 || true
    accel unregister ss "$DESKTOP" _launch || true
    accel setForeignShortcut asai 4 org.kde.spectacle.desktop _launch Spectacle Spectacle 2 "$PRINT" "$META_SHIFT_S" || true
fi

rm -f "$BIN_DIR/shoturscreen" "$APP_DIR/$DESKTOP" "$ACCEL_DIR/$DESKTOP"
update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true

echo "shoturscreen removed, Meta+Shift+S is back on Spectacle"
