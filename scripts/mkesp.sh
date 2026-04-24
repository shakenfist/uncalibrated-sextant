#!/usr/bin/env bash
# Assemble a FAT32 ESP image containing the UEFI binary.
#
# Runs inside a disposable Alpine container so the host does not
# need mtools or dosfstools installed. Alpine is used (rather than
# extending the existing rust build image) because it is much
# smaller and starts faster; mtools/dosfstools are available via
# apk with no extra Dockerfile machinery.
#
# The resulting dist/esp.img is host-visible for qemu.sh to read
# directly.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Ensure the dist directory exists on the host before the mount.
mkdir -p "$REPO_ROOT/dist"

docker run --rm \
    -v "$REPO_ROOT":/work \
    -v uncalibrated-sextant-target:/target:ro \
    alpine:3.21 \
    sh -c '
set -euo pipefail
apk add --no-cache mtools dosfstools >/dev/null 2>&1

# Ensure output directory exists inside container (it is the
# host-mounted dist/ so this is a no-op if already present).
mkdir -p /work/dist

# Create a 33 MiB zero-filled image.
dd if=/dev/zero of=/work/dist/esp.img bs=1M count=33 2>/dev/null

# Format as FAT32.
mkfs.vfat -F 32 -n ESP /work/dist/esp.img

# Create the EFI directory tree.
mmd -i /work/dist/esp.img ::/EFI ::/EFI/BOOT

# Copy the UEFI binary in as the default boot application.
mcopy -i /work/dist/esp.img \
    /target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi \
    ::/EFI/BOOT/BOOTX64.EFI

echo "ESP image assembled: /work/dist/esp.img"
chmod 666 /work/dist/esp.img
'
