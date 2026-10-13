#!/usr/bin/env bash
###############################################################################
# scripts/build_all.sh — Master compilation script for IsoVault
###############################################################################

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Building Rust Engine (isovault_engine) ==="
source "$HOME/.cargo/env" 2>/dev/null || true
(cd "$SCRIPT_DIR/engine" && cargo build --release)

echo "=== Building Native C GTK3 GUI (isovault_gui) ==="
make -C "$SCRIPT_DIR/gui" clean
make -C "$SCRIPT_DIR/gui"

echo "=== Building C Elevation Launcher (IsoVaultGUI) ==="
gcc -O2 -Wall -o "$SCRIPT_DIR/IsoVaultGUI" "$SCRIPT_DIR/launcher/launcher.c"
chmod +x "$SCRIPT_DIR/IsoVaultGUI"
chmod +x "$SCRIPT_DIR/IsoVaultGUI.sh"
chmod +x "$SCRIPT_DIR/IsoVault2Disk.sh"
ln -sf "$SCRIPT_DIR/IsoVault2Disk.sh" "$SCRIPT_DIR/isovault"
ln -sf "$SCRIPT_DIR/IsoVaultGUI" "$SCRIPT_DIR/isovault-gui"

echo ""
echo "=== All IsoVault components built successfully! ==="
echo "  - C Launcher:     $SCRIPT_DIR/IsoVaultGUI"
echo "  - Desktop Script: $SCRIPT_DIR/IsoVaultGUI.sh"
echo "  - CLI Tool:       $SCRIPT_DIR/IsoVault2Disk.sh (or ./isovault)"
echo "  - Native GUI:     $SCRIPT_DIR/gui/isovault_gui"
echo "  - Rust Engine:    $SCRIPT_DIR/engine/target/release/isovault_engine"
