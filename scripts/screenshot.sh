#!/usr/bin/env bash
# Capture a screenshot of the running scene via QMP.
#
# Boots the already-built ESP image headless, sends a synthetic space
# keypress via QMP to advance AWAITING -> Booting, drives through the
# locked-bootloader scene (Ignore + sextant{HELLO_OPERATOR} + Enter),
# waits for the parking screen to settle, then issues QMP screendump in
# PNG format. The committed docs/images/boot-sequence.png is a product
# of this script.
#
# Timing budget after the space keypress:
#   BOOT_SCRIPT_PRE   18 lines x 200 ms  = 3600 ms
#   bootloader preamble  2 lines x 200 ms  =  400 ms
#   -> 4.5 s settle before sending 'i'
#   blob render (3 LineRendered, no inter-line pacing)  = 0.5 s settle
#   paste 23 chars @ 30 ms/char                        = ~0.7 s
#   BOOT_PAUSE_MS (600) + BOOT_SCRIPT_POST 1 line (200)
#     + parking settle                                  = 2.0 s
# Total ~8.2 s; screendump fires inside the parking blink loop.
#
# Usage: screenshot.sh
# Env:
#   SCREENSHOT_OUTPUT  Override output PNG path (default docs/images/boot-sequence.png).
#   SCREENSHOT_TIMEOUT Banner-wait timeout in seconds (default 30).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUTPUT_PNG="${SCREENSHOT_OUTPUT:-$REPO_ROOT/docs/images/boot-sequence.png}"
TIMEOUT="${SCREENSHOT_TIMEOUT:-30}"

mkdir -p "$(dirname "$OUTPUT_PNG")" "$REPO_ROOT/dist"

# Build ESP fresh so the screenshot reflects whatever is in target/.
"$REPO_ROOT/scripts/mkesp.sh"

SERIAL_LOG="$REPO_ROOT/dist/screenshot-serial.log"
QMP_SOCK="$REPO_ROOT/dist/screenshot-qmp.sock"
VARS_COPY="$REPO_ROOT/dist/screenshot-OVMF_VARS.fd"

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
    echo "FAIL: banner not found within ${TIMEOUT}s" >&2
    exit 1
fi

# Drive AWAITING -> Booting -> bootloader (Ignore + paste) -> Parked via
# QMP and snapshot the parking screen.
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


def send_key(keys):
    """Send a single QMP send-key call and consume the response."""
    send('send-key', keys=keys)
    recv()


def qcode(name):
    """Return a single QMP qcode key descriptor."""
    return {'type': 'qcode', 'data': name}


# Build the key sequence for a single character of PASTE_TARGET.
# Lowercase letters -> bare qcode; uppercase -> shift + lowercase qcode;
# { -> shift + bracket_left; } -> shift + bracket_right;
# _ -> shift + minus.
SHIFT = qcode('shift')

def keys_for_char(ch):
    """Return the list of QMP key descriptors for one character."""
    if ch.islower():
        return [qcode(ch)]
    if ch.isupper():
        return [SHIFT, qcode(ch.lower())]
    if ch == '{':
        return [SHIFT, qcode('bracket_left')]
    if ch == '}':
        return [SHIFT, qcode('bracket_right')]
    if ch == '_':
        return [SHIFT, qcode('minus')]
    raise ValueError(f'no qcode mapping for character: {ch!r}')


greeting = recv()
if not greeting or 'QMP' not in greeting:
    sys.stderr.write(f'unexpected QMP greeting: {greeting!r}\n')
    sys.exit(1)

send('qmp_capabilities')
recv()

# Beat 1: AWAITING settle — wait for the blinking cursor to be on screen
# before sending the synthetic space that starts the boot sequence.
time.sleep(1.5)

# Synthetic space keypress: advances AWAITING -> Booting.
send_key([qcode('spc')])

# Beat 2: BOOT_SCRIPT_PRE settle (18 lines x 200 ms = 3600 ms) plus
# bootloader telemetry preamble (2 lines x 200 ms = 400 ms).
# Using 4.5 s for margin so the R/I/A prompt is on screen.
time.sleep(4.5)

# Send 'i' to select Ignore at the bootloader R/I/A prompt.
send_key([qcode('i')])

# Beat 3: blob screen render settle (intro + blob + input prompt lines).
time.sleep(0.5)

# Type the paste target string one character at a time.
# PASTE_TARGET = "sextant{HELLO_OPERATOR}"
PASTE_TARGET = 'sextant{HELLO_OPERATOR}'
for ch in PASTE_TARGET:
    send_key(keys_for_char(ch))
    # 30 ms inter-key delay so QEMU's keyboard handler does not coalesce
    # rapid consecutive sends; totals ~700 ms for the 23-character string.
    time.sleep(0.03)

# Send Enter to submit the paste.
send_key([qcode('ret')])

# Beat 4: BOOT_PAUSE_MS (600 ms) + BOOT_SCRIPT_POST 1 line (200 ms)
# + parking screen settle.  Use 2 s for margin.
time.sleep(2.0)

send('screendump', filename=output_png, format='png')
# screendump returns after the file is fully written.
resp = recv()
if resp is None or 'error' in resp:
    sys.stderr.write(f'screendump failed: {resp!r}\n')
    sys.exit(1)

# Second synthetic keypress releases the parking screen so the scene
# drains events to serial and ACPI-shuts-down naturally. QEMU exits on
# its own; no QMP quit needed.
send_key([qcode('spc')])
PY

# Give the scene time to drain and shut down before we wait on QEMU.
wait "$QEMU_PID" 2>/dev/null || true

if [ ! -f "$OUTPUT_PNG" ]; then
    echo "FAIL: screenshot not produced at $OUTPUT_PNG" >&2
    exit 1
fi

# Confirm the event drain ran — at minimum one "type=" line should
# appear alongside the startup banner.
if ! grep -qE '^t=[0-9]+ type=' "$SERIAL_LOG"; then
    echo "FAIL: no drain output in serial log" >&2
    echo "--- serial log tail ---" >&2
    tail -20 "$SERIAL_LOG" >&2
    exit 1
fi

ls -lh "$OUTPUT_PNG"
echo "drain events captured: $(grep -cE '^t=[0-9]+ type=' "$SERIAL_LOG")"
