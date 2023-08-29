# img2kvm-rs

A utility that convert disk image in Proxmox VE.

### Usage

The following command can conveniently convert an image file into the disk of a specified virtual machine in Proxmox VE.

```bash
$ img2kvm -n <IMG_FILE> -i <VM_ID> -s <STORAGE>
```

At this point, the virtual machine will show an unused disk.

For more help, please run `img2kvm -h`.

### CLI examples

```bash
$ img2kvm -i openwrt-22.03.5-x86-64-generic-squashfs-combined-efi.img.gz -i 100
```

The image file supports files ending with `iso` and `img`, it also supports `7z`, `gz`, `xz`, and `zip` extensions.

### How to Build

Before executing the command `cargo build --release`, you need to install `liblzma` library.

- In windwos

  ```bash
  > vcpkg install liblzma:x64-windows-static-md
  ```

- In Linux like Ubuntu

  ```bash
  sudo apt-get install liblzma-dev
  ```
