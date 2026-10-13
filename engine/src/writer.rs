use rand::RngCore;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use xz2::read::XzDecoder;

use crate::geometry::SECTOR_SIZE;
use crate::progress::Reporter;

pub fn decompress_xz_to_writer<R: Read, W: Write>(
    reader: R,
    writer: &mut W,
    max_bytes: Option<usize>,
) -> Result<usize, String> {
    let mut decompressor = XzDecoder::new(reader);
    let mut buffer = [0u8; 64 * 1024];
    let mut total_written = 0usize;

    loop {
        let bytes_to_read = if let Some(limit) = max_bytes {
            let remaining = limit - total_written;
            if remaining == 0 {
                break;
            }
            buffer.len().min(remaining)
        } else {
            buffer.len()
        };

        let read_bytes = decompressor
            .read(&mut buffer[..bytes_to_read])
            .map_err(|e| format!("XZ decompression read error: {}", e))?;

        if read_bytes == 0 {
            break;
        }

        writer
            .write_all(&buffer[..read_bytes])
            .map_err(|e| format!("Disk write error: {}", e))?;

        total_written += read_bytes;
    }

    Ok(total_written)
}

pub fn write_bootloader_data(
    disk_path: &str,
    boot_dir: &Path,
    isovault_img_path: &Path,
    part2_start_sector: u64,
    is_gpt: bool,
    reporter: &Reporter,
) -> Result<(), String> {
    reporter.report(60, "Writing MBR", "Writing stage-1 MBR bootstrap code...", "running");

    let mut disk_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(disk_path)
        .map_err(|e| format!("Failed to open {} for writing: {}", disk_path, e))?;

    // 1. Write boot.img (446 bytes)
    let boot_img_path = boot_dir.join("boot.img");
    let mut boot_img = File::open(&boot_img_path)
        .map_err(|e| format!("Failed to open {:?}: {}", boot_img_path, e))?;
    let mut boot_bytes = [0u8; 446];
    boot_img
        .read_exact(&mut boot_bytes)
        .map_err(|e| format!("Failed to read boot.img: {}", e))?;

    disk_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("Seek failed: {}", e))?;
    disk_file
        .write_all(&boot_bytes)
        .map_err(|e| format!("Failed to write boot.img to LBA 0: {}", e))?;

    // 2. Write core.img.xz
    reporter.report(70, "Writing Core", "Decompressing and writing GRUB core image...", "running");
    let core_img_path = boot_dir.join("core.img.xz");
    let core_file = File::open(&core_img_path)
        .map_err(|e| format!("Failed to open {:?}: {}", core_img_path, e))?;

    if is_gpt {
        // Write byte 0x22 at offset 92
        disk_file.seek(SeekFrom::Start(92)).map_err(|e| e.to_string())?;
        disk_file.write_all(&[0x22]).map_err(|e| e.to_string())?;

        // Write core.img starting at sector 34 (2014 sectors = 1031168 bytes)
        disk_file
            .seek(SeekFrom::Start(34 * SECTOR_SIZE))
            .map_err(|e| e.to_string())?;
        decompress_xz_to_writer(core_file, &mut disk_file, Some(2014 * 512))?;

        // Write byte 0x23 at offset 17908
        disk_file.seek(SeekFrom::Start(17908)).map_err(|e| e.to_string())?;
        disk_file.write_all(&[0x23]).map_err(|e| e.to_string())?;
    } else {
        // MBR: write core.img starting at sector 1 (count 2047 sectors = 1048064 bytes)
        disk_file
            .seek(SeekFrom::Start(1 * SECTOR_SIZE))
            .map_err(|e| e.to_string())?;
        decompress_xz_to_writer(core_file, &mut disk_file, Some(2047 * 512))?;
    }

    // 3. Write isovault.disk.img.xz to Partition 2
    reporter.report(80, "Writing EFI", "Writing 32MB boot partition image...", "running");
    let img_file = File::open(isovault_img_path)
        .map_err(|e| format!("Failed to open {:?}: {}", isovault_img_path, e))?;

    disk_file
        .seek(SeekFrom::Start(part2_start_sector * SECTOR_SIZE))
        .map_err(|e| format!("Seek to partition 2 start failed: {}", e))?;

    let written = decompress_xz_to_writer(img_file, &mut disk_file, Some(65536 * 512))?;
    if written < 65536 * 512 {
        return Err(format!(
            "Incomplete write to partition 2: expected {} bytes, wrote {}",
            65536 * 512,
            written
        ));
    }

    // 4. Generate & write 16-byte random UUID (offset 384) and 4-byte disk signature (offset 440)
    let mut rng = rand::thread_rng();
    let mut uuid_bytes = [0u8; 16];
    rng.fill_bytes(&mut uuid_bytes);
    disk_file.seek(SeekFrom::Start(384)).map_err(|e| e.to_string())?;
    disk_file.write_all(&uuid_bytes).map_err(|e| e.to_string())?;

    let mut sig_bytes = [0u8; 4];
    rng.fill_bytes(&mut sig_bytes);
    disk_file.seek(SeekFrom::Start(440)).map_err(|e| e.to_string())?;
    disk_file.write_all(&sig_bytes).map_err(|e| e.to_string())?;

    // 5. Sync to hardware
    reporter.report(90, "Syncing", "Syncing disk buffers to physical device...", "running");
    disk_file
        .sync_all()
        .map_err(|e| format!("Disk sync failed: {}", e))?;

    Ok(())
}
