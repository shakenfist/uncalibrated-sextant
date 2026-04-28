#!/usr/bin/env bash
# Launch QEMU with OVMF firmware, SPICE enabled, and auto-spawn
# `ryll` (not remote-viewer) to attach with paste-as-keystrokes
# enabled.
#
# Companion to scripts/spice.sh. Use this target when you need
# clipboard paste against a guest without vdagent — i.e. the
# locked-bootloader scene's awaiting-payload prompt — because
# remote-viewer cannot deliver paste-as-keystrokes; only ryll
# (with --enable-paste-as-keystrokes) can.
#
# Runs directly on the host (no Docker wrapper). The SPICE
# server binds on 127.0.0.1:5900; ryll connects automatically.
#
# RYLL BINARY LOCATION: Set the RYLL env variable to override.
# Default search order:
#   1. RYLL env variable (if set and non-empty)
#   2. `ryll` on PATH
#   3. ../ryll/target/release/ryll (sibling repo, release build)
#   4. ../ryll/target/debug/ryll (sibling repo, debug build)
# If none resolve, the script aborts with an install/build hint.
#
# IMPORTANT EXIT GESTURE: There is no QEMU-owned GTK window in
# this configuration. Closing the ryll window does NOT stop
# QEMU. To tear down the session, press Ctrl-C in this terminal.
# The trap installed below will kill both QEMU and ryll cleanly.
#
# If port 5900 is already in use, set the env variable:
#   SPICE_PORT=5901 make spice-ryll
# (Port-probing is Future work; the env override is the escape
# hatch.)
#
# Usage: spice-ryll.sh [ESP_IMAGE_PATH]
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

# Resolve the ryll binary. The env override wins; otherwise we
# search PATH then the sibling repo's target/ directory.
RYLL="${RYLL:-}"
if [ -z "$RYLL" ]; then
    if command -v ryll >/dev/null 2>&1; then
        RYLL=ryll
    elif [ -x "$REPO_ROOT/../ryll/target/release/ryll" ]; then
        RYLL="$REPO_ROOT/../ryll/target/release/ryll"
    elif [ -x "$REPO_ROOT/../ryll/target/debug/ryll" ]; then
        RYLL="$REPO_ROOT/../ryll/target/debug/ryll"
    else
        echo "ERROR: ryll binary not found." >&2
        echo "       Set RYLL=/path/to/ryll, or build it in" >&2
        echo "       the sibling shakenfist/ryll repository:" >&2
        echo "         cd ../ryll && cargo build --release" >&2
        exit 1
    fi
fi

# Copy a fresh writable VARS image each run for a clean EFI
# variable slate (prevents stale boot entries from previous
# runs interfering).
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$REPO_ROOT/dist/OVMF_VARS.fd"

qemu_pid=''
ryll_pid=''

cleanup() {
    # Kill both children; ignore errors if they have already exited.
    if [ -n "$qemu_pid" ]; then kill "$qemu_pid" 2>/dev/null || true; fi
    if [ -n "$ryll_pid" ]; then kill "$ryll_pid" 2>/dev/null || true; fi
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

# Wait for QEMU to bind the SPICE port before spawning ryll.
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

echo "SPICE port open; launching ryll ($RYLL)."
echo "Paste-as-keystrokes is enabled; trigger via Ctrl+Alt+V or"
echo "Menu -> Paste in the ryll GUI (NOT Ctrl+Shift+V)."
echo "To exit: press Ctrl-C in this terminal (not the ryll window)."
"$RYLL" \
    --direct "127.0.0.1:${SPICE_PORT}" \
    --enable-paste-as-keystrokes &
ryll_pid=$!

# Foreground on QEMU; the session ends when QEMU exits or Ctrl-C fires.
wait "$qemu_pid"
