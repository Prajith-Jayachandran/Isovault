use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::geometry::{get_disk_part_name, BOOT_SECTOR_NUM};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockDevice {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_formatted: String,
    pub model: String,
    pub vendor: String,
    pub transport: String,
    pub is_removable: bool,
    pub is_installed: bool,
    pub installed_version: String,
    pub installed_style: String, // "MBR", "GPT", or "None"
}

#[derive(Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Deserialize)]
struct LsblkDevice {
    name: String,
    path: Option<String>,
    size: Option<u64>,
    model: Option<String>,
    vendor: Option<String>,
    tran: Option<String>,
    rm: Option<bool>,
    hotplug: Option<bool>,
    #[serde(rename = "type")]
    device_type: String,
    #[allow(dead_code)]
    pttype: Option<String>,
}

pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;

    if bytes >= TIB {
        format!("{:.1} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn check_isovault_installed(disk_path: &str) -> (bool, String, String) {
    let part2_path = get_disk_part_name(disk_path, 2);
    let part2_name = Path::new(&part2_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let size_path = format!("/sys/class/block/{}/size", part2_name);
    if let Ok(content) = fs::read_to_string(&size_path) {
        if let Ok(sectors) = content.trim().parse::<u64>() {
            if sectors == BOOT_SECTOR_NUM {
                let pttype_output = Command::new("lsblk")
                    .args(["-dno", "PTTYPE", disk_path])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_uppercase())
                    .unwrap_or_else(|_| "UNKNOWN".to_string());

                let style = match pttype_output.as_str() {
                    "DOS" | "MBR" => "MBR".to_string(),
                    "GPT" => "GPT".to_string(),
                    _ => "MBR".to_string(),
                };

                let mut version = "1.1.18".to_string();
                let candidate_paths = [
                    "version",
                    "/home/prajithj/Isovault/version",
                    "/home/prajithj/isovault/version",
                ];
                for path in &candidate_paths {
                    if let Ok(v) = fs::read_to_string(path) {
                        version = v.trim().to_string();
                        break;
                    }
                }

                return (true, version, style);
            }
        }
    }

    (false, "None".to_string(), "None".to_string())
}

pub fn list_block_devices(show_all: bool) -> Vec<BlockDevice> {
    let output = Command::new("lsblk")
        .args([
            "-J",
            "-b",
            "-o",
            "NAME,PATH,SIZE,MODEL,VENDOR,TRAN,RM,HOTPLUG,TYPE,PTTYPE",
        ])
        .output();

    let mut result = Vec::new();

    let stdout = match output {
        Ok(out) if out.status.success() => out.stdout,
        _ => return result,
    };

    let lsblk: LsblkOutput = match serde_json::from_slice(&stdout) {
        Ok(data) => data,
        Err(_) => return result,
    };

    for dev in lsblk.blockdevices {
        if dev.device_type != "disk" {
            continue;
        }

        if dev.name.starts_with("loop") || dev.name.starts_with("zram") {
            continue;
        }

        let dev_path = dev
            .path
            .unwrap_or_else(|| format!("/dev/{}", dev.name));
        let size_bytes = dev.size.unwrap_or(0);
        let model = dev.model.unwrap_or_default().trim().to_string();
        let vendor = dev.vendor.unwrap_or_default().trim().to_string();
        let tran = dev.tran.unwrap_or_default().trim().to_string();
        let rm = dev.rm.unwrap_or(false);
        let hotplug = dev.hotplug.unwrap_or(false);

        let is_removable = rm || tran == "usb" || hotplug;

        if !show_all && !is_removable {
            continue;
        }

        let (is_installed, installed_version, installed_style) =
            check_isovault_installed(&dev_path);

        result.push(BlockDevice {
            name: dev.name,
            path: dev_path,
            size: size_bytes,
            size_formatted: format_size(size_bytes),
            model,
            vendor,
            transport: tran,
            is_removable,
            is_installed,
            installed_version,
            installed_style,
        });
    }

    result
}
