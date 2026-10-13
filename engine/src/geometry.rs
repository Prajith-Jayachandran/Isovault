pub const BOOT_SECTOR_NUM: u64 = 65536; // 32 MiB
pub const SECTOR_SIZE: u64 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskGeometry {
    pub total_sectors: u64,
    pub part1_start: u64,
    pub part1_end: u64,
    pub part2_start: u64,
    pub part2_end: u64,
    pub part1_dev: String,
    pub part2_dev: String,
}

pub fn get_disk_part_name(disk: &str, part_num: u32) -> String {
    let last_char = disk.chars().last().unwrap_or(' ');
    if last_char.is_ascii_digit() {
        format!("{}p{}", disk, part_num)
    } else {
        format!("{}{}", disk, part_num)
    }
}

pub fn calculate_geometry(
    disk: &str,
    total_sectors: u64,
    reserve_mb: u64,
    is_gpt: bool,
) -> Result<DiskGeometry, String> {
    if total_sectors <= BOOT_SECTOR_NUM + 4096 {
        return Err(format!(
            "Disk is too small ({} sectors). Minimum is {} sectors.",
            total_sectors,
            BOOT_SECTOR_NUM + 4096
        ));
    }

    let part1_start = 2048u64;

    let part1_end_pre = if is_gpt {
        let reserve_sectors = if reserve_mb > 0 {
            reserve_mb * 2048 + 33
        } else {
            33
        };
        if total_sectors <= reserve_sectors + BOOT_SECTOR_NUM + 1 {
            return Err("Disk too small for specified reserve space and EFI partition".to_string());
        }
        total_sectors - reserve_sectors - BOOT_SECTOR_NUM - 1
    } else {
        let reserve_sectors = reserve_mb * 2048;
        if total_sectors <= reserve_sectors + BOOT_SECTOR_NUM + 1 {
            return Err("Disk too small for specified reserve space and EFI partition".to_string());
        }
        total_sectors - reserve_sectors - BOOT_SECTOR_NUM - 1
    };

    let part2_start = part1_end_pre + 1;
    let modsector = part2_start % 8;
    let part1_end = if modsector > 0 {
        part1_end_pre - modsector
    } else {
        part1_end_pre
    };

    let part2_start = part1_end + 1;
    let part2_end = part2_start + BOOT_SECTOR_NUM - 1;

    if part1_start >= part1_end {
        return Err("Calculated partition 1 size is invalid (negative or zero)".to_string());
    }

    Ok(DiskGeometry {
        total_sectors,
        part1_start,
        part1_end,
        part2_start,
        part2_end,
        part1_dev: get_disk_part_name(disk, 1),
        part2_dev: get_disk_part_name(disk, 2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_naming() {
        assert_eq!(get_disk_part_name("/dev/sdb", 1), "/dev/sdb1");
        assert_eq!(get_disk_part_name("/dev/sdb", 2), "/dev/sdb2");
        assert_eq!(get_disk_part_name("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(get_disk_part_name("/dev/loop0", 2), "/dev/loop0p2");
    }

    #[test]
    fn test_geometry_alignment_mbr() {
        let total_sectors = 62914560u64; // ~30 GB
        let geom = calculate_geometry("/dev/sdb", total_sectors, 0, false).unwrap();
        assert_eq!(geom.part1_start, 2048);
        assert_eq!(geom.part2_start, geom.part1_end + 1);
        assert_eq!(geom.part2_start % 8, 0); // 4KB aligned!
        assert_eq!(geom.part2_end - geom.part2_start + 1, BOOT_SECTOR_NUM);
    }

    #[test]
    fn test_geometry_alignment_gpt() {
        let total_sectors = 62914560u64;
        let geom = calculate_geometry("/dev/sdb", total_sectors, 100, true).unwrap();
        assert_eq!(geom.part1_start, 2048);
        assert_eq!(geom.part2_start, geom.part1_end + 1);
        assert_eq!(geom.part2_start % 8, 0); // 4KB aligned!
        assert_eq!(geom.part2_end - geom.part2_start + 1, BOOT_SECTOR_NUM);
    }
}
