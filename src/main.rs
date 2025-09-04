use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    process::{exit, Command},
};

use anyhow::{bail, Context, Result};
use bzip2::read::MultiBzDecoder;
use clap::Parser;
use flate2::read::GzDecoder;
use lzma_rust2::{LzmaReader, XzReader};
use once_cell::sync::Lazy;
use zip::ZipArchive;

#[derive(Debug, Parser)]
#[command(name = "img2kvm", about = "A utility that convert disk image in Proxmox VE.")]
struct Parameter {
    /// The name of image file, e.g. openwrt-24.10.2-x86-64-generic-squashfs-combined-efi.img.
    /// Supported ending with 7z, bz2, bzip2, gz, lzma, xz, and zip extensions file.
    #[arg(
        short = 'n',
        long = "image-name",
        help = "the name of image file, e.g. openwrt-24.10.2-x86-64-generic-squashfs-combined-efi.img.\nSupported ending with 7z, bz2, bzip2, gz, lzma, xz, and zip extensions file."
    )]
    image_name: PathBuf,

    /// The ID of VM for Proxmox VE, e.g. '100'.
    #[arg(short = 'i', long = "vm-id", help = "the ID of VM for Proxmox VE, e.g. '100'.")]
    vm_id: usize,

    /// Storage pool of Proxmox VE.
    #[arg(short = 's', long, default_value = "local-lvm", help = "Storage pool of Proxmox VE.")]
    storage: String,
}

static WORK_DIR: Lazy<PathBuf> = Lazy::new(|| env::current_dir().expect("Failed to get current directory"));

fn main() {
    let parameter = Parameter::parse();

    if let Err(e) = run(parameter) {
        eprintln!("Error: {}", e);
        exit(1);
    }
}

fn run(parameter: Parameter) -> Result<()> {
    let image_path = parameter
        .image_name
        .canonicalize()
        .context("Failed to canonicalize image path")?;

    let image_path = dunce::canonicalize(image_path).context("Failed to canonicalize with dunce")?;

    let extension = image_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .context("File has no valid extension")?;

    let mut is_image_file = false;
    let processed_image_path = match extension.as_str() {
        "bz2" | "bzip2" => decompress_bz2_file(image_path)?,
        "gz" => decompress_gz_file(image_path)?,
        "lzma" => decompress_lzma_file(image_path)?,
        "xz" => decompress_xz_file(image_path)?,
        "zip" => decompress_zip_file(image_path)?,
        "img" | "iso" => {
            is_image_file = true;
            image_path
        }
        _ => bail!("Unsupported file extension: {}", extension),
    };

    let vmdisk_name = WORK_DIR.join("img2kvm_temp.qcow2");

    // Convert image to qcow2 format
    println!("--- convert img to qcow2...");
    let output = Command::new("qemu-img")
        .arg("convert")
        .arg("-f")
        .arg("raw")
        .arg("-O")
        .arg("qcow2")
        .arg(&processed_image_path)
        .arg(&vmdisk_name)
        .output()
        .context("Failed to execute qemu-img command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("qemu-img failed: {}", stderr);
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));

    // Import disk to VM
    println!("--- importdisk...");
    let output = Command::new("qm")
        .arg("importdisk")
        .arg(parameter.vm_id.to_string())
        .arg(&vmdisk_name)
        .arg(parameter.storage)
        .output()
        .context("Failed to execute qm command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("qm importdisk failed: {}", stderr);
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));

    // Clean up temporary files
    println!("--- remove temp file...");
    fs::remove_file(&vmdisk_name).context("Failed to remove temporary qcow2 file")?;

    if !is_image_file {
        fs::remove_file(&processed_image_path).context("Failed to remove decompressed image file")?;
    }

    println!("--- success");
    Ok(())
}

fn decompress_bz2_file(file_path: PathBuf) -> Result<PathBuf> {
    println!("decompress bz2 file {}...", file_path.display());

    let file = File::open(&file_path).with_context(|| format!("Failed to open bz2 file: {}", file_path.display()))?;
    let mut decoder = MultiBzDecoder::new(file);

    let mut buffer = Vec::new();
    decoder
        .read_to_end(&mut buffer)
        .context("Failed to decompress bz2 file")?;

    let file_stem = file_path.file_stem().context("Failed to get file stem from bz2 file")?;
    let image_path = WORK_DIR.join(file_stem);

    let mut image_file = File::create(&image_path)
        .with_context(|| format!("Failed to create decompressed file: {}", image_path.display()))?;
    image_file
        .write_all(&buffer)
        .context("Failed to write decompressed data")?;

    Ok(image_path)
}

fn decompress_gz_file(file_path: PathBuf) -> Result<PathBuf> {
    println!("decompress gz file {}...", file_path.display());

    let file = File::open(&file_path).with_context(|| format!("Failed to open gz file: {}", file_path.display()))?;
    let mut decoder = GzDecoder::new(file);

    let mut buffer = Vec::new();
    decoder
        .read_to_end(&mut buffer)
        .context("Failed to decompress gz file")?;

    let file_stem = file_path.file_stem().context("Failed to get file stem from gz file")?;
    let image_path = WORK_DIR.join(file_stem);

    let mut image_file = File::create(&image_path)
        .with_context(|| format!("Failed to create decompressed file: {}", image_path.display()))?;
    image_file
        .write_all(&buffer)
        .context("Failed to write decompressed data")?;

    Ok(image_path)
}

fn decompress_xz_file(file_path: PathBuf) -> Result<PathBuf> {
    println!("decompress xz file {}...", file_path.display());

    let file = File::open(&file_path).with_context(|| format!("Failed to open xz file: {}", file_path.display()))?;
    let mut reader = XzReader::new(file, true);

    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .context("Failed to decompress xz file")?;

    let file_stem = file_path.file_stem().context("Failed to get file stem from xz file")?;
    let image_path = WORK_DIR.join(file_stem);

    let mut image_file = File::create(&image_path)
        .with_context(|| format!("Failed to create decompressed file: {}", image_path.display()))?;
    image_file
        .write_all(&buffer)
        .context("Failed to write decompressed data")?;

    Ok(image_path)
}

fn decompress_lzma_file(file_path: PathBuf) -> Result<PathBuf> {
    println!("decompress lzma file {}...", file_path.display());

    let file = File::open(&file_path).with_context(|| format!("Failed to open lzma file: {}", file_path.display()))?;
    let mut reader = LzmaReader::new_mem_limit(file, 64 * 1_024, None).context("Failed to create LZMA reader")?;

    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .context("Failed to decompress lzma file")?;

    let file_stem = file_path
        .file_stem()
        .context("Failed to get file stem from lzma file")?;
    let image_path = WORK_DIR.join(file_stem);

    let mut image_file = File::create(&image_path)
        .with_context(|| format!("Failed to create decompressed file: {}", image_path.display()))?;
    image_file
        .write_all(&buffer)
        .context("Failed to write decompressed data")?;

    Ok(image_path)
}

fn decompress_zip_file(file_path: PathBuf) -> Result<PathBuf> {
    println!("decompress zip file {}...", file_path.display());

    let file = File::open(&file_path).with_context(|| format!("Failed to open zip file: {}", file_path.display()))?;
    let mut archive = ZipArchive::new(&file).context("Failed to read zip archive")?;

    if archive.len() == 0 {
        bail!("ZIP file is empty");
    }

    let mut entity = archive
        .by_index(0)
        .context("Failed to access first file in ZIP archive")?;

    let file_stem = file_path.file_stem().context("Failed to get file stem from zip file")?;
    let image_path = WORK_DIR.join(file_stem);

    let mut extracted_file = File::create(&image_path)
        .with_context(|| format!("Failed to create extracted file: {}", image_path.display()))?;
    std::io::copy(&mut entity, &mut extracted_file).context("Failed to extract file from ZIP archive")?;

    Ok(image_path)
}
