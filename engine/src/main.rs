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

fn init_part1_theme(part1_dev: &str, theme_pack_dir: &Path, reporter: &Reporter) -> Result<(), String> {
    reporter.report(85, "Theme Setup", "Setting up IsoVault theme on data partition...", "running");

    let mount_dir = Path::new("/tmp/isovault_theme_init");
    let _ = fs::create_dir_all(mount_dir);

    // Ensure not mounted previously
    let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_theme_init"]).output();

    // Mount partition 1
    let mount_status = std::process::Command::new("mount")
        .args([part1_dev, "/tmp/isovault_theme_init"])
        .status()
        .map_err(|e| format!("Failed to mount {}: {}", part1_dev, e))?;

    if !mount_status.success() {
        return Err(format!("mount {} failed", part1_dev));
    }

    // Create target directories on Partition 1:
    // Both isovault and ventoy (so Ventoy's hardcoded C plugin loader finds the theme)
    let isovault_dir = mount_dir.join("isovault");
    let isovault_theme = isovault_dir.join("theme");
    let ventoy_dir = mount_dir.join("ventoy");
    let ventoy_theme = ventoy_dir.join("theme");

    let _ = fs::create_dir_all(&isovault_theme);
    let _ = fs::create_dir_all(&ventoy_theme);

    // Copy theme files from theme pack
    let src_json = theme_pack_dir.join("isovault/isovault.json");
    let src_vjson = theme_pack_dir.join("ventoy/ventoy.json");
    let src_txt = theme_pack_dir.join("isovault/theme/theme.txt");
    let src_png = theme_pack_dir.join("isovault/theme/background.png");
    let src_jpg = theme_pack_dir.join("isovault/theme/background.jpg");

    if src_json.exists() {
        let _ = fs::copy(&src_json, isovault_dir.join("isovault.json"));
    }
    if src_vjson.exists() {
        let _ = fs::copy(&src_vjson, ventoy_dir.join("ventoy.json"));
    } else if src_json.exists() {
        let _ = fs::copy(&src_json, ventoy_dir.join("ventoy.json"));
    }
    if src_txt.exists() {
        let _ = fs::copy(&src_txt, isovault_theme.join("theme.txt"));
        let _ = fs::copy(&src_txt, ventoy_theme.join("theme.txt"));
    }
    if src_png.exists() {
        let _ = fs::copy(&src_png, isovault_theme.join("background.png"));
        let _ = fs::copy(&src_png, ventoy_theme.join("background.png"));
    }
    if src_jpg.exists() {
        let _ = fs::copy(&src_jpg, isovault_theme.join("background.jpg"));
        let _ = fs::copy(&src_jpg, ventoy_theme.join("background.jpg"));
    }

    let src_sel = theme_pack_dir.join("isovault/theme/select_c.png");
    if src_sel.exists() {
        let _ = fs::copy(&src_sel, isovault_theme.join("select_c.png"));
        let _ = fs::copy(&src_sel, ventoy_theme.join("select_c.png"));
    }

    // Copy fonts
    let isovault_fonts = isovault_theme.join("fonts");
    let ventoy_fonts = ventoy_theme.join("fonts");
    let _ = fs::create_dir_all(&isovault_fonts);
    let _ = fs::create_dir_all(&ventoy_fonts);

    let src_fonts = theme_pack_dir.join("isovault/theme/fonts");
    if let Ok(entries) = fs::read_dir(&src_fonts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    let _ = fs::copy(&path, isovault_fonts.join(file_name));
                    let _ = fs::copy(&path, ventoy_fonts.join(file_name));
                }
            }
        }
    }

    // Write a helpful README on Partition 1
    let readme_content = "====================================================\n\
                          IsoVault Multiboot USB Drive\n\
                          ====================================================\n\n\
                          How to use this drive:\n\
                          1. Simply drag and drop your .iso, .img, or .wim files into this partition.\n\
                          2. Safely eject and boot your computer via BIOS/UEFI.\n\
                          3. Choose your OS from the Cyberpunk menu and boot!\n\n\
                          Tip for Arch / CachyOS Linux:\n\
                          If booting stops at 'Press any key to continue', disable Secure Boot in\n\
                          your BIOS Setup, or press Ctrl+R (Grub2 Mode) in the boot menu.\n";
    let _ = fs::write(mount_dir.join("README_ISOVAULT.txt"), readme_content);

    // Sync and unmount
    let _ = std::process::Command::new("sync").status();
    let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_theme_init"]).status();

    Ok(())
}

fn sync_part2_efi(part2_dev: &str, efi_dir: &Path, reporter: &Reporter) -> Result<(), String> {
    reporter.report(92, "EFI Template", "Applying master GRUB configuration from template...", "running");
    let mount_dir = Path::new("/tmp/isovault_efi_init");
    let _ = fs::create_dir_all(mount_dir);
    let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_efi_init"]).output();

    // Give kernel a moment to refresh partition nodes
    let _ = std::process::Command::new("partprobe").arg(part2_dev).output();
    let _ = std::process::Command::new("udevadm").args(["settle"]).output();
    thread::sleep(Duration::from_millis(300));

    let mount_status = std::process::Command::new("mount")
        .args([part2_dev, "/tmp/isovault_efi_init"])
        .status();

    if let Ok(status) = mount_status {
        if status.success() {
            // Resolve efi_dir (handle symlink /home/prajithj/isovault vs /home/prajithj/Isovault)
            let actual_efi_dir = if efi_dir.exists() {
                efi_dir.to_path_buf()
            } else {
                PathBuf::from("/home/prajithj/Isovault/efi_pack")
            };

            let src_grub_cfg = actual_efi_dir.join("grub/grub.cfg");
            let dst_grub_dir = mount_dir.join("grub");
            if src_grub_cfg.exists() {
                let _ = fs::create_dir_all(&dst_grub_dir);
                let _ = fs::copy(&src_grub_cfg, dst_grub_dir.join("grub.cfg"));
            }

            let src_theme = actual_efi_dir.join("grub/themes");
            if src_theme.exists() {
                let dst_theme = dst_grub_dir.join("themes");
                let _ = fs::create_dir_all(&dst_theme);
                let _ = std::process::Command::new("cp")
                    .args(["-r", src_theme.to_str().unwrap(), dst_grub_dir.to_str().unwrap()])
                    .status();
            }

            let _ = std::process::Command::new("sync").status();
            let _ = std::process::Command::new("umount").args(["-l", "/tmp/isovault_efi_init"]).output();
        }
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
    let _ = sync_part2_efi(&geom.part2_dev, &efi_dir, &reporter);

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
    let _ = sync_part2_efi(&part2_dev, &efi_dir, &reporter);

    reporter.report(
        100,
        "Success",
        &format!("Successfully updated bootloader on {} without touching user ISOs!", device),
        "success",
    );
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
    }
}
