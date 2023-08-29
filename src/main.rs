use std::{
    env,
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
    process::{exit, Command},
};

use flate2::read::GzDecoder;
use lzma::LzmaReader;
use once_cell::sync::Lazy;
use structopt::StructOpt;
use zip::ZipArchive;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "img2kvm",
    about = "A utility that convert disk image in Proxmox VE."
)]
struct Parameter {
    /// image name
    #[structopt(
        parse(from_os_str),
        short = "-n",
        long = "--image-name",
        help = "the name of image file, e.g. openwrt-22.03.5-x86-64-generic-squashfs-combined-efi.img. Supported ending with 7z, gz, xz, and zip extensions file."
    )]
    image_name: PathBuf,

    /// vm id
    #[structopt(
        short = "-i",
        long = "--vm-id",
        help = "the ID of VM for Proxmox VE, e.g. '100'."
    )]
    vm_id: usize,

    /// storage
    #[structopt(
        short = "-s",
        long,
        default_value = "local-lvm",
        help = "Storage pool of Proxmox VE."
    )]
    storage: String,
}

static WORK_DIR: Lazy<PathBuf> = Lazy::new(|| env::current_dir().unwrap());

fn main() {
    let parameter = Parameter::from_args();

    match parameter.image_name.canonicalize() {
        Ok(image_path) => {
            let image_path = dunce::canonicalize(image_path).unwrap();

            let mut is_image_file = false;

            let image_path = match image_path
                .extension()
                .unwrap()
                .to_str()
                .unwrap()
                .to_lowercase()
                .as_str()
            {
                // extension is gz
                "gz" => decompress_gz_file(image_path),
                // extension is xz
                "xz" => decompress_xz_file(image_path),
                // extension is zip
                "zip" => decompress_zip_file(image_path),
                // extension is img or iso
                "img" | "iso" => {
                    is_image_file = true;
                    image_path
                }
                _ => {
                    eprintln!("Error: unsupported file: {:?}", image_path);
                    exit(1);
                }
            };

            let vmdisk_name = WORK_DIR.join("img2kvm_temp.qcow2");

            println!("--- convert img to qcow2...");

            let output = Command::new("qemu-img")
                .arg("convert")
                .arg("-f")
                .arg("raw")
                .arg("-O")
                .arg("qcow2")
                .arg(&image_path)
                .arg(&vmdisk_name)
                .output();

            match output {
                Ok(output) => {
                    if output.status.success() {
                        println!("{}", String::from_utf8_lossy(&output.stdout));

                        println!("--- importdisk...\n");

                        let output = Command::new("qm")
                            .arg("importdisk")
                            .arg(parameter.vm_id.to_string())
                            .arg(&vmdisk_name)
                            .arg(parameter.storage)
                            .output();

                        match output {
                            Ok(output) => {
                                if output.status.success() {
                                    println!("{}", String::from_utf8_lossy(&output.stdout));

                                    println!("--- remove temp file...\n");
                                    fs::remove_file(vmdisk_name).unwrap();
                                    if !is_image_file {
                                        fs::remove_file(image_path).unwrap();
                                    }

                                    println!("--- success");
                                } else {
                                    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                                }
                            }
                            Err(error) => match error.kind() {
                                io::ErrorKind::NotFound => {
                                    eprintln!("Error: qm command han't not found.");
                                }
                                io::ErrorKind::PermissionDenied => {
                                    eprintln!("Error: Permission denied.");
                                }
                                _ => {
                                    eprintln!("Error: An unknown error occurred: {:?}", error);
                                }
                            },
                        }
                    } else {
                        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                }
                Err(error) => match error.kind() {
                    io::ErrorKind::NotFound => {
                        eprintln!("Error: qemu-img command han't not found.");
                    }
                    io::ErrorKind::PermissionDenied => {
                        eprintln!("Error: Permission denied.");
                    }
                    _ => {
                        eprintln!("Error: An unknown error occurred: {:?}", error);
                    }
                },
            }
        }
        Err(error) => match error.kind() {
            io::ErrorKind::NotFound => {
                eprintln!("Error: The image file han't not found.");
            }
            io::ErrorKind::PermissionDenied => {
                eprintln!("Error: Permission denied.");
            }
            _ => {
                eprintln!("Error: An unknown error occurred: {:?}", error);
            }
        },
    }
}

fn decompress_gz_file(file_path: PathBuf) -> PathBuf {
    println!("decompress img file {}...\n", file_path.display());

    let file = File::open(&file_path).unwrap();
    let mut decoder = GzDecoder::new(file);

    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer).unwrap();

    let image_path = WORK_DIR.join(PathBuf::from(&file_path.file_stem().unwrap()));

    let mut image_file = File::create(&image_path).unwrap();
    image_file.write_all(&buffer).unwrap();

    image_path
}

fn decompress_xz_file(file_path: PathBuf) -> PathBuf {
    println!("decompress img file {}...\n", file_path.display());

    let file = File::open(&file_path).unwrap();
    let mut reader = LzmaReader::new_decompressor(&file).unwrap();

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();

    let image_path = WORK_DIR.join(PathBuf::from(&file_path.file_stem().unwrap()));

    let mut image_file = File::create(&image_path).unwrap();
    image_file.write_all(&buffer).unwrap();

    image_path
}

fn decompress_zip_file(file_path: PathBuf) -> PathBuf {
    println!("decompress img file {}...\n", file_path.display());

    let file = File::open(&file_path).unwrap();
    let mut archive = ZipArchive::new(&file).unwrap();

    let mut entity = archive.by_index(0).unwrap();

    let image_path = WORK_DIR.join(PathBuf::from(&file_path.file_stem().unwrap()));

    let mut extracted_file = File::create(&image_path).unwrap();
    std::io::copy(&mut entity, &mut extracted_file).unwrap();

    image_path
}
