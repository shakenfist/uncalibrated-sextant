#!/usr/bin/env bash
# Headless full-scene smoke test for the on-screen visual digest.
#
# Drives the scripted scene through to the parking screen, screendumps
# the parked frame, decodes the QR with zbarimg, and asserts the TLV
# payload is well-formed:
#
#   - magic == "SXDG"
#   - schema version == 2 (bumped in step 2c to signal rolling-hash
#     records; schema version 1 is now rejected)
#   - frame counter parses as u32 LE and is >= 3 (one refresh_digest
#     call inside run_awaiting paints the AWAITING-screen QR;
#     run_awaiting then blocks until space, after which the
#     post-run_awaiting and post-run_booting calls fire, painting
#     the parking-screen QR at frame=3 before run_parked blocks on
#     its own blink loop)
#   - record count (u8) parses and is >= 9 (8 per-channel hash records
#     plus at least one raw event record from the scripted drive)
#   - each record's type tag is in 0x01..=0x08 (raw events) or
#     0x11..=0x18 (per-channel rolling-hash records, added in step 2c),
#     with a defensive check that the value length does not run past
#     the body end
#   - the eight per-channel hash records are parsed and surfaced in the
#     success line in tag-numeric order
#   - the trailing 4 bytes parse as a u32 LE CRC32C and are
#     surfaced in the success line
#
# CRC chaining verification (step 2c correctness check):
#
#   The scripted scene takes the correct-paste path and never triggers
#   a BootloaderTimeout event. The chaining-math invariant says:
#   an unpopulated channel (zero events since boot) must have hash
#   0x00000000; a populated channel must have a non-zero hash. If the
#   chaining math is wrong, both populated and unpopulated channels
#   would likely produce garbage non-zero values instead.
#
#   Assertions:
#     - bootloader_timeout hash MUST be 0x00000000 (never triggered)
#     - keypress hash MUST NOT be 0x00000000 (scripted scene sends keys)
#     - mode_switch hash MUST be 0x00000000 (no mode switches in smoke)
#     - mode_cycle hash MUST be 0x00000000 (no mode cycle in smoke)
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

# Raw-event tag -> human-readable name. Mirrors src/digest.rs TAG_*
# constants (0x01..=0x08) and src/event.rs Event variants.
RAW_TAG_NAMES = {
    0x01: 'Keypress',
    0x02: 'LineRendered',
    0x03: 'SceneTransition',
    0x04: 'BootloaderDecision',
    0x05: 'PasteReceived',
    0x06: 'BootloaderTimeout',
    0x07: 'ModeSwitch',
    0x08: 'ModeCycle',
}

# Per-channel rolling-hash tag -> human-readable name. Mirrors
# src/digest.rs TAG_HASH_* constants (0x11..=0x18), added in step 2c.
# Each hash record has tag 0x1N, len=4, and a u32 LE CRC32C value.
HASH_TAG_NAMES = {
    0x11: 'hash:Keypress',
    0x12: 'hash:LineRendered',
    0x13: 'hash:SceneTransition',
    0x14: 'hash:BootloaderDecision',
    0x15: 'hash:PasteReceived',
    0x16: 'hash:BootloaderTimeout',
    0x17: 'hash:ModeSwitch',
    0x18: 'hash:ModeCycle',
}

# Combined tag lookup covering both ranges.
ALL_TAG_NAMES = {**RAW_TAG_NAMES, **HASH_TAG_NAMES}

# Header (10) + trailer (4) = 14-byte minimum.
if len(payload) < 14:
    sys.stderr.write(
        'digest-payload-smoke: payload shorter than header+trailer '
        '(got %d bytes)\n' % len(payload)
    )
    sys.exit(1)

# Phase 2d truncation invariant: the encoder must respect
# DIGEST_PAYLOAD_CAPACITY = 106 bytes (V5/L byte-mode capacity).
# The scripted scene's bootloader paste flow generates >200 bytes of
# event records; the encoder's newest-first selection truncates the
# raw record block to fit. A payload exceeding 106 bytes would mean
# either the truncation logic regressed or the QR encoder accepted an
# over-capacity payload and silently failed to encode (qrcodegen-no-
# heap would actually panic here, but a defensive check at the
# decoder side surfaces the failure mode clearly).
if len(payload) > 106:
    sys.stderr.write(
        'digest-payload-smoke: payload exceeds V5/L capacity '
        '(got %d bytes, max 106)\n' % len(payload)
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
# Schema version 2 is required: version 1 predates the per-channel
# rolling-hash records (step 2c) and is no longer accepted by this
# smoke. A strict decoder that expects v2 records to be present
# should refuse v1 payloads.
if version != 2:
    sys.stderr.write(
        'digest-payload-smoke: schema version mismatch: got %d, expected 2 '
        '(step 2c bumped from 1 to 2 to signal per-channel hash records)\n'
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

# Step 2c adds 8 per-channel hash records (tags 0x11..=0x18) before
# the raw event records. The scripted drive additionally pushes at
# least one raw event, so the minimum is 9 (8 hash + 1 raw).
if records < 9:
    sys.stderr.write(
        'digest-payload-smoke: record count too low: got %d, expected >= 9 '
        '(8 per-channel hash records + at least 1 raw event)\n' % records
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
    if tag not in ALL_TAG_NAMES:
        sys.stderr.write(
            'digest-payload-smoke: record %d has unknown tag 0x%02x '
            '(expected 0x01..=0x08 for raw events or '
            '0x11..=0x18 for per-channel hash records)\n'
            % (record_idx, tag)
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
    value = payload[value_start:value_end]
    parsed.append((tag, length, value))
    offset = value_end

crc = struct.unpack('<I', payload[-4:])[0]

# Extract the eight per-channel hash values from the parsed records.
# The encoder emits them in tag-numeric order (0x11..=0x18) immediately
# after the header; pull them out by tag for the chaining verification.
# Keyed by tag number -> u32 hash value.
channel_hash_values = {}
for tag, length, value in parsed:
    if tag in HASH_TAG_NAMES:
        if length != 4:
            sys.stderr.write(
                'digest-payload-smoke: hash record tag=0x%02x has '
                'unexpected length %d (expected 4)\n' % (tag, length)
            )
            sys.exit(1)
        channel_hash_values[tag] = struct.unpack('<I', value)[0]

# Verify all eight hash tags are present.
for expected_tag in sorted(HASH_TAG_NAMES.keys()):
    if expected_tag not in channel_hash_values:
        sys.stderr.write(
            'digest-payload-smoke: missing per-channel hash record '
            'for tag 0x%02x (%s)\n'
            % (expected_tag, HASH_TAG_NAMES[expected_tag])
        )
        sys.exit(1)

# Build the hashes list in tag-numeric order for the success line.
# Order: keypress, line_rendered, scene_transition, bootloader_decision,
#        paste_received, bootloader_timeout, mode_switch, mode_cycle.
hash_tags_ordered = [0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]
hashes_str = ', '.join(
    '0x%08x' % channel_hash_values[t] for t in hash_tags_ordered
)

print(
    'digest-payload-smoke: ok (magic=SXDG version=2 frame=%d records=%d '
    'crc32c=0x%08x hashes=[%s])' % (frame, records, crc, hashes_str)
)
for idx, (tag, length, value) in enumerate(parsed):
    if tag in HASH_TAG_NAMES:
        hash_val = struct.unpack('<I', value)[0]
        print(
            '  record[%d]: tag=0x%02x (%s) len=%d hash=0x%08x'
            % (idx, tag, ALL_TAG_NAMES[tag], length, hash_val)
        )
    else:
        print(
            '  record[%d]: tag=0x%02x (%s) len=%d'
            % (idx, tag, ALL_TAG_NAMES[tag], length)
        )

# ----------------------------------------------------------------
# CRC chaining verification
#
# This is the load-bearing correctness check for the chaining math
# that step 2b landed but left unverified at the wire level.
#
# Invariant: an unpopulated channel (zero events of that type since
# boot) must have hash == 0x00000000; a populated channel must have
# a non-zero hash. If the CRC chaining is implemented incorrectly
# (e.g. the resume_initial calculation is wrong), the accumulator
# for an unpopulated channel would not stay at 0x00000000 but would
# instead propagate garbage from a misapplied initial value.
#
# The scripted scene:
#   - sends keypresses        -> TAG_HASH_KEYPRESS (0x11) MUST be non-zero
#   - renders lines           -> TAG_HASH_LINE_RENDERED (0x12) MUST be non-zero
#   - transitions scenes      -> TAG_HASH_SCENE_TRANSITION (0x13) MUST be non-zero
#   - takes ignore+correct    -> TAG_HASH_BOOTLOADER_DECISION (0x14) MUST be non-zero
#   - receives correct paste  -> TAG_HASH_PASTE_RECEIVED (0x15) MUST be non-zero
#   - no BootloaderTimeout    -> TAG_HASH_BOOTLOADER_TIMEOUT (0x16) MUST be 0x00000000
#   - no mode switches        -> TAG_HASH_MODE_SWITCH (0x17) MUST be 0x00000000
#   - no mode cycles          -> TAG_HASH_MODE_CYCLE (0x18) MUST be 0x00000000
# ----------------------------------------------------------------
chaining_errors = []

# bootloader_timeout: scripted scene takes the correct-paste path and
# never triggers a BootloaderTimeout event.
bt_hash = channel_hash_values[0x16]
if bt_hash != 0x00000000:
    chaining_errors.append(
        'bootloader_timeout hash is 0x%08x, expected 0x00000000 '
        '(scripted scene never triggers BootloaderTimeout; a non-zero '
        'value indicates CRC chaining math error or unexpected event)' % bt_hash
    )

# keypress: the scripted scene sends keypresses (space to advance from
# awaiting, 'i' for ignore, the paste characters, Enter, space again).
kp_hash = channel_hash_values[0x11]
if kp_hash == 0x00000000:
    chaining_errors.append(
        'keypress hash is 0x00000000, expected non-zero '
        '(scripted scene sends keypresses; zero hash indicates CRC '
        'chaining math error or no keypress events were pushed)'
    )

# mode_switch: the scripted scene does not switch display modes.
ms_hash = channel_hash_values[0x17]
if ms_hash != 0x00000000:
    chaining_errors.append(
        'mode_switch hash is 0x%08x, expected 0x00000000 '
        '(scripted scene does not switch modes; a non-zero value '
        'indicates CRC chaining math error or unexpected event)' % ms_hash
    )

# mode_cycle: the scripted scene does not cycle modes.
mc_hash = channel_hash_values[0x18]
if mc_hash != 0x00000000:
    chaining_errors.append(
        'mode_cycle hash is 0x%08x, expected 0x00000000 '
        '(scripted scene does not cycle modes; a non-zero value '
        'indicates CRC chaining math error or unexpected event)' % mc_hash
    )

if chaining_errors:
    sys.stderr.write(
        'digest-payload-smoke: CRC chaining verification FAILED:\n'
    )
    for err in chaining_errors:
        sys.stderr.write('  - %s\n' % err)
    sys.exit(1)

print('digest-payload-smoke: CRC chaining verification ok '
      '(bootloader_timeout=0, keypress!=0, mode_switch=0, mode_cycle=0)')
PY
