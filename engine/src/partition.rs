use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::geometry::DiskGeometry;
use crate::progress::Reporter;

pub fn unmount_all(disk_path: &str) {
    if let Ok(content) = std::fs::read_to_string("/proc/mounts") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let dev = parts[0];
                let mount_point = parts[1];
                if dev.starts_with(disk_path) {
                    let _ = Command::new("umount").args(["-l", mount_point]).output();
                }
            }
        }
    }
}

pub fn wipe_disk_header(disk_path: &str) -> Result<(), String> {
    unmount_all(disk_path);

    let mut disk_file = OpenOptions::new()
        .write(true)
        .open(disk_path)
        .map_err(|e| format!("Failed to open {} for wiping: {}", disk_path, e))?;

    let zeroes = [0u8; 32768]; // 32 KB wipe
    disk_file
        .write_all(&zeroes)
        .map_err(|e| format!("Failed to wipe disk header: {}", e))?;
    disk_file
        .sync_all()
        .map_err(|e| format!("Sync failed: {}", e))?;

    Ok(())
}

pub fn create_partitions(
    disk_path: &str,
    geom: &DiskGeometry,
    is_gpt: bool,
    reporter: &Reporter,
) -> Result<(), String> {
    reporter.report(20, "Partitioning", "Creating partition table...", "running");

    wipe_disk_header(disk_path)?;

    if is_gpt {
        let status = Command::new("parted")
            .args([
                "-a",
                "none",
                "--script",
                disk_path,
                "mklabel",
                "gpt",
                "unit",
                "s",
                "mkpart",
                "IsoVault",
                "ntfs",
                &geom.part1_start.to_string(),
                &geom.part1_end.to_string(),
                "mkpart",
                "IsoVaultEFI",
                "fat16",
                &geom.part2_start.to_string(),
                &geom.part2_end.to_string(),
                "set",
                "2",
                "esp",
                "on",
                "quit",
            ])
            .status()
            .map_err(|e| format!("Failed to execute parted: {}", e))?;

        if !status.success() {
            return Err("parted GPT partitioning failed".to_string());
        }
    } else {
        let status = Command::new("parted")
            .args([
                "-a",
                "none",
                "--script",
                disk_path,
                "mklabel",
                "msdos",
                "unit",
                "s",
                "mkpart",
                "primary",
                "ntfs",
                &geom.part1_start.to_string(),
                &geom.part1_end.to_string(),
                "mkpart",
                "primary",
                "fat16",
                &geom.part2_start.to_string(),
                &geom.part2_end.to_string(),
                "set",
                "1",
                "boot",
                "on",
                "quit",
            ])
            .status()
            .map_err(|e| format!("Failed to execute parted: {}", e))?;

        if !status.success() {
            return Err("parted MBR partitioning failed".to_string());
        }

        // Set partition 2 type to 0xEF in MBR table (offset 466)
        let _ = OpenOptions::new()
            .write(true)
            .open(disk_path)
            .and_then(|mut f| {
                use std::io::Seek;
                f.seek(std::io::SeekFrom::Start(466))?;
                f.write_all(&[0xEF])?;
                f.sync_all()
            });
    }

    // Tell kernel to re-read partition table
    reporter.report(30, "Partitioning", "Reloading kernel partition table...", "running");
    let _ = Command::new("partprobe").arg(disk_path).output();
    let _ = Command::new("udevadm").args(["settle"]).output();
    thread::sleep(Duration::from_millis(1500));

    // Verify partition block device nodes exist
    let part1_path = &geom.part1_dev;
    let part2_path = &geom.part2_dev;

    for _ in 0..10 {
        if std::path::Path::new(part1_path).exists() && std::path::Path::new(part2_path).exists() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
        let _ = Command::new("partprobe").arg(disk_path).output();
    }

    if !std::path::Path::new(part1_path).exists() {
        return Err(format!("Partition node {} did not appear", part1_path));
    }

    Ok(())
}

pub fn format_partition1(
    part1_path: &str,
    total_size_gb: u64,
    filesystem: &str,
    reporter: &Reporter,
) -> Result<(), String> {
    reporter.report(
        40,
        "Formatting",
        &format!("Formatting partition 1 ({}) as {}...", part1_path, filesystem),
        "running",
    );

    unmount_all(part1_path);

    let cluster_sectors = if total_size_gb > 32 { 256 } else { 64 };

    let status = match filesystem {
        "fat32" => Command::new("mkfs.fat")
            .args(["-F", "32", "-n", "IsoVault", part1_path])
            .status()
            .map_err(|e| format!("Failed to run mkfs.fat: {}", e))?,
        _ => {
            let res = Command::new("mkexfatfs")
                .args([
                    "-n",
                    "IsoVault",
                    "-s",
                    &cluster_sectors.to_string(),
                    part1_path,
                ])
                .status();

            match res {
                Ok(s) if s.success() => s,
                _ => Command::new("mkfs.exfat")
                    .args(["-n", "IsoVault", part1_path])
                    .status()
                    .map_err(|e| format!("Failed to run mkfs.exfat: {}", e))?,
            }
        }
    };

    if !status.success() {
        return Err(format!("Formatting partition 1 ({}) failed", part1_path));
    }

    Ok(())
}
