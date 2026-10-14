#!/usr/bin/env bash
###############################################################################
# IsoVault2Disk.sh — Command Line Interface for IsoVault (Multiboot USB Creator)
###############################################################################

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
ENGINE="$SCRIPT_DIR/engine/target/release/isovault_engine"

if [[ ! -x "$ENGINE" ]]; then
    echo "Building isovault_engine binary..."
    (cd "$SCRIPT_DIR/engine" && cargo build --release)
fi

usage() {
    echo "Usage: IsoVault2Disk.sh [OPTIONS] <DISK>"
    echo ""
    echo "Options:"
    echo "  -i           Install IsoVault to disk (Prompts if already installed)"
    echo "  -I           Force install IsoVault to disk (Overwrites existing)"
    echo "  -u           Update bootloader on disk without touching user ISOs"
    echo "  -l           List detected storage devices"
    echo "  -g           Use GPT partition style (Default is MBR)"
    echo "  -f <FS>      Filesystem for data partition (exfat or fat32, default: exfat)"
    echo "  -r <MB>      Reserve <MB> space at end of disk (default: 0)"
    echo "  --all        List all devices (including internal SATA/NVMe)"
    echo "  --deps       Check and auto-install missing system dependencies"
    echo ""
    echo "Examples:"
    echo "  sudo ./IsoVault2Disk.sh -i /dev/sdb"
    echo "  sudo ./IsoVault2Disk.sh -i -g /dev/sdb"
    echo "  sudo ./IsoVault2Disk.sh -u /dev/sdb"
    echo "  ./IsoVault2Disk.sh -l"
    exit 1
}

MODE=""
STYLE="mbr"
FS="exfat"
RESERVE=0
SHOW_ALL=0
DISK=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -i) MODE="install"; shift ;;
        -I) MODE="install"; shift ;;
        -u) MODE="update"; shift ;;
        -l) MODE="list"; shift ;;
        -g) STYLE="gpt"; shift ;;
        -f) FS="$2"; shift 2 ;;
        -r) RESERVE="$2"; shift 2 ;;
        --all) SHOW_ALL=1; shift ;;
        --deps) MODE="deps"; shift ;;
        -h|--help) usage ;;
        -*) echo "Unknown option: $1"; usage ;;
        *) DISK="$1"; shift ;;
    esac
done

if [[ -z "$MODE" ]]; then
    usage
fi

if [[ "$MODE" == "list" ]]; then
    if (( SHOW_ALL )); then
        exec "$ENGINE" list --all
    else
        exec "$ENGINE" list
    fi
fi

if [[ "$MODE" == "deps" ]]; then
    if [[ $EUID -ne 0 ]]; then
        echo "Root privileges required to install dependencies."
        exec sudo "$ENGINE" check-deps --install
    fi
    exec "$ENGINE" check-deps --install
fi

if [[ -z "$DISK" ]]; then
    echo "Error: Target disk must be specified."
    usage
fi

if [[ $EUID -ne 0 ]]; then
    echo "Error: Disk installation and update operations require root privileges."
    echo "Please re-run with sudo: sudo ./IsoVault2Disk.sh $*"
    exit 1
fi

if [[ "$MODE" == "install" ]]; then
    exec "$ENGINE" install --device "$DISK" --style "$STYLE" --fs "$FS" --reserve "$RESERVE"
elif [[ "$MODE" == "update" ]]; then
    exec "$ENGINE" update --device "$DISK"
fi
