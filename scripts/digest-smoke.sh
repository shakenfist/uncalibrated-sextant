#!/usr/bin/env bash
# Headless smoke test for the on-screen visual digest.
#
# Boots the `--features digest-smoke` binary, holds in AWAITING (does
# NOT send the space keystroke that advances to the boot script),
# screendumps the framebuffer to PNG via QMP, decodes the QR with
# zbarimg, and asserts the payload round-trips as "hello".
#
# Verifies the end-to-end pipeline from `Renderer::draw_digest` through
# the GOP framebuffer to an off-target decoder. Production builds
# (without the feature) do not call draw_digest at all; this target is
# the only thing that exercises it.
#
# Mirrors scripts/screenshot.sh's QEMU/QMP/OVMF conventions so the
# headless boot path is identical to the existing screenshot smoke.
# The only intentional divergence is that this script never sends the
# space keystroke - we want the AWAITING screen with the QR, not the
# parked screen.
#
# Usage: digest-smoke.sh
# Env:
#   DIGEST_SMOKE_OUTPUT   Override output PNG path (default
#                         dist/digest-smoke.png).
#   DIGEST_SMOKE_TIMEOUT  Banner-wait timeout in seconds (default 30).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUTPUT_PNG="${DIGEST_SMOKE_OUTPUT:-$REPO_ROOT/dist/digest-smoke.png}"
TIMEOUT="${DIGEST_SMOKE_TIMEOUT:-30}"

# Tooling preflight: fail fast with a friendly message if the host is
# missing zbarimg (most likely missing tool; qemu-system-x86_64 is
# already required by every other target in this repo so its absence
# would have surfaced long before this script ran). python3 needs
# Pillow for the pre-decode invert (see below).
for tool in qemu-system-x86_64 zbarimg python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "digest-smoke: required tool not found: $tool" >&2
        if [ "$tool" = "zbarimg" ]; then
            echo "  install with: sudo apt install zbar-tools" >&2
        fi
        exit 1
    fi
done
if ! python3 -c 'import PIL' >/dev/null 2>&1; then
    echo "digest-smoke: python3 module 'PIL' not found" >&2
    echo "  install with: sudo apt install python3-pil" >&2
    exit 1
fi

mkdir -p "$REPO_ROOT/dist" "$(dirname "$OUTPUT_PNG")"

# Build a feature-aware ESP. mkesp.sh always copies the binary from
# /work/target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi,
# which is whatever the most recent cargo build (the Makefile target's
# preceding `cargo build --release ... --features digest-smoke` step)
# wrote into the named docker volume. So the wiring is implicit but
# correct: build with feature, then mkesp packages the feature binary.
"$REPO_ROOT/scripts/mkesp.sh"

SERIAL_LOG="$REPO_ROOT/dist/digest-smoke-serial.log"
QMP_SOCK="$REPO_ROOT/dist/digest-smoke-qmp.sock"
VARS_COPY="$REPO_ROOT/dist/digest-smoke-OVMF_VARS.fd"

rm -f "$SERIAL_LOG" "$QMP_SOCK"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$VARS_COPY"

qemu-system-x86_64 \
    -enable-kvm \
    -machine q35 \
    -cpu qemu64 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$VARS_COPY" \
    -drive format=raw,file="$REPO_ROOT/dist/esp.img" \
    -display none \
    -serial "file:$SERIAL_LOG" \
    -qmp "unix:$QMP_SOCK,server,nowait" &
QEMU_PID=$!

cleanup() {
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$VARS_COPY" "$QMP_SOCK"
}
trap cleanup EXIT

# Wait for the startup banner so we know the binary reached main and
# AWAITING is about to render.
BANNER='Hello from Uncalibrated Sextant'
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
    if [ -f "$SERIAL_LOG" ] && grep -qF "$BANNER" "$SERIAL_LOG"; then
        break
    fi
    sleep 1
    ELAPSED=$((ELAPSED + 1))
done

if [ "$ELAPSED" -ge "$TIMEOUT" ]; then
    echo "digest-smoke: banner not found within ${TIMEOUT}s" >&2
    exit 1
fi

# Hold in AWAITING (no space keystroke). Let the chrome and digest
# paint, then screendump.
OUTPUT_PNG="$OUTPUT_PNG" QMP_SOCK="$QMP_SOCK" python3 - <<'PY'
import json
import os
import socket
import sys
import time

sock_path = os.environ['QMP_SOCK']
output_png = os.environ['OUTPUT_PNG']

s = socket.socket(socket.AF_UNIX)
s.connect(sock_path)
f = s.makefile('rwb')


def recv():
    line = f.readline()
    if not line:
        return None
    return json.loads(line)


def send(cmd, **args):
    msg = {'execute': cmd}
    if args:
        msg['arguments'] = args
    f.write((json.dumps(msg) + '\n').encode())
    f.flush()


greeting = recv()
if not greeting or 'QMP' not in greeting:
    sys.stderr.write(f'unexpected QMP greeting: {greeting!r}\n')
    sys.exit(1)

send('qmp_capabilities')
recv()

# AWAITING settle - draw_chrome + draw_digest (2025 BltOps) + the
# first cursor blink. 1.5 s mirrors screenshot.sh's pre-space settle
# and is comfortably more than the digest's paint budget.
time.sleep(1.5)

send('screendump', filename=output_png, format='png')
resp = recv()
if resp is None or 'error' in resp:
    sys.stderr.write(f'screendump failed: {resp!r}\n')
    sys.exit(1)
PY

if [ ! -f "$OUTPUT_PNG" ]; then
    echo "digest-smoke: screenshot not produced at $OUTPUT_PNG" >&2
    exit 1
fi

# draw_digest renders QR modules as bright FG (green) on dark BG
# (black) to match the scene's CRT aesthetic, but zbarimg expects
# the standard dark-modules-on-light-background convention and will
# silently report "no symbols" on the as-captured frame. Invert the
# luminance before decoding so the smoke verifies the QR's *bit
# pattern* round-trips, independent of the FG/BG colour choice. The
# inverted PNG lives alongside the original so the operator can
# inspect either after a failure.
INVERTED_PNG="${OUTPUT_PNG%.png}-inverted.png"
python3 - "$OUTPUT_PNG" "$INVERTED_PNG" <<'PY'
import sys
from PIL import Image, ImageOps
src, dst = sys.argv[1], sys.argv[2]
ImageOps.invert(Image.open(src).convert('RGB')).save(dst)
PY

# Decode and assert. zbarimg --raw prints just the payload bytes (no
# "QR-Code:" prefix), and -q suppresses the summary line. We trim a
# trailing newline so a payload of exactly "hello" matches.
decoded=$(zbarimg --raw -q "$INVERTED_PNG" | tr -d '\n')
expected=hello
if [ "$decoded" != "$expected" ]; then
    printf 'digest-smoke: decode mismatch: got %q, expected %q\n' \
        "$decoded" "$expected" >&2
    exit 1
fi

echo "digest-smoke: ok ($decoded)"
