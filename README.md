# IsoVault — Multiboot USB Creator (C + Rust)

**IsoVault** is a high-performance, Linux-native implementation of the **Ventoy** multiboot architecture. It combines a native **C GTK3 Desktop GUI** with a memory-safe **Rust disk engine** to provide 100% functional parity with Ventoy, with zero boot errors.

---

## Key Features

- **True Ventoy Compatibility**: Uses the proven Ventoy bootloader payload (`ventoy.disk.img.xz`) featuring `dmsetup` device-mapper initramfs injection (`ventoy.cpio`), UEFI Shim Secure Boot, and Windows virtual optical drive emulation.
- **Direct ISO Drag-and-Drop**: Format the USB once. Afterwards, simply copy `.iso` files onto the main storage partition using your file manager. No extraction or tool re-running needed.
- **Full-RAM Booting**: Images are dynamically discovered at boot time and booted seamlessly.
- **Native C GTK3 Desktop GUI**: Spawns an instant graphical window.
- **C Polkit Launcher**: Automatically prompts for administrative privileges via `pkexec` or `sudo`.
- **Memory-Safe Rust Engine**: Accurate 64-bit LBA geometry calculations, strict 4KB sector alignment, atomic MBR/GPT partition writing, and in-memory XZ streaming decompression.
- **Non-Destructive Update (`-u`)**: Upgrades the bootloader on the 32MB boot partition without touching or wiping user ISOs.
- **Multi-Distro Dependency Resolver**: Automatic package detection and installation for Arch Linux (`pacman`), Ubuntu/Debian (`apt`), and Fedora (`dnf`).

---

## Directory Structure

```
~/venbootUSB/
├── IsoVaultGUI                # Compiled C launcher (prompts for root via pkexec)
├── IsoVaultGUI.sh             # Shell launcher wrapper
├── IsoVault2Disk.sh           # CLI interface (matching Ventoy2Disk.sh)
├── isovault                   # Symlink to IsoVault2Disk.sh
├── isovault-gui               # Symlink to IsoVaultGUI
│
├── gui/
│   ├── isovault_gui.c         # Native GTK3 GUI in C
│   ├── isovault_gui.glade     # Modern GTK3 Glade layout specification
│   ├── Makefile               # GUI build recipe
│   └── isovault_gui           # Compiled GUI binary
│
├── launcher/
│   └── launcher.c             # Polkit/sudo elevation launcher source
│
├── engine/                    # High-performance Rust disk backend (isovault_engine)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            # CLI entry point (list, install, update, check-deps)
│       ├── device.rs          # Block device discovery & inspection
│       ├── geometry.rs        # 64-bit LBA sector math & 4KB alignment
│       ├── partition.rs       # Partition table creation (parted/BLKRRP)
│       ├── writer.rs          # Atomic block I/O & in-memory XZ decompressor
│       ├── progress.rs        # Real-time JSON progress event streaming
│       └── deps.rs            # Distro package manager dependency resolver
│
├── boot/                      # Bootloader binaries
│   ├── boot.img               # MBR bootstrap (446 bytes)
│   ├── core.img.xz            # GRUB core image
│   └── theme/
│       └── background.jpg     # Cyberpunk 1080p theme backdrop
│
├── ventoy/                    # Runtime environment
│   ├── ventoy.disk.img.xz     # 32 MiB EFI partition image (GRUB + dmsetup ventoy.cpio)
│   └── version                # Version string (1.1.18)
│
└── scripts/
    └── build_all.sh           # Master build script
```

---

## How to Use

### 1. Graphical Interface (GUI)
Launch the graphical app:
```bash
./isovault-gui
# or
./IsoVaultGUI
# or
./IsoVaultGUI.sh
```
1. Select your target USB drive from the dropdown.
2. Select **MBR** (BIOS + UEFI) or **GPT** (UEFI).
3. Select **exFAT** (recommended for large files) or **FAT32**.
4. Click **Install** and confirm the safety dialog.
5. Wait for the progress bar to reach 100%.

### 2. Command Line Interface (CLI)
List detected devices:
```bash
./isovault -l
```

Install to a USB drive:
```bash
sudo ./isovault -i /dev/sdb
# or with GPT partition style:
sudo ./isovault -i -g /dev/sdb
```

Safely update bootloader without touching user ISOs:
```bash
sudo ./isovault -u /dev/sdb
```

Check and install system dependencies:
```bash
sudo ./isovault --deps
```

### 3. Copying ISOs & Booting
1. Mount the newly formatted USB drive (`Ventoy` partition).
2. Drag and drop any `.iso` files (Ubuntu, Arch, Windows, Fedora, Debian, etc.) into the drive.
3. Plug the USB drive into any PC, boot via BIOS/UEFI boot menu, select the USB, and enjoy seamless multibooting!

---

## Rebuilding From Source

To recompile all C and Rust components:
```bash
./scripts/build_all.sh
```

## License
GPL-3.0 License
