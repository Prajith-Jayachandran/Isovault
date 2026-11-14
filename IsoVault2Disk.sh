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
    echo "  -t           Test disk in sandboxed Live VM (QEMU/KVM with snapshot=on)"
    echo "  --pack-drivers Package drivers in /isovault/drivers/net into drivers.cpio"
    echo "  --create-persistence Create persistence .dat image for live Linux ISOs"
    echo "  --verify-iso Scan and compute SHA-256 integrity checksums of stored ISOs"
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
    echo "  sudo ./IsoVault2Disk.sh --create-persistence /dev/sdb --size 4096 --label casper-rw --iso ubuntu.iso"
    echo "  sudo ./IsoVault2Disk.sh --verify-iso /dev/sdb"
    echo "  ./IsoVault2Disk.sh -l"
    exit "${1:-0}"
}

MODE=""
STYLE="mbr"
FS="exfat"
RESERVE=0
SHOW_ALL=0
DISK=""
PERSIST_SIZE="2048"
PERSIST_LABEL="casper-rw"
PERSIST_ISO=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -i) MODE="install"; shift ;;
        -I) MODE="install"; shift ;;
        -u) MODE="update"; shift ;;
        -t|--test-vm) MODE="test-vm"; shift ;;
        --pack-drivers) MODE="pack-drivers"; shift ;;
        --create-persistence|-P) MODE="create-persistence"; shift ;;
        --verify-iso) MODE="verify-iso"; shift ;;
        --size) PERSIST_SIZE="$2"; shift 2 ;;
        --label) PERSIST_LABEL="$2"; shift 2 ;;
        --iso) PERSIST_ISO="$2"; shift 2 ;;
        -l) MODE="list"; shift ;;
        -g) STYLE="gpt"; shift ;;
        -f) FS="$2"; shift 2 ;;
        -r) RESERVE="$2"; shift 2 ;;
        --all) SHOW_ALL=1; shift ;;
        --deps) MODE="deps"; shift ;;
        -h|--help) usage 0 ;;
        -*) echo "Unknown option: $1"; usage 1 ;;
        *) DISK="$1"; shift ;;
    esac
done

if [[ -z "$MODE" ]]; then
    usage 1
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

if [[ "$MODE" == "test-vm" ]]; then
    if ! command -v qemu-system-x86_64 &>/dev/null; then
        echo "Error: qemu-system-x86_64 not found. Please install QEMU."
        exit 1
    fi
    KVM_ARGS=()
    CPU_ARGS=("-cpu" "qemu64")
    if [[ -w /dev/kvm ]]; then
        KVM_ARGS=("-enable-kvm")
        CPU_ARGS=("-cpu" "host")
    fi
    OVMF=""
    for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/x64/OVMF.4m.fd /usr/share/edk2-ovmf/x64/OVMF_CODE.fd /usr/share/ovmf/x64/OVMF.fd /usr/share/OVMF/OVMF_CODE.fd /usr/share/ovmf/OVMF.fd; do
        if [[ -f "$p" ]]; then
            OVMF="$p"
            break
        fi
    done
    PFLASH_ARGS=()
    if [[ -n "$OVMF" ]]; then
        PFLASH_ARGS=(-drive "if=pflash,format=raw,readonly=on,file=$OVMF")
    fi
    echo "Launching IsoVault Live VM Preview for $DISK in sandboxed mode (snapshot=on)..."
    exec qemu-system-x86_64 "${KVM_ARGS[@]}" "${CPU_ARGS[@]}" -m 4096 -smp 4 \
        "${PFLASH_ARGS[@]}" \
        -drive "file=$DISK,format=raw,cache=none,if=virtio,snapshot=on" \
        -vga virtio -name "IsoVault Live Preview [$DISK]"
fi

if [[ $EUID -ne 0 ]]; then
    echo "Error: Disk installation and update operations require root privileges."
    echo "Please re-run with sudo: sudo ./IsoVault2Disk.sh $*"
    exit 1
fi

if [[ "$MODE" == "pack-drivers" ]]; then
    part1="${DISK}1"
    if [[ "$DISK" =~ [0-9]$ ]]; then
        part1="${DISK}p1"
    fi
    mnt="/tmp/isovault_pack_drivers_$$"
    mkdir -p "$mnt"
    echo "Mounting $part1 to pack drivers..."
    if mount "$part1" "$mnt"; then
        drv_dir="$mnt/isovault/drivers"
        if [[ -d "$drv_dir/net" ]]; then
            echo "Packaging Linux network modules/firmware into $drv_dir/drivers.cpio..."
            (cd "$drv_dir/net" && find . -mindepth 1 | cpio -o -H newc 2>/dev/null > "$drv_dir/drivers.cpio")
            echo "Packed drivers.cpio ($(stat -c%s "$drv_dir/drivers.cpio" 2>/dev/null || echo 0) bytes)."
        fi
        sync
        umount "$mnt"
        rmdir "$mnt"
        echo "Drivers successfully synchronized on $part1!"
        exit 0
    else
        echo "Failed to mount $part1. Ensure drive is formatted with IsoVault."
        rmdir "$mnt"
        exit 1
    fi
fi

if [[ "$MODE" == "create-persistence" ]]; then
    ARGS=("--device" "$DISK" "--size" "$PERSIST_SIZE" "--label" "$PERSIST_LABEL")
    if [[ -n "$PERSIST_ISO" ]]; then
        ARGS+=("--iso" "$PERSIST_ISO")
    fi
    exec "$ENGINE" create-persistence "${ARGS[@]}"
elif [[ "$MODE" == "verify-iso" ]]; then
    exec "$ENGINE" verify-iso --device "$DISK"
elif [[ "$MODE" == "install" ]]; then
    exec "$ENGINE" install --device "$DISK" --style "$STYLE" --fs "$FS" --reserve "$RESERVE"
elif [[ "$MODE" == "update" ]]; then
    exec "$ENGINE" update --device "$DISK"
fi
