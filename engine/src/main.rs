use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::time::Duration;

mod deps;
mod device;
mod geometry;
mod partition;
mod progress;
mod writer;

use device::list_block_devices;
use geometry::{calculate_geometry, get_disk_part_name};
use progress::Reporter;

#[derive(Parser)]
#[command(
    name = "isovault_engine",
    version = "1.1.18",
    about = "Core high-performance disk engine for IsoVault (Multiboot USB Creator)",
    author = "IsoVault Team"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available block storage devices
    List {
        /// Show all devices including non-removable internal drives
        #[arg(short, long)]
        all: bool,

        /// Output in JSON format
        #[arg(short, long)]
        json: bool,
    },

    /// Install IsoVault to target disk (Destructive)
    Install {
        /// Target disk device (e.g. /dev/sdb)
        #[arg(short, long)]
        device: String,

        /// Partition style: "mbr" or "gpt"
        #[arg(short, long, default_value = "mbr")]
        style: String,

        /// Filesystem for data partition: "exfat" or "fat32"
        #[arg(short, long, default_value = "exfat")]
        fs: String,

        /// Reserved space in MiB at end of disk
        #[arg(short, long, default_value = "0")]
        reserve: u64,

        /// Custom path to boot directory containing boot.img & core.img.xz
        #[arg(long, default_value = "/home/prajithj/isovault/boot")]
        boot_dir: PathBuf,

        /// Custom path to boot partition image
        #[arg(long, default_value = "/home/prajithj/isovault/isovault.disk.img.xz")]
        img_path: PathBuf,

        /// Custom path to theme pack directory
        #[arg(long, default_value = "/home/prajithj/isovault/theme_pack")]
        theme_dir: PathBuf,

        /// Custom path to EFI template directory containing master grub.cfg
        #[arg(long, default_value = "/home/prajithj/isovault/efi_pack")]
        efi_dir: PathBuf,

        /// Output progress in JSON format
        #[arg(short, long)]
        json: bool,
    },

    /// Safely update bootloader on existing installation without touching user ISOs
    Update {
        /// Target disk device (e.g. /dev/sdb)
        #[arg(short, long)]
        device: String,

        /// Custom path to boot directory
        #[arg(long, default_value = "/home/prajithj/isovault/boot")]
        boot_dir: PathBuf,

        /// Custom path to boot partition image
        #[arg(long, default_value = "/home/prajithj/isovault/isovault.disk.img.xz")]
        img_path: PathBuf,

        /// Custom path to EFI template directory containing master grub.cfg
        #[arg(long, default_value = "/home/prajithj/isovault/efi_pack")]
        efi_dir: PathBuf,

        /// Output progress in JSON format
        #[arg(short, long)]
        json: bool,
    },

    /// Verify and install required system dependencies
    CheckDeps {
        /// Automatically install missing packages
        #[arg(short, long)]
        install: bool,
    },

    /// Launch sandboxed Live VM preview for target disk (QEMU with snapshot=on)
    TestVm {
        /// Target disk device (e.g. /dev/sdb)
        #[arg(short, long)]
        device: String,
    },

    /// Create a persistent overlay (.dat) container for Linux live ISOs
    CreatePersistence {
        /// Target disk device (e.g. /dev/sdb)
        #[arg(short, long)]
        device: String,

        /// Size in megabytes (e.g. 2048 or 4096)
        #[arg(short, long, default_value = "2048")]
        size: u64,

        /// Filesystem label (casper-rw, writable, persistence)
        #[arg(short, long, default_value = "casper-rw")]
        label: String,

        /// Associate with ISO filename on Partition 1 (e.g. ubuntu-24.04-desktop-amd64.iso)
        #[arg(long)]
        iso: Option<String>,
    },

    /// Scan and verify integrity and SHA-256 checksums of ISO images on target disk
    VerifyIso {
        /// Target disk device (e.g. /dev/sdb)
        #[arg(short, long)]
        device: String,

        /// Output in JSON format
        #[arg(short, long)]
        json: bool,
    },
}

fn read_disk_total_sectors(disk: &str) -> Result<u64, String> {
    let dev_name = Path::new(disk)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid disk path: {}", disk))?;

    let size_path = format!("/sys/class/block/{}/size", dev_name);
    let content = fs::read_to_string(&size_path)
        .map_err(|e| format!("Failed to read sector count from {}: {}", size_path, e))?;

    content
        .trim()
        .parse::<u64>()
        .map_err(|e| format!("Failed to parse sector count: {}", e))
}

fn set_dos_hidden_attribute(path: &Path) {
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    // FAT_IOCTL_SET_ATTRIBUTES = _IOW('r', 0x11, __u32) = 0x40047211
    const FAT_IOCTL_SET_ATTRIBUTES: libc::c_ulong = 0x40047211;
    const ATTR_HIDDEN: u32 = 0x02;

    if let Ok(file) = File::open(path) {
        let mut attr: u32 = ATTR_HIDDEN;
        unsafe {
            let _ = libc::ioctl(file.as_raw_fd(), FAT_IOCTL_SET_ATTRIBUTES, &mut attr);
        }
    }
}

fn init_part1_theme(part1_dev: &str, theme_pack_dir: &Path, reporter: &Reporter) -> Result<(), String> {
    reporter.report(85, "Theme Setup", "Setting up IsoVault theme on data partition...", "running");

    let mount_dir = Path::new("/tmp/isovault_theme_init");
    let _ = fs::create_dir_all(mount_dir);

    // Give kernel and udev time to register partition formatting
    let _ = std::process::Command::new("udevadm").args(["settle"]).output();
    thread::sleep(Duration::from_millis(600));

    // Robust retry loop to mount Partition 1
    let mut mounted = false;
    for attempt in 1..=5 {
        let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_theme_init"]).output();
        let mount_status = std::process::Command::new("mount")
            .args([part1_dev, "/tmp/isovault_theme_init"])
            .status();

        if let Ok(status) = mount_status {
            if status.success() {
                mounted = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(500 * attempt));
        let _ = std::process::Command::new("udevadm").args(["settle"]).output();
    }

    if !mounted {
        return Err(format!("Failed to mount data partition {} after 5 attempts", part1_dev));
    }

    // Partition 1 contains ONLY "isovault" (no ventoy directories)
    let isovault_dir = mount_dir.join("isovault");
    let theme_dir = isovault_dir.join("theme");
    let fonts_dst = theme_dir.join("fonts");
    let _ = fs::create_dir_all(&fonts_dst);

    // Provision Feature directories: Drivers, Persistence, Audits, Streams
    let drv_storage = isovault_dir.join("drivers/storage");
    let drv_net = isovault_dir.join("drivers/net");
    let persistence_dir = isovault_dir.join("persistence");
    let audits_dir = isovault_dir.join("audits");
    let streams_dir = isovault_dir.join("streams");

    let _ = fs::create_dir_all(&drv_storage);
    let _ = fs::create_dir_all(&drv_net);
    let _ = fs::create_dir_all(&persistence_dir);
    let _ = fs::create_dir_all(&audits_dir);
    let _ = fs::create_dir_all(&streams_dir);

    // Write JIT Driver Slipstream guide
    let drv_readme = "==============================================================\n\
                      IsoVault Just-In-Time (JIT) Slipstream Driver Injector\n\
                      ==============================================================\n\
                      Drop raw hardware drivers here to have IsoVault automatically\n\
                      inject them into RAM during boot without modifying your ISOs!\n\n\
                      • Windows Storage / NVMe / Intel VMD RST drivers (.inf, .sys):\n\
                        Place in: /isovault/drivers/storage/\n\
                        -> Automatically loaded by Windows PE installer.\n\n\
                      • Linux WiFi / Ethernet firmware & kernel modules (.ko, .bin):\n\
                        Place in: /isovault/drivers/net/\n\
                        -> Automatically injected into initramfs during boot.\n\
                      ==============================================================\n";
    let _ = fs::write(isovault_dir.join("drivers/README_DRIVERS.txt"), drv_readme);

    // Write Cloud-Stream Boot sample stubs & guide
    let stream_sample = "{\n\
      \"name\": \"Alpine Linux 3.20 (Cloud Stream)\",\n\
      \"kernel_url\": \"https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64/netboot/vmlinuz-lts\",\n\
      \"initrd_url\": \"https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64/netboot/initramfs-lts\",\n\
      \"cmdline\": \"modules=loop,squashfs modloop=https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/x86_64/netboot/modloop-lts quiet\"\n\
    }\n";
    let _ = fs::write(streams_dir.join("alpine_cloud.isostream"), stream_sample);

    let arch_stream = "{\n\
      \"name\": \"Arch Linux Netboot (iPXE Cloud Stream)\",\n\
      \"ipxe_url\": \"https://archlinux.org/releng/netboot/archlinux.ipxe\",\n\
      \"description\": \"Live boot latest Arch Linux directly from official Arch mirror into RAM\"\n\
    }\n";
    let _ = fs::write(streams_dir.join("arch_cloud.isostream"), arch_stream);

    let netboot_stream = "{\n\
      \"name\": \"Netboot.xyz Universal Cloud Boot (100+ OSs)\",\n\
      \"efi_url\": \"https://boot.netboot.xyz/ipxe/netboot.xyz.efi\",\n\
      \"description\": \"Universal network bootloader providing menus for hundreds of operating systems\"\n\
    }\n";
    let _ = fs::write(streams_dir.join("netboot_xyz.isostream"), netboot_stream);

    let streams_readme = "==============================================================\n\
                          IsoVault Cloud-Stream & Netboot Hub\n\
                          ==============================================================\n\
                          Drop *.isostream files here to register cloud network boot\n\
                          profiles in your IsoVault bootloader without consuming disk space!\n\n\
                          Sample stubs included:\n\
                          • alpine_cloud.isostream: Alpine Linux direct RAM netboot\n\
                          • arch_cloud.isostream: Arch Linux rolling netboot\n\
                          • netboot_xyz.isostream: Universal 100+ OS cloud installer\n\
                          ==============================================================\n";
    let _ = fs::write(streams_dir.join("README_STREAMS.txt"), streams_readme);

    // Write Live Persistence Guide
    let persist_readme = "==============================================================\n\
                          IsoVault Live Persistence Container Guide\n\
                          ==============================================================\n\
                          IsoVault supports persistent storage overlays for Live Linux ISOs.\n\
                          Any changes (documents, installed packages, browser history)\n\
                          are saved directly into the persistence .dat container.\n\n\
                          Supported Distribution Labels:\n\
                          • Ubuntu / Linux Mint / Pop!_OS / Zorin OS:\n\
                            Label: casper-rw (or writable for Ubuntu 20.04+)\n\
                            Backend: /isovault/persistence/persistence_casper-rw.dat\n\n\
                          • Debian Live / Kali Linux / Parrot OS:\n\
                            Label: persistence\n\
                            Backend: /isovault/persistence/persistence_persistence.dat\n\
                            Note: Requires /persistence.conf containing \"/ union\"\n\
                            (automatically configured when generated via IsoVault!)\n\n\
                          • Fedora / CentOS / AlmaLinux Live:\n\
                            Label: live-rw\n\
                            Backend: /isovault/persistence/persistence_live-rw.dat\n\n\
                          Quick generation via CLI:\n\
                            sudo isovault --create-persistence /dev/sdX 4096 casper-rw ubuntu.iso\n\
                          ==============================================================\n";
    let _ = fs::write(persistence_dir.join("README_PERSISTENCE.txt"), persist_readme);

    // Source assets from theme pack
    let src_json = theme_pack_dir.join("isovault/isovault.json");
    let src_txt = theme_pack_dir.join("isovault/theme/theme.txt");
    let src_png = theme_pack_dir.join("isovault/theme/background.png");
    let src_jpg = theme_pack_dir.join("isovault/theme/background.jpg");
    let src_sel = theme_pack_dir.join("isovault/theme/select_c.png");
    let src_fonts = theme_pack_dir.join("isovault/theme/fonts");

    // Copy JSON config
    if src_json.exists() {
        let _ = fs::copy(&src_json, isovault_dir.join("isovault.json"));
    }

    // Copy theme assets
    if src_txt.exists() {
        let _ = fs::copy(&src_txt, theme_dir.join("theme.txt"));
    }
    if src_png.exists() {
        let _ = fs::copy(&src_png, theme_dir.join("background.png"));
    }
    if src_jpg.exists() {
        let _ = fs::copy(&src_jpg, theme_dir.join("background.jpg"));
    }
    if src_sel.exists() {
        let _ = fs::copy(&src_sel, theme_dir.join("select_c.png"));
    }

    // Copy fonts
    if let Ok(entries) = fs::read_dir(&src_fonts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    let _ = fs::copy(&path, fonts_dst.join(file_name));
                }
            }
        }
    }

    // FreeDesktop standard: hides listed files/folders in GNOME/KDE/XFCE file managers
    let hidden_content = "isovault\nREADME_ISOVAULT.txt\n";
    let _ = fs::write(mount_dir.join(".hidden"), hidden_content);

    // User-facing documentation on Partition 1
    let readme_content = "====================================================\n\
                          IsoVault Multiboot USB Drive\n\
                          ====================================================\n\n\
                          How to use this drive:\n\
                          1. Drag and drop your .iso, .img, or .wim files directly into this drive.\n\
                          2. Safely eject and boot your computer via BIOS/UEFI.\n\
                          3. Choose your OS from the Cyberpunk menu and boot!\n\n\
                          Tip for Arch / CachyOS Linux:\n\
                          If booting stops at 'Press any key to continue', disable Secure Boot in\n\
                          your BIOS Setup, or press Ctrl+R (Grub2 Mode) in the boot menu.\n";
    let _ = fs::write(mount_dir.join("README_ISOVAULT.txt"), readme_content);

    // Apply DOS hidden attribute (ATTR_HIDDEN 0x02) for Windows Explorer
    set_dos_hidden_attribute(&mount_dir.join("isovault"));
    set_dos_hidden_attribute(&mount_dir.join(".hidden"));
    set_dos_hidden_attribute(&mount_dir.join("README_ISOVAULT.txt"));

    // Sync all dirty pages to ensure hardware persistence
    let _ = std::process::Command::new("sync").status();
    let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_theme_init"]).status();

    Ok(())
}

fn resolve_asset_path(provided: PathBuf, filename: &str) -> PathBuf {
    if provided.exists() {
        return provided;
    }
    // 1. Try relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(canon) = exe.canonicalize() {
            let mut cur = canon.as_path();
            while let Some(parent) = cur.parent() {
                let candidate = parent.join(filename);
                if candidate.exists() {
                    return candidate;
                }
                cur = parent;
            }
        }
    }
    // 2. Try relative to current working directory
    let cwd_candidate = Path::new(".").join(filename);
    if cwd_candidate.exists() {
        return cwd_candidate;
    }
    // 3. Try known directories
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let p1 = PathBuf::from(&home).join("Isovault").join(filename);
        if p1.exists() {
            return p1;
        }
        let p2 = PathBuf::from(&home).join("isovault").join(filename);
        if p2.exists() {
            return p2;
        }
    }
    provided
}

fn sync_part2_efi(part2_dev: &str, disk_dev: &str, efi_dir: &Path, reporter: &Reporter) -> Result<(), String> {
    reporter.report(92, "EFI Template", "Applying master GRUB configuration from template...", "running");
    let mount_dir = Path::new("/tmp/isovault_efi_init");
    let _ = fs::create_dir_all(mount_dir);
    let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_efi_init"]).output();

    // Reread disk partition table on base disk (not partition device)
    let _ = std::process::Command::new("partprobe").arg(disk_dev).output();
    let _ = std::process::Command::new("udevadm").args(["settle"]).output();
    thread::sleep(Duration::from_millis(500));

    let mut mounted = false;
    for attempt in 1..=5 {
        let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_efi_init"]).output();
        let mount_status = std::process::Command::new("mount")
            .args([part2_dev, "/tmp/isovault_efi_init"])
            .status();

        if let Ok(status) = mount_status {
            if status.success() {
                mounted = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(400 * attempt));
        let _ = std::process::Command::new("partprobe").arg(disk_dev).output();
        let _ = std::process::Command::new("udevadm").args(["settle"]).output();
    }

    if mounted {
        let actual_efi_dir = resolve_asset_path(efi_dir.to_path_buf(), "efi_pack");

        // 1. Synchronize grub.cfg and hwinfo.cfg
        let src_grub_cfg = actual_efi_dir.join("grub/grub.cfg");
        let dst_grub_dir = mount_dir.join("grub");
        if src_grub_cfg.exists() {
            let _ = fs::create_dir_all(&dst_grub_dir);
            let _ = fs::copy(&src_grub_cfg, dst_grub_dir.join("grub.cfg"));
        }
        let src_hwinfo_cfg = actual_efi_dir.join("grub/hwinfo.cfg");
        if src_hwinfo_cfg.exists() {
            let _ = fs::copy(&src_hwinfo_cfg, dst_grub_dir.join("hwinfo.cfg"));
        }

        // 2. Synchronize themes recursively
        let src_theme = actual_efi_dir.join("grub/themes");
        if src_theme.exists() {
            let dst_theme = dst_grub_dir.join("themes");
            let _ = fs::create_dir_all(&dst_theme);
            let _ = std::process::Command::new("cp")
                .args(["-r", src_theme.to_str().unwrap(), dst_grub_dir.to_str().unwrap()])
                .status();
        }
        // Remove legacy ventoy theme on Partition 2
        let _ = fs::remove_dir_all(dst_grub_dir.join("themes/ventoy"));

        // 3. Ensure fonts exist in grub/fonts
        let dst_fonts_dir = dst_grub_dir.join("fonts");
        let _ = fs::create_dir_all(&dst_fonts_dir);
        let src_font1 = actual_efi_dir.join("grub/themes/isovault/fonts/SpaceGrotesk-16.pf2");
        let src_font2 = actual_efi_dir.join("grub/themes/isovault/fonts/SpaceGrotesk-18.pf2");
        if src_font1.exists() {
            let _ = fs::copy(&src_font1, dst_fonts_dir.join("SpaceGrotesk-16.pf2"));
            let _ = fs::copy(&src_font1, dst_fonts_dir.join("unicode.pf2"));
        }
        if src_font2.exists() {
            let _ = fs::copy(&src_font2, dst_fonts_dir.join("SpaceGrotesk-18.pf2"));
        }

        // 4. Ensure backup isovault.json exists on Partition 2 for zero-failure fallback
        let dst_isovault_dir = mount_dir.join("isovault");
        let _ = fs::create_dir_all(&dst_isovault_dir);
        let src_ijson = resolve_asset_path(PathBuf::from("theme_pack"), "theme_pack").join("isovault/isovault.json");
        if src_ijson.exists() {
            let _ = fs::copy(&src_ijson, dst_isovault_dir.join("isovault.json"));
            // Also sync to ventoy/ventoy.json on EFI partition for binary module compatibility
            let dst_vdir = mount_dir.join("ventoy");
            if dst_vdir.exists() {
                let _ = fs::copy(&src_ijson, dst_vdir.join("ventoy.json"));
            }
        }

        let _ = std::process::Command::new("sync").status();
        let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_efi_init"]).output();
    }

    Ok(())
}

fn handle_install(
    device: String,
    style: String,
    fs_type: String,
    reserve: u64,
    boot_dir: PathBuf,
    img_path: PathBuf,
    theme_dir: PathBuf,
    efi_dir: PathBuf,
    json: bool,
) {
    let boot_dir = resolve_asset_path(boot_dir, "boot");
    let img_path = resolve_asset_path(img_path, "isovault.disk.img.xz");
    let theme_dir = resolve_asset_path(theme_dir, "theme_pack");
    let efi_dir = resolve_asset_path(efi_dir, "efi_pack");
    let reporter = Reporter::new(json);
    reporter.report(5, "Validating", &format!("Validating target device {}...", device), "running");

    if !Path::new(&device).exists() {
        reporter.report(0, "Error", &format!("Device {} does not exist", device), "error");
        exit(1);
    }

    let is_gpt = style.to_lowercase() == "gpt";

    let total_sectors = match read_disk_total_sectors(&device) {
        Ok(s) => s,
        Err(e) => {
            reporter.report(0, "Error", &e, "error");
            exit(1);
        }
    };

    let total_gb = total_sectors / 2097152;
    if total_sectors > 4294967296 && !is_gpt {
        reporter.report(
            0,
            "Error",
            "Disk exceeds 2TB. MBR cannot address this capacity; use GPT (--style gpt).",
            "error",
        );
        exit(1);
    }

    let geom = match calculate_geometry(&device, total_sectors, reserve, is_gpt) {
        Ok(g) => g,
        Err(e) => {
            reporter.report(0, "Error", &e, "error");
            exit(1);
        }
    };

    reporter.report(
        15,
        "Geometry",
        &format!(
            "Calculated geometry: Part1 [{}-{}], Part2 [{}-{}]",
            geom.part1_start, geom.part1_end, geom.part2_start, geom.part2_end
        ),
        "running",
    );

    // 1. Create partitions
    if let Err(e) = partition::create_partitions(&device, &geom, is_gpt, &reporter) {
        reporter.report(0, "Error", &format!("Partitioning failed: {}", e), "error");
        exit(1);
    }

    // 2. Format partition 1
    if let Err(e) = partition::format_partition1(&geom.part1_dev, total_gb, &fs_type, &reporter) {
        reporter.report(0, "Error", &format!("Formatting partition 1 failed: {}", e), "error");
        exit(1);
    }

    // 3. Setup Theme & Setup Menu on Partition 1
    if let Err(e) = init_part1_theme(&geom.part1_dev, &theme_dir, &reporter) {
        reporter.report(50, "Warning", &format!("Theme setup warning: {}", e), "running");
    }

    // 4. Write bootloader binaries
    if let Err(e) = writer::write_bootloader_data(
        &device,
        &boot_dir,
        &img_path,
        geom.part2_start,
        is_gpt,
        &reporter,
    ) {
        reporter.report(0, "Error", &format!("Writing bootloader failed: {}", e), "error");
        exit(1);
    }

    // 5. Apply master EFI template (grub.cfg, themes)
    let _ = sync_part2_efi(&geom.part2_dev, &device, &efi_dir, &reporter);

    reporter.report(
        100,
        "Success",
        &format!(
            "Successfully installed IsoVault to {} with custom theme and setup menu!",
            device
        ),
        "success",
    );
}

fn handle_update(device: String, boot_dir: PathBuf, img_path: PathBuf, efi_dir: PathBuf, json: bool) {
    let boot_dir = resolve_asset_path(boot_dir, "boot");
    let img_path = resolve_asset_path(img_path, "isovault.disk.img.xz");
    let efi_dir = resolve_asset_path(efi_dir, "efi_pack");
    let reporter = Reporter::new(json);
    reporter.report(5, "Validating", &format!("Verifying existing installation on {}...", device), "running");

    let (is_installed, ver, style) = device::check_isovault_installed(&device);
    if !is_installed {
        reporter.report(
            0,
            "Error",
            &format!("Device {} does not contain a valid IsoVault installation. Use 'install' instead.", device),
            "error",
        );
        exit(1);
    }

    reporter.report(
        20,
        "Checking",
        &format!("Found installed version {} (Style: {}). Preserving partition 1...", ver, style),
        "running",
    );

    let part2_dev = get_disk_part_name(&device, 2);
    let part2_name = Path::new(&part2_dev)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let start_path = format!("/sys/class/block/{}/start", part2_name);
    let part2_start = match fs::read_to_string(&start_path) {
        Ok(c) => match c.trim().parse::<u64>() {
            Ok(s) => s,
            Err(e) => {
                reporter.report(0, "Error", &format!("Failed to parse partition 2 start: {}", e), "error");
                exit(1);
            }
        },
        Err(e) => {
            reporter.report(0, "Error", &format!("Failed to read partition 2 start offset: {}", e), "error");
            exit(1);
        }
    };

    let is_gpt = style == "GPT";

    // Unmount partitions
    partition::unmount_all(&device);

    // Write bootloader data without modifying partition 1
    if let Err(e) = writer::write_bootloader_data(
        &device,
        &boot_dir,
        &img_path,
        part2_start,
        is_gpt,
        &reporter,
    ) {
        reporter.report(0, "Error", &format!("Update failed: {}", e), "error");
        exit(1);
    }

    // Apply master EFI template (grub.cfg, themes)
    let _ = sync_part2_efi(&part2_dev, &device, &efi_dir, &reporter);

    reporter.report(
        100,
        "Success",
        &format!("Successfully updated bootloader on {} without touching user ISOs!", device),
        "success",
    );
}

fn handle_test_vm(device: String) {
    if !Path::new(&device).exists() {
        eprintln!("Error: Device {} does not exist", device);
        exit(1);
    }

    let ovmf_candidates = [
        "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
        "/usr/share/edk2/x64/OVMF.4m.fd",
        "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd",
        "/usr/share/ovmf/x64/OVMF.fd",
        "/usr/share/OVMF/OVMF_CODE.fd",
        "/usr/share/ovmf/OVMF.fd",
    ];
    let ovmf = ovmf_candidates.iter().find(|p| Path::new(p).exists());

    let has_kvm = Path::new("/dev/kvm").exists()
        && std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .is_ok();

    println!("Launching IsoVault Live VM Preview for {} in sandboxed mode (snapshot=on)...", device);

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    if has_kvm {
        cmd.arg("-enable-kvm").args(["-cpu", "host"]);
    } else {
        cmd.args(["-cpu", "qemu64"]);
    }
    cmd.args(["-m", "4096", "-smp", "4"]);

    if let Some(ovmf_path) = ovmf {
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,readonly=on,file={}", ovmf_path));
    }

    cmd.arg("-drive")
        .arg(format!("file={},format=raw,cache=none,if=virtio,snapshot=on", device));
    cmd.args(["-vga", "virtio"]);
    cmd.arg("-name").arg(format!("IsoVault Live Preview [{}]", device));

    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                eprintln!("QEMU exited with code: {:?}", status.code());
            }
        }
        Err(e) => {
            eprintln!("Failed to launch QEMU: {}", e);
            eprintln!("Make sure qemu-system-x86_64 is installed.");
            exit(1);
        }
    }
}

fn handle_create_persistence(
    device: String,
    size_mb: u64,
    label: String,
    iso_opt: Option<String>,
) {
    let part1_dev = get_disk_part_name(&device, 1);
    if !Path::new(&part1_dev).exists() {
        eprintln!("Error: Partition 1 ({}) does not exist. Please format drive with IsoVault first.", part1_dev);
        exit(1);
    }

    let mount_dir = Path::new("/tmp/isovault_persist_mnt");
    let _ = fs::create_dir_all(mount_dir);
    let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();

    let mount_status = std::process::Command::new("mount")
        .args([&part1_dev, mount_dir.to_str().unwrap()])
        .status();

    if mount_status.is_err() || !mount_status.unwrap().success() {
        eprintln!("Error: Failed to mount partition {} to create persistence container.", part1_dev);
        exit(1);
    }

    let persistence_dir = mount_dir.join("isovault/persistence");
    let _ = fs::create_dir_all(&persistence_dir);

    let dat_filename = format!("persistence_{}.dat", label);
    let dat_path = persistence_dir.join(&dat_filename);

    println!("Creating {} MB persistence container at /isovault/persistence/{}...", size_mb, dat_filename);

    // Create sparse file
    let file = match fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&dat_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();
                eprintln!("Error creating file {:?}: {}", dat_path, e);
                exit(1);
            }
        };

    if let Err(e) = file.set_len(size_mb * 1024 * 1024) {
        let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();
        eprintln!("Error allocating file {:?}: {}", dat_path, e);
        exit(1);
    }
    drop(file);

    println!("Formatting container with ext4 (Label: {})...", label);
    let mkfs_res = std::process::Command::new("mkfs.ext4")
        .args(["-F", "-L", &label, "-m", "0", dat_path.to_str().unwrap()])
        .output();

    if mkfs_res.is_err() || !mkfs_res.as_ref().unwrap().status.success() {
        let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();
        eprintln!("Error running mkfs.ext4 on {:?}", dat_path);
        exit(1);
    }

    // If label is "persistence" (Debian/Kali), provision /persistence.conf with "/ union\n"
    if label == "persistence" {
        println!("Configuring Debian/Kali persistence.conf inside container...");
        let inner_mnt = Path::new("/tmp/isovault_dat_inner_mnt");
        let _ = fs::create_dir_all(inner_mnt);
        let _ = std::process::Command::new("umount").args(["-l", inner_mnt.to_str().unwrap()]).output();
        if let Ok(st) = std::process::Command::new("mount")
            .args(["-o", "loop", dat_path.to_str().unwrap(), inner_mnt.to_str().unwrap()])
            .status() {
                if st.success() {
                    let _ = fs::write(inner_mnt.join("persistence.conf"), "/ union\n");
                    let _ = std::process::Command::new("sync").status();
                    let _ = std::process::Command::new("umount").args(["-l", inner_mnt.to_str().unwrap()]).output();
                }
            }
    }

    // Update isovault.json if ISO filename is provided
    if let Some(iso_name) = iso_opt {
        let json_path = mount_dir.join("isovault/isovault.json");
        let mut json_val: serde_json::Value = if json_path.exists() {
            let data = fs::read_to_string(&json_path).unwrap_or_else(|_| "{}".to_string());
            serde_json::from_str(&data).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let iso_clean = if iso_name.starts_with('/') {
            iso_name
        } else {
            format!("/{}", iso_name)
        };
        let backend_path = format!("/isovault/persistence/{}", dat_filename);

        let new_entry = serde_json::json!({
            "image": iso_clean,
            "backend": backend_path
        });

        let mut persistence_list = json_val
            .get("persistence")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Remove old entry if same image
        persistence_list.retain(|e| e.get("image").and_then(|i| i.as_str()) != Some(&iso_clean));
        persistence_list.push(new_entry);

        if let Some(obj) = json_val.as_object_mut() {
            obj.insert("persistence".to_string(), serde_json::Value::Array(persistence_list));
        }

        if let Ok(formatted) = serde_json::to_string_pretty(&json_val) {
            let _ = fs::write(&json_path, formatted);
            println!("Updated /isovault/isovault.json with persistence mapping for {}!", iso_clean);
        }
    }

    let _ = std::process::Command::new("sync").status();
    let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();

    println!("Successfully created persistence container persistence_{}.dat ({} MB)!", label, size_mb);
}

fn handle_verify_iso(device: String, json: bool) {
    let part1_dev = get_disk_part_name(&device, 1);
    if !Path::new(&part1_dev).exists() {
        if json {
            println!("[]");
        } else {
            eprintln!("Error: Partition 1 ({}) not found on {}", part1_dev, device);
        }
        exit(1);
    }

    let mount_dir = Path::new("/tmp/isovault_verify_mnt");
    let _ = fs::create_dir_all(mount_dir);
    let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();

    let mount_status = std::process::Command::new("mount")
        .args([&part1_dev, mount_dir.to_str().unwrap()])
        .status();

    if mount_status.is_err() || !mount_status.unwrap().success() {
        if json {
            println!("[]");
        } else {
            eprintln!("Error: Failed to mount Partition 1 ({})", part1_dev);
        }
        exit(1);
    }

    #[derive(serde::Serialize)]
    struct IsoCheckResult {
        path: String,
        size_bytes: u64,
        size_formatted: String,
        sha256: String,
        status: String,
    }

    let mut results: Vec<IsoCheckResult> = Vec::new();

    fn scan_dir(dir: &Path, base: &Path, results: &mut Vec<IsoCheckResult>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let dname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if dname != "isovault" && dname != ".ventoy" && dname != "ventoy" {
                        scan_dir(&p, base, results);
                    }
                } else if p.is_file() {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    if ["iso", "img", "wim", "vhd"].contains(&ext.as_str()) {
                        let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().to_string();
                        let meta = fs::metadata(&p).ok();
                        let size = meta.map(|m| m.len()).unwrap_or(0);
                        let size_fmt = if size > 1024 * 1024 * 1024 {
                            format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
                        } else {
                            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                        };

                        let sha = if let Ok(out) = std::process::Command::new("sha256sum").arg(&p).output() {
                            let text = String::from_utf8_lossy(&out.stdout);
                            text.split_whitespace().next().unwrap_or("").to_string()
                        } else {
                            "UNKNOWN".to_string()
                        };

                        let status = if size > 0 && !sha.is_empty() && sha != "UNKNOWN" {
                            "VALID".to_string()
                        } else {
                            "CORRUPTED".to_string()
                        };

                        results.push(IsoCheckResult {
                            path: format!("/{}", rel),
                            size_bytes: size,
                            size_formatted: size_fmt,
                            sha256: sha,
                            status,
                        });
                    }
                }
            }
        }
    }

    scan_dir(mount_dir, mount_dir, &mut results);

    let _ = std::process::Command::new("umount").args(["-l", mount_dir.to_str().unwrap()]).output();

    if json {
        let json_out = serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string());
        println!("{}", json_out);
    } else {
        println!("+------------------------------------------+------------+------------------------------------+------------+");
        println!("| ISO / IMAGE FILE                         | SIZE       | SHA-256 CHECKSUM (FIRST 32 CHARS)  | STATUS     |");
        println!("+------------------------------------------+------------+------------------------------------+------------+");
        if results.is_empty() {
            println!("| (No ISO images found on data partition)  |            |                                    |            |");
        } else {
            for r in &results {
                let short_sha = if r.sha256.len() > 32 { &r.sha256[..32] } else { &r.sha256 };
                println!("| {:<40} | {:<10} | {:<34} | {:<10} |", r.path, r.size_formatted, short_sha, r.status);
            }
        }
        println!("+------------------------------------------+------------+------------------------------------+------------+");
        println!("Found {} verified images on {}.", results.len(), part1_dev);
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::List { all, json } => {
            let devices = list_block_devices(all);
            if json {
                println!("{}", serde_json::to_string_pretty(&devices).unwrap());
            } else if devices.is_empty() {
                println!("No removable USB storage devices detected.");
                println!("(Use --all to display internal disks as well)");
            } else {
                println!("{:<14} {:<10} {:<8} {:<10} {:<12} {}", "DEVICE", "SIZE", "STYLE", "INSTALLED", "TRANSPORT", "MODEL");
                println!("{}", "─".repeat(70));
                for d in devices {
                    let installed_str = if d.is_installed {
                        format!("v{}", d.installed_version)
                    } else {
                        "No".to_string()
                    };
                    println!(
                        "{:<14} {:<10} {:<8} {:<10} {:<12} {} {}",
                        d.path, d.size_formatted, d.installed_style, installed_str, d.transport, d.vendor, d.model
                    );
                }
            }
        }

        Commands::Install {
            device,
            style,
            fs,
            reserve,
            boot_dir,
            img_path,
            theme_dir,
            efi_dir,
            json,
        } => {
            handle_install(device, style, fs, reserve, boot_dir, img_path, theme_dir, efi_dir, json);
        }

        Commands::Update {
            device,
            boot_dir,
            img_path,
            efi_dir,
            json,
        } => {
            handle_update(device, boot_dir, img_path, efi_dir, json);
        }

        Commands::CheckDeps { install } => {
            if install {
                if let Err(e) = deps::auto_install_missing_deps() {
                    eprintln!("Dependency installation error: {}", e);
                    exit(1);
                }
            } else {
                println!("{:<36} {:<15} STATUS", "COMPONENT", "COMMAND");
                println!("{}", "─".repeat(60));
                for d in deps::check_dependencies() {
                    let status = if d.is_installed {
                        "\x1b[32mInstalled\x1b[0m"
                    } else {
                        "\x1b[31mMissing\x1b[0m"
                    };
                    println!("{:<36} {:<15} {}", d.name, d.command, status);
                }
            }
        }

        Commands::TestVm { device } => {
            handle_test_vm(device);
        }

        Commands::CreatePersistence {
            device,
            size,
            label,
            iso,
        } => {
            handle_create_persistence(device, size, label, iso);
        }

        Commands::VerifyIso { device, json } => {
            handle_verify_iso(device, json);
        }
    }
}
