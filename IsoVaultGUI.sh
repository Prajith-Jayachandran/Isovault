#!/usr/bin/env bash
###############################################################################
# IsoVaultGUI.sh — Desktop GUI Launcher for IsoVault
###############################################################################

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUI_BIN="$SCRIPT_DIR/gui/isovault_gui"

if [[ ! -x "$GUI_BIN" ]]; then
    echo "Compiling IsoVault GUI binary..."
    make -C "$SCRIPT_DIR/gui"
fi

if [[ $EUID -eq 0 ]]; then
    exec "$GUI_BIN" "$@"
fi

# Not root — request graphical privilege elevation
if command -v pkexec &>/dev/null; then
    exec pkexec env DISPLAY="${DISPLAY:-}" XAUTHORITY="${XAUTHORITY:-}" WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" "$GUI_BIN" "$@"
elif command -v sudo &>/dev/null; then
    exec sudo -E "$GUI_BIN" "$@"
else
    echo "Error: Root privileges required. Install polkit (pkexec) or sudo."
    exit 1
fi
