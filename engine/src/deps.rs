use std::process::Command;

pub struct DependencyCheck {
    pub name: &'static str,
    pub command: &'static str,
    pub is_installed: bool,
}

pub fn check_dependencies() -> Vec<DependencyCheck> {
    let required = [
        ("Parted Disk Tool", "parted"),
        ("Partprobe Partition Synchronizer", "partprobe"),
        ("FAT Filesystem Formatter", "mkfs.fat"),
        ("exFAT Filesystem Formatter", "mkfs.exfat"),
        ("Block Device Lister", "lsblk"),
        ("C Compiler", "gcc"),
        ("Package Config", "pkg-config"),
    ];

    let mut checks = Vec::new();
    for (name, cmd) in required {
        let is_installed = Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        checks.push(DependencyCheck {
            name,
            command: cmd,
            is_installed,
        });
    }

    checks
}

pub fn auto_install_missing_deps() -> Result<(), String> {
    let missing: Vec<_> = check_dependencies()
        .into_iter()
        .filter(|d| !d.is_installed)
        .collect();

    if missing.is_empty() {
        println!("All system dependencies are already installed.");
        return Ok(());
    }

    println!("Detected missing tools: {:?}", missing.iter().map(|m| m.command).collect::<Vec<_>>());

    if Command::new("which").arg("pacman").output().map(|o| o.status.success()).unwrap_or(false) {
        println!("Installing missing dependencies via pacman (Arch Linux)...");
        let status = Command::new("pacman")
            .args(["-S", "--noconfirm", "parted", "dosfstools", "exfatprogs", "util-linux", "gcc", "pkgconf", "gtk3"])
            .status()
            .map_err(|e| format!("Failed to run pacman: {}", e))?;
        if !status.success() {
            return Err("pacman installation failed".to_string());
        }
    } else if Command::new("which").arg("apt-get").output().map(|o| o.status.success()).unwrap_or(false) {
        println!("Installing missing dependencies via apt (Debian/Ubuntu)...");
        let status = Command::new("apt-get")
            .args(["install", "-y", "parted", "dosfstools", "exfatprogs", "util-linux", "gcc", "pkg-config", "libgtk-3-dev"])
            .status()
            .map_err(|e| format!("Failed to run apt-get: {}", e))?;
        if !status.success() {
            return Err("apt-get installation failed".to_string());
        }
    } else if Command::new("which").arg("dnf").output().map(|o| o.status.success()).unwrap_or(false) {
        println!("Installing missing dependencies via dnf (Fedora)...");
        let status = Command::new("dnf")
            .args(["install", "-y", "parted", "dosfstools", "exfatprogs", "util-linux", "gcc", "pkgconf", "gtk3-devel"])
            .status()
            .map_err(|e| format!("Failed to run dnf: {}", e))?;
        if !status.success() {
            return Err("dnf installation failed".to_string());
        }
    } else {
        return Err("Unsupported package manager. Please install missing tools manually.".to_string());
    }

    Ok(())
}
