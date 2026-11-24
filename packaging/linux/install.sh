#!/usr/bin/env bash
# Per-user install of Kapix on Linux (no sudo required).
# Usage: ./packaging/linux/install.sh [--rebuild] | --uninstall
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"
# Kapix pin windows report app-id `kapix.pin`; panels and docks look icons up by app-id.
WINDOW_APP_IDS="kapix.pin"

refresh_caches() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" >/dev/null 2>&1 || true
    fi
}

case "${1:-}" in
    --help|-h)
        echo "Usage: $(basename "$0") [--rebuild] | --uninstall"
        echo ""
        echo "Installs Kapix per-user into ~/.local/bin and ~/.local/share (no sudo)."
        echo "  --rebuild    force a fresh release build before installing"
        echo "  --uninstall  remove the installed binary, desktop entry, and icons"
        exit 0
        ;;
    --uninstall)
        rm -f "$BIN_DIR/kapix" "$APP_DIR/kapix.desktop" "$ICON_DIR/kapix.svg"
        for id in $WINDOW_APP_IDS; do
            rm -f "$ICON_DIR/$id.svg"
        done
        refresh_caches
        echo "Kapix removed. Your config (~/.config/kapix) and screenshots were left untouched."
        exit 0
        ;;
    ""|--rebuild)
        ;;
    *)
        echo "Unknown option: $1 (try --help)" >&2
        exit 2
        ;;
esac

if [ -f "$ROOT_DIR/Cargo.toml" ]; then
    BIN_SRC="$ROOT_DIR/target/release/kapix"
    if [ ! -x "$BIN_SRC" ] || [ "${1:-}" = "--rebuild" ]; then
        echo "Building kapix (release)..."
        cargo build --release -p kapix-app --locked --manifest-path "$ROOT_DIR/Cargo.toml"
    fi
else
    BIN_SRC="$ROOT_DIR/kapix"
    if [ "${1:-}" = "--rebuild" ]; then
        echo "--rebuild requires a source checkout with Cargo.toml." >&2
        exit 2
    fi
    if [ ! -x "$BIN_SRC" ]; then
        echo "Prebuilt binary not found at $BIN_SRC" >&2
        exit 1
    fi
fi

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"
install -Dm755 "$BIN_SRC" "$BIN_DIR/kapix"
install -Dm644 "$ROOT_DIR/packaging/linux/kapix.desktop" "$APP_DIR/kapix.desktop"
install -Dm644 "$ROOT_DIR/assets/icon.svg" "$ICON_DIR/kapix.svg"
for id in $WINDOW_APP_IDS; do
    install -Dm644 "$ROOT_DIR/assets/icon.svg" "$ICON_DIR/$id.svg"
done
refresh_caches

echo "Installed:"
echo "  binary:  $BIN_DIR/kapix"
echo "  desktop: $APP_DIR/kapix.desktop"
echo "  icon:    $ICON_DIR/kapix.svg"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "  note:    $BIN_DIR is not on your PATH" ;;
esac

echo ""
echo "Optional runtime helpers:"
for pair in "wl-copy:wl-clipboard (clipboard copy)" "tesseract:tesseract (offline OCR)" "notify-send:libnotify (desktop notifications)"; do
    bin="${pair%%:*}"
    desc="${pair#*:}"
    if command -v "$bin" >/dev/null 2>&1; then
        echo "  [ok]      $desc"
    else
        echo "  [missing] $desc"
    fi
done

echo ""
echo "Niri window rules and keybinds (paste into ~/.config/niri/config.kdl):"
echo "  $ROOT_DIR/packaging/niri/kapix.kdl"
