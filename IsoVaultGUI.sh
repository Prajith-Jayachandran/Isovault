#!/usr/bin/env bash
###############################################################################
# IsoVaultGUI.sh — Desktop GUI Launcher for IsoVault
###############################################################################

SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
GUI_BIN="$SCRIPT_DIR/gui/isovault_gui"

if [[ ! -x "$GUI_BIN" ]]; then
    echo "Compiling IsoVault GUI binary..."
    make -C "$SCRIPT_DIR/gui"
fi

if [[ $EUID -eq 0 ]]; then
    export GDK_BACKEND="${GDK_BACKEND:-x11}"
    export DISPLAY="${DISPLAY:-:1}"
    exec "$GUI_BIN" "$@"
fi

# Allow local root access to X11/XWayland display automatically
if command -v xhost &>/dev/null; then
    xhost +si:localuser:root >/dev/null 2>&1 || true
fi

# Not root — request graphical privilege elevation with XWayland/X11 backend
if command -v pkexec &>/dev/null; then
    exec pkexec env DISPLAY="${DISPLAY:-:0}" GDK_BACKEND=x11 XAUTHORITY="${XAUTHORITY:-$HOME/.Xauthority}" "$GUI_BIN" "$@"
elif command -v sudo &>/dev/null; then
    exec sudo -E env DISPLAY="${DISPLAY:-:0}" GDK_BACKEND=x11 "$GUI_BIN" "$@"
else
    echo "Error: Root privileges required. Install polkit (pkexec) or sudo."
    exit 1
fi
