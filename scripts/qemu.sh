#!/usr/bin/env bash
# Launch QEMU with OVMF firmware and the ESP image.
#
# Runs directly on the host (no Docker wrapper). The resulting GTK
# window opens in the operator's X session, matching ryll's pattern.
#
# Usage: qemu.sh [ESP_IMAGE_PATH]
#   ESP_IMAGE_PATH defaults to dist/esp.img.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ESP_PATH="${1:-dist/esp.img}"

# Resolve relative paths against the repo root.
case "$ESP_PATH" in
    /*) ;;
    *)  ESP_PATH="$REPO_ROOT/$ESP_PATH" ;;
esac

# Ensure output directory exists.
mkdir -p "$REPO_ROOT/dist"

# Copy a fresh writable VARS image each run for a clean EFI variable
# slate (prevents stale boot entries from previous runs interfering).
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$REPO_ROOT/dist/OVMF_VARS.fd"

qemu-system-x86_64 \
    -enable-kvm \
    -machine q35 \
    -cpu qemu64 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$REPO_ROOT/dist/OVMF_VARS.fd" \
    -drive format=raw,file="$ESP_PATH" \
    -display gtk \
    -serial file:"$REPO_ROOT/dist/serial.log"
