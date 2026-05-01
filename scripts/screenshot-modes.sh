#!/usr/bin/env bash
# Headless coverage for the display-mode-keystroke dispatcher.
#
# Boots the already-built ESP image, drives through the locked-
# bootloader scene (Ignore + paste) to parking, presses '3' to
# request a same-resolution mode switch (1024x768 -> 1024x768,
# the binary's default), waits for the toast + repaint to
# settle, then sends 'space' to exit parking and drain events.
# Asserts the resulting serial log carries exactly one
# type=mode_switch line with the expected requested / applied
# dimensions.
#
# Usage: screenshot-modes.sh
# Env:
#   SCREENSHOT_MODES_TIMEOUT  Banner-wait timeout (default 30).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TIMEOUT="${SCREENSHOT_MODES_TIMEOUT:-30}"

mkdir -p "$REPO_ROOT/dist"

SERIAL_LOG="$REPO_ROOT/dist/screenshot-modes-serial.log"
QMP_SOCK="$REPO_ROOT/dist/screenshot-modes-qmp.sock"
VARS_COPY="$REPO_ROOT/dist/screenshot-modes-OVMF_VARS.fd"

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

# Drive AWAITING -> Booting -> bootloader (Ignore + paste) -> Parked,
# then press '3' for a mode-switch and 'space' to exit parking.
QMP_SOCK="$QMP_SOCK" python3 - <<'PY'
import json
import os
import socket
import sys
import time

sock_path = os.environ['QMP_SOCK']

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

# Beat 5: parking screen is now stable. Press '3' to request
# a 1024x768 mode switch (which is already the active mode,
# so applied == requested and the assertion below is exact).
send_key([qcode('3')])

# Beat 6: toast TTL is 1500 ms; mode-switch repaint is fast.
# Wait for the toast to settle and the dispatcher to push the
# ModeSwitch ring-buffer event before exiting. If a future
# tuning change bumps TOAST_MS past 1500, this sleep needs to
# match.
time.sleep(2.0)

# Beat 7: send space to exit parking — non-mode key, falls
# through try_handle_mode_key, triggers the Parked->Parked
# SceneTransition push, releases the run_parked loop.
send_key([qcode('spc')])
PY

# Give the scene time to drain and shut down before we wait on QEMU.
wait "$QEMU_PID" 2>/dev/null || true

# Confirm the ModeSwitch event made it to serial. Exact match
# on the format string in src/serial.rs's drain function for
# Event::ModeSwitch with requested == applied == (1024, 768).
EXPECTED='type=mode_switch requested=1024x768 applied=1024x768'
COUNT=$(grep -cF "$EXPECTED" "$SERIAL_LOG" || true)
if [ "$COUNT" -ne 1 ]; then
    echo "FAIL: expected exactly one ModeSwitch line for 1024x768, got $COUNT" >&2
    echo "--- serial log mode_switch lines ---" >&2
    grep -F 'type=mode_switch' "$SERIAL_LOG" >&2 || true
    exit 1
fi

echo "drain events captured: $(grep -cE '^t=[0-9]+ type=' "$SERIAL_LOG")"
echo "PASS: ModeSwitch line confirmed for 1024x768"
