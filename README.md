# IsoVault — Multiboot USB Creator (C + Rust)

**IsoVault** is a high-performance, Linux-native multiboot USB creator. Combining a native **C GTK3 Desktop GUI** with a robust, memory-safe **Rust disk engine**, IsoVault delivers 100% reliable multibooting with zero boot errors, seamless Wayland desktop integration, and out-of-the-box support for modern Linux distributions and Windows.

---

## Credits & Acknowledgements

IsoVault stands on the shoulders of giants. We express our deepest gratitude and credit to:
- **[Ventoy](https://github.com/ventoy/Ventoy)** and its original creator, **[longpanda](https://github.com/longpanda)**.
  Ventoy revolutionized bootable USB creation through its ingenious bootloader architecture: using Linux device-mapper (`dmsetup`) initramfs injection (`ventoy.cpio`), UEFI Shim Secure Boot, and virtual optical disk emulation for Windows.
- The open-source multiboot community for their ongoing contributions to UEFI and GRUB2 boot capabilities.

---

## Recent Updates & Changelog

### 1. Template-Driven Bootloader Synchronization (`efi_pack/`)
- **Master GRUB Template**: The active GRUB configuration template is now centrally managed at `efi_pack/grub/grub.cfg`.
- **Automatic Deployment on Flash**: Whenever you format (`install`) or `update` a drive, `isovault_engine` automatically synchronizes `grub.cfg` from `efi_pack/grub/grub.cfg` directly to Partition 2. Any custom edits made to the template persist and reflect automatically across future drive creations.
- **Master Image Synchronization**: `isovault.disk.img.xz` incorporates the patched EFI payload directly.

### 2. Full Arch Linux & CachyOS Compatibility
- **Initramfs Auto-Detection**: Native pattern matching for all modern Arch-based kernels, including `initramfs-linux-cachyos.img`, `cachyos-lts`, `zen`, `lts`, `hardened`, and `aegis-offensive`.
- **Archiso Symlink Hooking**: Patched `ventoy.cpio` and `ventoy_chain.sh` / `ventoy_loop.sh` to properly detect Archiso kernels (`cachyos`, `garuda`, `endeavour`, `artix`) and automatically generate `/dev/disk/by-label` device symlinks.
- **Fixed Systemd-Boot Fallthrough**: GRUB hook resolution has been decoupled from `loader/entries` checks, ensuring all Arch-derivative ISOs boot cleanly in both UEFI Normal and GRUB2 modes.

### 3. Cyberpunk Theme & Typography
- **Selected Item Indicator**: Enhanced boot menu with full background selection bar indicators (`select_c.png`).
- **Space Grotesk Typography**: Includes custom high-legibility fonts (`SpaceGrotesk-16.pf2`, `SpaceGrotesk-18.pf2`) pre-configured in `isovault.json` and `ventoy.json`.
- **Full Theme Deployment**: Automatically populated to Partition 1 during formatting.

### 4. Modern Wayland & Polkit Privilege Elevation
- Seamless elevation on Wayland compositors (Hyprland, Sway, GNOME Wayland, KDE Plasma Wayland).
- Automatic local display authorization (`xhost +si:localuser:root`) and XWayland fallback to prevent root permission crashes.

---

## Directory Structure

```
~/Isovault/
├── IsoVaultGUI                # C elevation launcher (prompts via pkexec / sudo)
├── IsoVaultGUI.sh             # Desktop shell launcher wrapper
├── IsoVault2Disk.sh           # CLI interface (matching Ventoy command structure)
├── isovault                   # Symlink to IsoVault2Disk.sh
├── isovault-gui               # Symlink to IsoVaultGUI
├── isovault.disk.img.xz       # Master 32MB EFI partition image
│
├── efi_pack/                  # Master EFI / GRUB Template
│   ├── EFI/BOOT/              # Signed UEFI bootloaders (grubx64, grubia32)
│   └── grub/
│       ├── grub.cfg           # Master GRUB config template (automatically synced)
│       └── themes/            # Master EFI themes & fonts
│
├── theme_pack/                # Partition 1 Cyberpunk Theme Assets
│   ├── isovault/              # isovault.json, theme.txt, fonts, wallpapers
│   └── ventoy/                # ventoy.json, theme.txt, fonts, wallpapers
│
├── gui/                       # Native C GTK3 Desktop GUI
│   ├── isovault_gui.c         # GUI implementation
│   ├── isovault_gui.glade     # Glade XML interface layout
│   └── Makefile               # C compilation recipe
│
├── launcher/                  # C elevation binary source
│   └── launcher.c             # Polkit/sudo elevation logic
│
├── engine/                    # High-performance Rust disk backend
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            # Core CLI & workflow coordinator
│       ├── device.rs          # Block device discovery & inspection
│       ├── geometry.rs        # 64-bit LBA geometry & 4KB alignment
│       ├── partition.rs       # Partition table creation (parted/BLKRRP)
│       ├── writer.rs          # Low-level sector writer & XZ streaming
│       ├── progress.rs        # Real-time JSON progress event streaming
│       └── deps.rs            # Distro package manager dependency resolver
│
├── boot/                      # BIOS MBR boot code (boot.img, core.img.xz)
└── scripts/
    └── build_all.sh           # Master build & linking script
```

---

## Installation & Setup

### 1. System Requirements & Dependencies

IsoVault requires standard disk utilities and development tools:

- **Arch Linux / Manjaro / CachyOS**:
  ```bash
  sudo pacman -S --needed base-devel gtk3 json-glib rust cargo parted dosfstools exfatprogs polkit xorg-xhost
  ```

- **Ubuntu / Debian**:
  ```bash
  sudo apt update
  sudo apt install -y build-essential libgtk-3-dev libjson-glib-dev rustc cargo parted dosfstools exfatprogs polkitd x11-xserver-utils
  ```

- **Fedora**:
  ```bash
  sudo dnf install -y gcc make gtk3-devel json-glib-devel rust cargo parted dosfstools exfatprogs polkit xhost
  ```

Alternatively, you can let IsoVault check and install its own dependencies automatically:
```bash
sudo ./IsoVault2Disk.sh --deps
```

### 2. Building From Source

To build the Rust disk engine, the GTK3 GUI, and the C elevation launcher all at once:
```bash
cd ~/Isovault
./scripts/build_all.sh
```

To make the tools accessible globally from any terminal, symlinks can be placed into your `~/.cargo/bin` or `/usr/local/bin`:
```bash
mkdir -p ~/.cargo/bin
ln -sf ~/Isovault/IsoVault2Disk.sh ~/.cargo/bin/isovault
ln -sf ~/Isovault/IsoVaultGUI.sh ~/.cargo/bin/isovault-gui
```
*(Ensure `~/.cargo/bin` is in your shell `$PATH`).*

---

## User Guide: How to Use IsoVault

### Method 1: Graphical Desktop Application (Recommended)

1. Launch the GUI:
   ```bash
   isovault-gui
   # or from the folder:
   ~/Isovault/IsoVaultGUI
   ```
2. When prompted by Polkit, enter your administrator password.
3. **Select Device**: Choose your target USB drive from the top dropdown list.
4. **Choose Partition Style**:
   - **MBR** (Default): Compatible with legacy BIOS systems as well as standard UEFI.
   - **GPT**: Recommended for modern UEFI-only hardware or drives larger than 2TB.
5. **Choose Filesystem**:
   - **exFAT** (Default, Recommended): Allows files larger than 4GB with excellent cross-platform support.
   - **FAT32**: For legacy compatibility.
6. **Install**: Click the **Install** button. A confirmation warning will appear to ensure you don't accidentally erase a drive with important data. Confirm, and wait for the progress bar to reach 100%.

> [!NOTE]
> If IsoVault is already installed on the drive, the **Update** button will become active. Clicking **Update** will safely refresh the bootloader on Partition 2 without modifying or deleting your stored ISOs.

---

### Method 2: Command Line Interface (CLI)

The CLI tool (`isovault` or `./IsoVault2Disk.sh`) provides quick commands for terminal workflows:

#### 1. List Available USB Storage Drives
```bash
isovault -l
```
*(Use `isovault -l --all` to list internal disks as well).*

#### 2. Install IsoVault to a USB Drive (Destructive)
```bash
# Standard MBR install with exFAT:
sudo isovault -i /dev/sdX

# Modern GPT install:
sudo isovault -i -g /dev/sdX

# Reserve 2048 MB of unpartitioned space at disk end:
sudo isovault -i -r 2048 /dev/sdX
```
*(Replace `/dev/sdX` with your target USB drive letter, e.g., `/dev/sda`).*

#### 3. Update Bootloader on Existing Drive (Non-Destructive)
To update the bootloader and apply your latest `efi_pack/grub/grub.cfg` without touching any of your ISO files:
```bash
sudo isovault -u /dev/sdX
```

---

## Adding ISOs and Booting

Once flashed, using the USB drive requires zero special software:

1. **Copy Images**:
   - Open your file manager and open the `IsoVault` partition.
   - Simply drag and drop your `.iso`, `.img`, or `.wim` files onto the drive.
   - You can organize images into subfolders (e.g. `Linux/`, `Windows/`, `Rescue/`).
2. **Safe Removal**: Always safely unmount / eject the USB drive before unplugging.
3. **Booting**:
   - Insert the USB drive into your target computer.
   - Press your system's boot key (e.g. `F12`, `F11`, `F8`, `F2`, or `Esc`) during power-on to select the USB drive.
   - Select your desired operating system from the Cyberpunk boot menu and boot.

### Pro-Tips & Troubleshooting

- **Normal Mode vs. GRUB2 Mode**:
  - Most operating systems (Ubuntu, Debian, Fedora, Arch, Windows) boot instantly in **Normal Mode**.
  - If a particular Linux ISO encounters an issue, highlight the ISO in the menu and press `Ctrl+R` to switch to **GRUB2 Mode**.
- **Secure Boot**:
  - If your PC stops with a signature error, either enroll the Ventoy key presented on screen or disable **Secure Boot** in your motherboard UEFI settings.
- **Verify ISO Checksums**:
  - Always verify downloaded ISO files with `sha256sum <file.iso>`. A file that was interrupted or truncated during download will result in GRUB script syntax errors when booting.

---

## Customizing Boot Configuration & Templates

To modify GRUB behavior permanently:
1. Open and edit [`efi_pack/grub/grub.cfg`](file:///home/prajithj/Isovault/efi_pack/grub/grub.cfg).
2. Any formatting or update operations (`isovault -i` or `isovault -u`) will automatically sync your edits directly onto your USB drives.

To customize themes and fonts:
1. Edit files in [`theme_pack/isovault/`](file:///home/prajithj/Isovault/theme_pack/isovault/).
2. You can replace `background.png`, add custom `.pf2` fonts into `theme_pack/isovault/theme/fonts/`, or edit `theme.txt`.

---

## License

IsoVault is licensed under the **GNU General Public License v3.0 (GPL-3.0)**.
See the upstream [Ventoy repository](https://github.com/ventoy/Ventoy) for upstream licensing terms and bootloader component licenses.
