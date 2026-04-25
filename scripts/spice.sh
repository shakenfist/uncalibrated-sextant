#!/usr/bin/env bash
# Launch QEMU with OVMF firmware, SPICE enabled, and auto-spawn
# remote-viewer to attach.
#
# Runs directly on the host (no Docker wrapper). The SPICE server
# binds on 127.0.0.1:5900; remote-viewer connects automatically.
#
# IMPORTANT EXIT GESTURE: There is no QEMU-owned GTK window in this
# configuration. Closing the remote-viewer window does NOT stop QEMU.
# To tear down the session, press Ctrl-C in this terminal. The trap
# installed below will kill both QEMU and remote-viewer cleanly.
#
# If port 5900 is already in use, set the env variable:
#   SPICE_PORT=5901 make spice
# (Port-probing is Future work; the env override is the escape hatch.)
#
# Usage: spice.sh [ESP_IMAGE_PATH]
#   ESP_IMAGE_PATH defaults to dist/esp.img.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ESP_PATH="${1:-dist/esp.img}"
SPICE_PORT="${SPICE_PORT:-5900}"

# Resolve relative paths against the repo root.
case "$ESP_PATH" in
    /*) ;;
    *)  ESP_PATH="$REPO_ROOT/$ESP_PATH" ;;
esac

# Ensure output directory exists.
mkdir -p "$REPO_ROOT/dist"

# Check that remote-viewer is available before starting QEMU.
if ! command -v remote-viewer >/dev/null 2>&1; then
    echo "ERROR: remote-viewer not found on PATH." >&2
    echo "       Install it with: sudo apt install virt-viewer" >&2
    exit 1
fi

# Copy a fresh writable VARS image each run for a clean EFI variable
# slate (prevents stale boot entries from previous runs interfering).
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$REPO_ROOT/dist/OVMF_VARS.fd"

qemu_pid=''
viewer_pid=''

cleanup() {
    # Kill both children; ignore errors if they have already exited.
    if [ -n "$qemu_pid" ];   then kill "$qemu_pid"   2>/dev/null || true; fi
    if [ -n "$viewer_pid" ]; then kill "$viewer_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT INT TERM

qemu-system-x86_64 \
    -enable-kvm \
    -machine q35 \
    -cpu qemu64 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$REPO_ROOT/dist/OVMF_VARS.fd" \
    -drive format=raw,file="$ESP_PATH" \
    -vga qxl \
    -spice "port=${SPICE_PORT},disable-ticketing=on,addr=127.0.0.1" \
    -display none \
    -serial file:"$REPO_ROOT/dist/serial.log" &
qemu_pid=$!

# Wait for QEMU to bind the SPICE port before spawning remote-viewer.
echo "Waiting for SPICE port ${SPICE_PORT} to open..."
connected=0
for _ in $(seq 1 20); do
    if true </dev/tcp/127.0.0.1/"${SPICE_PORT}" 2>/dev/null; then
        connected=1
        break
    fi
    sleep 0.25
done

if [ "$connected" -ne 1 ]; then
    echo "ERROR: SPICE port ${SPICE_PORT} did not open within 5 seconds." >&2
    exit 1
fi

echo "SPICE port open; launching remote-viewer."
echo "To exit: press Ctrl-C in this terminal (not the remote-viewer window)."
remote-viewer "spice://127.0.0.1:${SPICE_PORT}" &
viewer_pid=$!

# Foreground on QEMU; the session ends when QEMU exits or Ctrl-C fires.
wait "$qemu_pid"
