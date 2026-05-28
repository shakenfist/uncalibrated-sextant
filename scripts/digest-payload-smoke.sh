#!/usr/bin/env bash
# Headless full-scene smoke test for the on-screen visual digest.
#
# Drives the scripted scene through to the parking screen, screendumps
# the parked frame, decodes the QR with zbarimg, and asserts the TLV
# payload is well-formed:
#
#   - magic == "SXDG"
#   - schema version == 1
#   - frame counter parses as u32 LE and is >= 3 (one refresh_digest
#     call inside run_awaiting paints the AWAITING-screen QR;
#     run_awaiting then blocks until space, after which the
#     post-run_awaiting and post-run_booting calls fire, painting
#     the parking-screen QR at frame=3 before run_parked blocks on
#     its own blink loop)
#   - record count (u8) parses and is >= 1 (the scripted scene
#     generates SceneTransition, LineRendered, PasteReceived, and
#     keypress events that survive into the post-parking ring)
#   - each record's type tag is in 0x01..=0x08, with a defensive
#     check that the value length does not run past the body end
#   - the trailing 4 bytes parse as a u32 LE CRC32C and are
#     surfaced in the success line
#
# Usage: digest-payload-smoke.sh
# Env:
#   DIGEST_PAYLOAD_SMOKE_OUTPUT   Override output PNG path (default
#                                 dist/digest-payload.png).
#   DIGEST_PAYLOAD_SMOKE_TIMEOUT  Banner-wait timeout in seconds
#                                 (default 30).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUTPUT_PNG="${DIGEST_PAYLOAD_SMOKE_OUTPUT:-$REPO_ROOT/dist/digest-payload.png}"
TIMEOUT="${DIGEST_PAYLOAD_SMOKE_TIMEOUT:-30}"

for tool in qemu-system-x86_64 zbarimg python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "digest-payload-smoke: required tool not found: $tool" >&2
        if [ "$tool" = "zbarimg" ]; then
            echo "  install with: sudo apt install zbar-tools" >&2
        fi
        exit 1
    fi
done
if ! python3 -c 'import PIL' >/dev/null 2>&1; then
    echo "digest-payload-smoke: python3 module 'PIL' not found" >&2
    echo "  install with: sudo apt install python3-pil" >&2
    exit 1
fi

mkdir -p "$REPO_ROOT/dist" "$(dirname "$OUTPUT_PNG")"

# Stage the ESP image from whatever binary the most recent cargo
# build wrote into the named docker volume.
"$REPO_ROOT/scripts/mkesp.sh"

SERIAL_LOG="$REPO_ROOT/dist/digest-payload-smoke-serial.log"
QMP_SOCK="$REPO_ROOT/dist/digest-payload-smoke-qmp.sock"
VARS_COPY="$REPO_ROOT/dist/digest-payload-smoke-OVMF_VARS.fd"

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
    echo "digest-payload-smoke: banner not found within ${TIMEOUT}s" >&2
    exit 1
fi

# Drive the scripted scene through AWAITING -> booting -> bootloader
# challenge -> parking, then screendump the parked frame.
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
    send('send-key', keys=keys)
    recv()


def qcode(name):
    return {'type': 'qcode', 'data': name}


SHIFT = qcode('shift')


def keys_for_char(ch):
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

# Beat 1: AWAITING settle, advance to boot script.
time.sleep(1.5)
send_key([qcode('spc')])

# Beat 2: BOOT_SCRIPT_PRE + bootloader preamble, choose 'i' (ignore).
time.sleep(4.5)
send_key([qcode('i')])

# Beat 3: blob screen settle, paste the challenge response.
time.sleep(0.5)
for ch in 'sextant{HELLO_OPERATOR}':
    send_key(keys_for_char(ch))
    time.sleep(0.03)
send_key([qcode('ret')])

# Beat 4: BOOT_PAUSE_MS + BOOT_SCRIPT_POST + parking settle.
time.sleep(8.0)

send('screendump', filename=output_png, format='png')
resp = recv()
if resp is None or 'error' in resp:
    sys.stderr.write(f'screendump failed: {resp!r}\n')
    sys.exit(1)

# Release the parking screen so the scene drains and ACPI-shuts-down.
send_key([qcode('spc')])
PY

if [ ! -f "$OUTPUT_PNG" ]; then
    echo "digest-payload-smoke: screenshot not produced at $OUTPUT_PNG" >&2
    exit 1
fi

# Invert luminance so zbarimg's dark-on-light expectation is met.
INVERTED_PNG="${OUTPUT_PNG%.png}-inverted.png"
python3 - "$OUTPUT_PNG" "$INVERTED_PNG" <<'PY'
import sys
from PIL import Image, ImageOps
src, dst = sys.argv[1], sys.argv[2]
ImageOps.invert(Image.open(src).convert('RGB')).save(dst)
PY

# Decode and parse the TLV payload. The body parser is defensive:
# a malformed payload (record length exceeds remaining bytes,
# unknown tag, mismatched body length) fails fast with a clear
# error rather than crashing on an out-of-range slice.
INVERTED_PNG="$INVERTED_PNG" python3 - <<'PY'
import os
import struct
import subprocess
import sys

inverted = os.environ['INVERTED_PNG']
proc = subprocess.run(
    ['zbarimg', '--raw', '-q', inverted],
    capture_output=True,
    check=False,
)
if proc.returncode != 0:
    sys.stderr.write(
        'digest-payload-smoke: zbarimg failed (rc=%d): %s\n'
        % (proc.returncode, proc.stderr.decode('utf-8', 'replace'))
    )
    sys.exit(1)

payload = proc.stdout
if payload.endswith(b'\n'):
    payload = payload[:-1]

# zbarimg's --raw still UTF-8 encodes QR byte-mode output: it
# interprets each input byte as ISO-8859-1 then re-encodes the
# resulting code points as UTF-8, so bytes >= 0x80 become two-byte
# UTF-8 sequences. Reverse that to recover the original byte stream.
try:
    payload = payload.decode('utf-8').encode('latin-1')
except (UnicodeDecodeError, UnicodeEncodeError) as exc:
    sys.stderr.write('digest-payload-smoke: zbarimg output is not UTF-8/'
                     'latin-1 round-trippable: %s\n' % exc)
    sys.exit(1)

# Tag -> human-readable name. Mirrors src/digest.rs TAG_* constants
# and src/event.rs Event variants.
TAG_NAMES = {
    0x01: 'Keypress',
    0x02: 'LineRendered',
    0x03: 'SceneTransition',
    0x04: 'BootloaderDecision',
    0x05: 'PasteReceived',
    0x06: 'BootloaderTimeout',
    0x07: 'ModeSwitch',
    0x08: 'ModeCycle',
}

# Header (10) + trailer (4) = 14-byte minimum.
if len(payload) < 14:
    sys.stderr.write(
        'digest-payload-smoke: payload shorter than header+trailer '
        '(got %d bytes)\n' % len(payload)
    )
    sys.exit(1)

magic = payload[0:4]
if magic != b'SXDG':
    sys.stderr.write(
        'digest-payload-smoke: magic mismatch: got %r, expected b"SXDG"\n'
        % magic
    )
    sys.exit(1)

version = payload[4]
if version != 1:
    sys.stderr.write(
        'digest-payload-smoke: schema version mismatch: got %d, expected 1\n'
        % version
    )
    sys.exit(1)

frame = struct.unpack('<I', payload[5:9])[0]
records = payload[9]

# refresh_digest fires once inside run_awaiting (frame=1, AWAITING
# QR), then post-run_awaiting (frame=2) and post-run_booting
# (frame=3) before run_parked blocks on its own blink loop. The
# parking-screen QR is the frame=3 one — the post-run_parked
# refresh only fires after the operator releases the parking
# screen, which is too late to capture here.
if frame < 3:
    sys.stderr.write(
        'digest-payload-smoke: frame counter too low: got %d, expected >= 3 '
        '(parking-screen refresh)\n' % frame
    )
    sys.exit(1)

# The scripted drive pushes SceneTransition, LineRendered,
# PasteReceived, and Keypress events into the ring; at least one
# must survive into the post-parking digest.
if records < 1:
    sys.stderr.write(
        'digest-payload-smoke: record count too low: got %d, expected >= 1 '
        '(scripted drive should populate the ring)\n' % records
    )
    sys.exit(1)

# Walk the TLV body. Bounds-check defensively so a malformed
# payload fails with a clear error rather than crashing on a slice.
body_end = len(payload) - 4  # last 4 bytes are the CRC trailer.
offset = 10
parsed = []
for record_idx in range(records):
    if offset + 2 > body_end:
        sys.stderr.write(
            'digest-payload-smoke: record %d header overruns body '
            '(offset=%d body_end=%d)\n' % (record_idx, offset, body_end)
        )
        sys.exit(1)
    tag = payload[offset]
    length = payload[offset + 1]
    if tag not in TAG_NAMES:
        sys.stderr.write(
            'digest-payload-smoke: record %d has unknown tag 0x%02x '
            '(expected one of 0x01..=0x08)\n' % (record_idx, tag)
        )
        sys.exit(1)
    value_start = offset + 2
    value_end = value_start + length
    if value_end > body_end:
        sys.stderr.write(
            'digest-payload-smoke: record %d (tag=0x%02x len=%d) value '
            'overruns body (value_end=%d body_end=%d)\n'
            % (record_idx, tag, length, value_end, body_end)
        )
        sys.exit(1)
    parsed.append((tag, length))
    offset = value_end

crc = struct.unpack('<I', payload[-4:])[0]

print(
    'digest-payload-smoke: ok (magic=SXDG version=1 frame=%d records=%d '
    'crc32c=0x%08x)' % (frame, records, crc)
)
for idx, (tag, length) in enumerate(parsed):
    print(
        '  record[%d]: tag=0x%02x (%s) len=%d'
        % (idx, tag, TAG_NAMES[tag], length)
    )
PY
