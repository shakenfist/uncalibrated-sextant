# Visual digest wire format

The visual on-screen digest is a QR code rendered into the
bottom-right corner of the framebuffer. It encodes a fixed-
size TLV payload that an external decoder (e.g. ryll) can
read by screendumping the framebuffer and decoding the QR.

This document is the authoritative wire-format reference.
The source of truth for the encoder is `src/digest.rs`; the
event types are defined in `src/event.rs`. Drift between
this document and those files is a bug.

## QR encoding parameters

- **Version**: 5 (37×37 modules of payload, plus a 4-module
  quiet zone = 45×45 grid)
- **ECC level**: Low (per the QR Code 2005 spec Table 7,
  V5/L byte-mode capacity = 106 bytes)
- **Mode**: byte
- **Module pixel size**: 4×4 — each QR module is rendered as
  a 4×4 px tile via one `BltOp::BufferToVideo` call
  (Principle 6 of `DESIGN.md`)
- **Mask**: automatic (encoder picks)
- **Colours**: phosphor-green foreground, pure black
  background — matching the rest of the harness palette

## Payload layout

The QR encodes exactly `DIGEST_PAYLOAD_CAPACITY = 106` bytes
of byte-mode data, broken into a 10-byte header, a variable-
length TLV body (up to 92 bytes), and a 4-byte trailer.

### Header (10 bytes, fixed)

| Offset | Length | Field              | Encoding                |
|--------|--------|--------------------|-------------------------|
| 0      | 4      | Magic              | ASCII `SXDG`            |
| 4      | 1      | Schema version     | `u8`, currently `0x01`  |
| 5      | 4      | Frame counter      | `u32` little-endian     |
| 9      | 1      | Event record count | `u8` (max 255 records)  |

**Magic** is `b"SXDG"` (Sextant DiGest). It exists so a decoder
seeing a random screenshot can detect "this PNG contains a
digest" with very high confidence — random PNG noise will not
hit four exact bytes at offset 0.

**Schema version** is `0x01` for the current format. Bump
when a field shape changes or a TLV type is repurposed.
Adding a new TLV type does *not* require a version bump
(TLV's whole point).

**Frame counter** is monotonic per boot, starting at `1` for
the first refresh. Wraps at `u32::MAX` (136 years at 1 Hz —
not a real concern).

**Event record count** is the number of TLV records following
the header, before the CRC trailer. Caps at 255 by encoding;
in practice the capacity budget (below) caps it lower.

### Body (TLV records, variable length, up to 92 bytes total)

Each record:

| Offset | Length | Field           | Encoding                  |
|--------|--------|-----------------|---------------------------|
| 0      | 1      | Type tag        | `u8` — see table below    |
| 1      | 1      | Length of value | `u8` (max 255 bytes)      |
| 2      | N      | Value           | type-specific, see below  |

Type tag table (parallel to `serial::drain`'s `type=` strings):

| Tag  | Variant              | Value shape                                                  |
|------|----------------------|--------------------------------------------------------------|
| 0x01 | `Keypress`           | `u64 timestamp_ms`, `u16 unicode`, `u16 scancode` (12 B)     |
| 0x02 | `LineRendered`       | `u64 timestamp_ms`, `u16 row` (10 B)                         |
| 0x03 | `SceneTransition`    | `u64 timestamp_ms`, `u8 from_phase`, `u8 to_phase` (10 B)    |
| 0x04 | `BootloaderDecision` | `u64 timestamp_ms`, `u8 choice`, `u32 attempt` (13 B)        |
| 0x05 | `PasteReceived`      | `u64 timestamp_ms`, `u16 len`, `u8 correct` (11 B)           |
| 0x06 | `BootloaderTimeout`  | `u64 timestamp_ms` (8 B)                                     |
| 0x07 | `ModeSwitch`         | `u64 timestamp_ms`, `u16 req_w`, `u16 req_h`,                |
|      |                      | `u16 app_w`, `u16 app_h` (16 B)                              |
| 0x08 | `ModeCycle`          | `u64 timestamp_ms`, `u32 count`, `u8 interrupted` (13 B)     |

All integers are little-endian.

`phase` and `choice` mini-enums use stable `u8` discriminants
chosen at encode time by an explicit `match`, **not** by
Rust's default repr discriminants. The wire values are:

- `PHASE_AWAITING = 0x00`
- `PHASE_BOOTING = 0x01`
- `PHASE_PARKED = 0x02`
- `CHOICE_RECOVER = 0x00`
- `CHOICE_IGNORE = 0x01`
- `CHOICE_ANYWAY = 0x02`

These wire numbers are stable across reorderings of the Rust
enum variants. Reorder the source freely; the wire stays put.

### Trailer (4 bytes, fixed)

| Offset | Length | Field   | Encoding                   |
|--------|--------|---------|----------------------------|
| 0      | 4      | CRC32C  | Castagnoli, little-endian  |

The CRC32C is computed over every framebuffer pixel byte
*outside* the digest region — i.e., the QR encodes a hash of
everything-on-screen-except-itself. The excluded rectangle is
the runtime right-anchored region at `(origin_x, origin_y)`
with side length `DIGEST_REGION_PX`, where `origin_x` and
`origin_y` derive from the current GOP mode's width and
height (see `Renderer::draw_digest`).

`BltPixel` is 4 bytes (BGRA-ish in uefi-rs 0.37), so a
1024×768 framebuffer is ~3 MB of pixel data to hash. The
read-back goes one scanline at a time via
`BltOp::VideoToBltBuffer` to avoid any framebuffer-sized
allocation; measured cost is ~21 ms per refresh under
OVMF+QXL at 1024×768.

## Capacity budget

QR Version 5 / ECC Low byte-mode capacity is **106 bytes**.

- Header: 10 bytes
- CRC trailer: 4 bytes
- Remaining for TLV records: **92 bytes**

Each TLV record is 2 bytes of header overhead plus 8 to 16
bytes of value. Worst-case (all `ModeSwitch`, 18 B each):
**5 records**. Best-case (all `BootloaderTimeout`, 10 B each):
**9 records**. Typical case (mixed events, ~12 B average):
**6 to 7 records**.

The encoder takes the *most-recent-N* events that fit, not
the oldest-N. It walks the ring buffer in reverse, pushes
records until adding one more would overflow the 92-byte
budget, then emits the included records in chronological
(forward) order in the QR. This matters most on the parking
screen, where the ring buffer typically holds 30+ events
and the QR can only carry the last 6 or 7.

If 6–7 events ever feels too thin, the documented next step
is QR Version 7 (45×45 modules, 154 B byte-mode capacity at
ECC Medium → 140 bytes for records, ~10–12 records). Version
7 at 4-pixel modules with quiet zone runs to 212 px square,
which overflows the current 180 px region — switching would
require either dropping module scale to 3 px or enlarging
the region. Both are larger changes than the current scope
covers; tracked under *Future work* in
[PLAN-visual-digest.md](plans/PLAN-visual-digest.md).

## Choice of ECC level

The harness uses ECC Low to maximise payload capacity. Low
tolerates ~7% of QR modules being unreadable before the
decoder gives up; Medium tolerates ~15% but caps V5 byte
mode at 84 bytes. The 106-byte capacity is load-bearing —
the encoder relies on it to fit a full TLV header, several
event records, and the CRC trailer — so the ECC trade was
worth the lower damage budget.

Practical implication: a future CRT-scruff overlay (see
DESIGN.md) must keep its damage budget conservative in the
digest region, or the QR will stop decoding. The QR sits in
its own non-scruff layer, so the issue is bounded.

## Provenance

- Encoder: `src/digest.rs::encode`
- Event variants: `src/event.rs`
- Renderer: `src/renderer/mod.rs::Renderer::draw_digest`
- Framebuffer hash: `src/renderer/mod.rs::Renderer::crc32c_framebuffer_excluding_digest`
- Smoke harness: `scripts/digest-payload-smoke.sh`
  (decoder reference implementation in Python)

Update this doc and the source it documents together. A
reviewer comparing the table above against
`src/digest.rs::write_record` and `src/event.rs::Event` should
find no drift.
