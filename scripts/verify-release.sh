#!/usr/bin/env bash
# Verify that a release artifact boots and shows the expected banner.
#
# Launches QEMU headless (no display) against the given image, waits up
# to TIMEOUT seconds for "Hello from Uncalibrated Sextant" to appear in
# the serial log, then kills QEMU and reports the result.
#
# Usage: verify-release.sh <image-path> [<format>]
#   image-path  Path to the disk image to verify.
#   format      qemu-img format string: 'raw' (default) or 'qcow2'.
#
# Exit codes:
#   0  Banner found — artifact verified.
#   1  Timeout or banner not found.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_PATH="${1:?Usage: verify-release.sh <image-path> [<format>]}"
FORMAT="${2:-raw}"
TIMEOUT="${VERIFY_TIMEOUT:-30}"

# Resolve relative paths against the repo root.
case "$IMAGE_PATH" in
    /*) ;;
    *)  IMAGE_PATH="$REPO_ROOT/$IMAGE_PATH" ;;
esac

SERIAL_LOG="$REPO_ROOT/dist/verify-serial.log"
VARS_COPY="$REPO_ROOT/dist/verify-OVMF_VARS.fd"

# Clean up any previous verification log.
rm -f "$SERIAL_LOG"

# Copy a fresh writable VARS image for a clean EFI variable slate.
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$VARS_COPY"

echo "Verifying: $IMAGE_PATH (format=$FORMAT)"

qemu-system-x86_64 \
    -enable-kvm \
    -machine q35 \
    -cpu qemu64 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$VARS_COPY" \
    -drive "format=${FORMAT},file=${IMAGE_PATH}" \
    -display none \
    -no-reboot \
    -serial "file:${SERIAL_LOG}" &

QEMU_PID=$!
echo "QEMU PID: $QEMU_PID"

BANNER='Hello from Uncalibrated Sextant'
ELAPSED=0
FOUND=0

while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
    if [ -f "$SERIAL_LOG" ] && grep -qF "$BANNER" "$SERIAL_LOG" 2>/dev/null; then
        FOUND=1
        break
    fi
    sleep 1
    ELAPSED=$((ELAPSED + 1))
done

# Kill QEMU regardless of outcome; ignore errors if it already exited.
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

# Clean up temporary VARS copy.
rm -f "$VARS_COPY"

if [ "$FOUND" -eq 1 ]; then
    echo "PASS: banner found in serial log after ${ELAPSED}s."
    exit 0
else
    echo "FAIL: banner not found within ${TIMEOUT}s."
    if [ -f "$SERIAL_LOG" ]; then
        echo "--- serial log tail ---"
        tail -20 "$SERIAL_LOG"
    fi
    exit 1
fi
