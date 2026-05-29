//! TLV encoder for the visual on-screen digest. The wire format is
//! documented in `docs/visual-digest-format.md`.
//!
//! The encoder is a pure function over a `RingBuffer<256>` snapshot,
//! a monotonic frame counter, and an injected `framebuffer_hash`. It
//! writes a fixed-size payload into a caller-provided stack buffer:
//!
//! - 10-byte header: magic (`SXDG`) + schema version + frame counter
//!   (u32 LE) + record count (u8).
//! - Variable-length TLV body: one record per `Event`, type tag + u8
//!   length + value bytes. All integers little-endian.
//! - 4-byte trailer: `framebuffer_hash` as u32 LE. The hash itself
//!   is the integrity check; QR ECC handles transport corruption, so
//!   no second CRC of the payload bytes is emitted.
//!
//! Capacity is fixed at 106 bytes (QR Version 5 / ECC Low byte-mode).
//! Records that would overflow are dropped — the encoder takes the
//! *most-recent-N* events that fit, walking the ring buffer in
//! reverse and emitting in chronological (forward) order.

use crc::{Crc, CRC_32_ISCSI};

use crate::event::{BootloaderChoice, Event, Phase, RingBuffer};

/// Magic identifier for a SeXtant DiGest payload. Four exact bytes at
/// offset 0 let a host-side decoder say "this PNG contains a digest"
/// with very high confidence against random noise.
pub(crate) const DIGEST_MAGIC: [u8; 4] = *b"SXDG";

/// Schema version of the wire format. Bumped from 1 to 2 in step 2c
/// to signal that per-channel rolling-hash records (tags 0x11..=0x18)
/// are now present in every payload. Existing raw event tags (0x01..=
/// 0x08) are unchanged; the bump is informational rather than a hard
/// break — a v1-only decoder that encounters v2 records sees unknown
/// tags in the 0x10–0x1F reserved range and should skip them.
pub(crate) const DIGEST_SCHEMA_VERSION: u8 = 0x02;

/// TLV type tag: `Event::Keypress`. Parallel to `serial::drain`'s
/// `type=keypress` discriminator.
pub(crate) const TAG_KEYPRESS: u8 = 0x01;
/// TLV type tag: `Event::LineRendered`. Parallel to
/// `serial::drain`'s `type=line` discriminator.
pub(crate) const TAG_LINE_RENDERED: u8 = 0x02;
/// TLV type tag: `Event::SceneTransition`. Parallel to
/// `serial::drain`'s `type=transition` discriminator.
pub(crate) const TAG_SCENE_TRANSITION: u8 = 0x03;
/// TLV type tag: `Event::BootloaderDecision`. Parallel to
/// `serial::drain`'s `type=bootloader_decision` discriminator.
pub(crate) const TAG_BOOTLOADER_DECISION: u8 = 0x04;
/// TLV type tag: `Event::PasteReceived`. Parallel to
/// `serial::drain`'s `type=paste` discriminator.
pub(crate) const TAG_PASTE_RECEIVED: u8 = 0x05;
/// TLV type tag: `Event::BootloaderTimeout`. Parallel to
/// `serial::drain`'s `type=bootloader_timeout` discriminator.
pub(crate) const TAG_BOOTLOADER_TIMEOUT: u8 = 0x06;
/// TLV type tag: `Event::ModeSwitch`. Parallel to `serial::drain`'s
/// `type=mode_switch` discriminator.
pub(crate) const TAG_MODE_SWITCH: u8 = 0x07;
/// TLV type tag: `Event::ModeCycle`. Parallel to `serial::drain`'s
/// `type=mode_cycle` discriminator.
pub(crate) const TAG_MODE_CYCLE: u8 = 0x08;

/// TLV type tag: per-channel rolling CRC32C hash for `Event::Keypress`.
/// Mirrors `TAG_KEYPRESS` in the 0x10–0x1F reserved range. The value
/// (4 bytes LE) is the CRC32C of every `Keypress` TLV record since boot.
pub(crate) const TAG_HASH_KEYPRESS: u8 = 0x11;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::LineRendered`.
/// Mirrors `TAG_LINE_RENDERED` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_LINE_RENDERED: u8 = 0x12;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::SceneTransition`.
/// Mirrors `TAG_SCENE_TRANSITION` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_SCENE_TRANSITION: u8 = 0x13;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::BootloaderDecision`.
/// Mirrors `TAG_BOOTLOADER_DECISION` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_BOOTLOADER_DECISION: u8 = 0x14;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::PasteReceived`.
/// Mirrors `TAG_PASTE_RECEIVED` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_PASTE_RECEIVED: u8 = 0x15;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::BootloaderTimeout`.
/// Mirrors `TAG_BOOTLOADER_TIMEOUT` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_BOOTLOADER_TIMEOUT: u8 = 0x16;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::ModeSwitch`.
/// Mirrors `TAG_MODE_SWITCH` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_MODE_SWITCH: u8 = 0x17;
/// TLV type tag: per-channel rolling CRC32C hash for `Event::ModeCycle`.
/// Mirrors `TAG_MODE_CYCLE` in the 0x10–0x1F reserved range.
pub(crate) const TAG_HASH_MODE_CYCLE: u8 = 0x18;

/// On-wire byte size of a single per-channel rolling-hash record:
/// one tag byte + one length byte (always 4) + four CRC32C bytes.
pub(crate) const RECORD_HASH_SIZE: usize = 6;

/// Number of per-channel rolling-hash records emitted per payload.
/// One record per `Event` variant; tags 0x11..=0x18 in numeric order.
pub(crate) const NUM_HASH_CHANNELS: usize = 8;

/// Wire discriminant for `Phase::Awaiting`.
pub(crate) const PHASE_AWAITING: u8 = 0x00;
/// Wire discriminant for `Phase::Booting`.
pub(crate) const PHASE_BOOTING: u8 = 0x01;
/// Wire discriminant for `Phase::Parked`.
pub(crate) const PHASE_PARKED: u8 = 0x02;

/// Wire discriminant for `BootloaderChoice::Retry`.
pub(crate) const CHOICE_RECOVER: u8 = 0x00;
/// Wire discriminant for `BootloaderChoice::Ignore`.
pub(crate) const CHOICE_IGNORE: u8 = 0x01;
/// Wire discriminant for `BootloaderChoice::Abort`.
pub(crate) const CHOICE_ANYWAY: u8 = 0x02;

/// Fixed header length: magic (4) + version (1) + frame counter (4) +
/// record count (1).
pub(crate) const DIGEST_HEADER_LEN: usize = 10;
/// Fixed trailer length: framebuffer hash (4).
pub(crate) const DIGEST_TRAILER_LEN: usize = 4;
/// Header + trailer overhead = 14 bytes.
pub(crate) const DIGEST_FIXED_OVERHEAD: usize = DIGEST_HEADER_LEN + DIGEST_TRAILER_LEN;

/// Maximum QR Version 5 / ECC Low byte-mode capacity, in bytes.
/// Derived from QR Code 2005 spec, Table 7: V5 byte-mode capacity by
/// ECC level is L=106, M=84, Q=60, H=46. The renderer's
/// `draw_digest` configures the encoder for V5/Low to match; if you
/// change one, you must change the other. `Renderer::draw_digest`'s
/// doc comment carries the wider rationale (ECC trade-off, mismatch
/// hazard, panic-on-oversize behaviour).
pub(crate) const DIGEST_PAYLOAD_CAPACITY: usize = 106;

// Pin the capacity to the V5/Low spec figure. `QrCodeEcc::Low` in
// `Renderer::draw_digest` is the other half of the pairing and is not
// const-evaluable, so the cross-file invariant cannot be enforced in
// one assertion. This half catches the common drift mode: someone
// raises the capacity (e.g. to fit more records) without realising
// they need to bump the ECC level too. Raising past 106 forces a
// re-read of the table above.
const _: () = assert!(
    DIGEST_PAYLOAD_CAPACITY <= 106,
    "DIGEST_PAYLOAD_CAPACITY exceeds QR Version 5 / ECC Low byte-mode \
     capacity (106 bytes). Either drop the capacity back to 106, or \
     bump the encoder in src/renderer/mod.rs::draw_digest to a \
     version/ECC combination that supports the new value (see QR Code \
     2005 spec, Table 7).",
);

/// CRC32C algorithm (Castagnoli polynomial, as used in iSCSI, SCTP,
/// and Btrfs). The `crc` crate computes this with a const table at
/// zero runtime cost beyond the per-byte XOR. Consumed by
/// `Renderer::crc32c_framebuffer_excluding_digest`.
pub(crate) const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// Outcome of an `encode` call.
#[derive(Debug)]
pub(crate) enum EncodeError {
    /// Caller supplied a buffer smaller than `DIGEST_PAYLOAD_CAPACITY`.
    /// Currently unreachable because the buffer type carries the
    /// capacity invariant; retained for future flexibility.
    #[allow(dead_code)]
    BufferTooSmall,
    /// Encoder bookkeeping error — should not happen if the per-variant
    /// record sizes are correct. Reported instead of panicking so a
    /// runtime accounting bug degrades to "no digest this frame"
    /// rather than a UEFI crash.
    InternalOverflow,
}

/// Map a `Phase` to its wire discriminant. The match is deliberate —
/// the wire values are stable across format-version bumps and are
/// **not** tied to Rust's default enum discriminants (which the
/// compiler may renumber if variants are reordered, added, or
/// removed). Reorder `Phase` freely; the wire still works.
fn phase_wire(phase: Phase) -> u8 {
    match phase {
        Phase::Awaiting => PHASE_AWAITING,
        Phase::Booting => PHASE_BOOTING,
        Phase::Parked => PHASE_PARKED,
    }
}

/// Map a `BootloaderChoice` to its wire discriminant. Same stability
/// rationale as `phase_wire`: the match is the contract, not Rust's
/// default reprs.
fn choice_wire(choice: BootloaderChoice) -> u8 {
    match choice {
        BootloaderChoice::Retry => CHOICE_RECOVER,
        BootloaderChoice::Ignore => CHOICE_IGNORE,
        BootloaderChoice::Abort => CHOICE_ANYWAY,
    }
}

/// Total on-wire byte size of a single record (including its 2-byte
/// type-tag + length-of-value overhead). Per the format spec table:
///
/// | Variant              | Bytes |
/// |----------------------|-------|
/// | `Keypress`           | 14    |
/// | `LineRendered`       | 12    |
/// | `SceneTransition`    | 12    |
/// | `BootloaderDecision` | 15    |
/// | `PasteReceived`      | 13    |
/// | `BootloaderTimeout`  | 10    |
/// | `ModeSwitch`         | 18    |
/// | `ModeCycle`          | 15    |
fn size_of_record(event: &Event) -> usize {
    match event {
        Event::Keypress { .. } => 14,
        Event::LineRendered { .. } => 12,
        Event::SceneTransition { .. } => 12,
        Event::BootloaderDecision { .. } => 15,
        Event::PasteReceived { .. } => 13,
        Event::BootloaderTimeout { .. } => 10,
        Event::ModeSwitch { .. } => 18,
        Event::ModeCycle { .. } => 15,
    }
}

/// Encode a digest payload into `out`. Returns the number of bytes
/// written.
///
/// Algorithm:
///
/// 1. Collect references to every event in the ring (chronological).
/// 2. Walk newest-to-oldest, summing per-record sizes, until adding
///    one more would exceed the raw-event record budget (44 bytes
///    after subtracting the 48-byte rolling-hash block). Remember the
///    oldest included index.
/// 3. Write the 10-byte header: magic, version, frame counter (u32
///    LE), total record count (u8, hash records + raw records).
/// 4. Write the 8 per-channel rolling-hash records in tag-numeric
///    order (0x11..=0x18), immediately after the header. Each record
///    is 6 bytes: tag (1) + len=4 (1) + CRC32C value (4, LE).
/// 5. Write the included raw-event records in chronological (forward)
///    order.
/// 6. Write the 4-byte trailer: `framebuffer_hash` as u32 LE.
///
/// Wire layout (v2):
///   [10-byte header]
///   [8 × 6-byte hash records   = 48 bytes]
///   [raw event records          ≤ 44 bytes]
///   [4-byte trailer]
///   Total ≤ 106 bytes (DIGEST_PAYLOAD_CAPACITY, V5/L).
///
/// Pure function: no IO, no clock reads, no `&mut Renderer`.
pub(crate) fn encode(
    ring: &RingBuffer<256>,
    frame_counter: u32,
    framebuffer_hash: u32,
    channel_hashes: &ChannelHashes,
    out: &mut [u8; DIGEST_PAYLOAD_CAPACITY],
) -> Result<usize, EncodeError> {
    // Stack-allocated index buffer. `Option<&Event>` is two
    // pointer-words; 256 entries is ~4 KB on a 64-bit target. Our
    // UEFI stack is comfortably larger than that.
    let mut events: [Option<&Event>; 256] = [None; 256];
    let mut total: usize = 0;
    for event in ring.iter() {
        if total >= events.len() {
            // Ring only holds 256 events by construction; this branch
            // is defensive against future ring resizing.
            return Err(EncodeError::InternalOverflow);
        }
        events[total] = Some(event);
        total += 1;
    }

    // Walk newest-to-oldest, selecting the most-recent run of events
    // that fits in the raw-event record budget. The hash block (8 ×
    // RECORD_HASH_SIZE = 48 bytes) is deducted from the budget so
    // raw records never displace hash records.
    const HASH_BLOCK_BYTES: usize = NUM_HASH_CHANNELS * RECORD_HASH_SIZE; // 48
    const RECORD_BUDGET: usize = DIGEST_PAYLOAD_CAPACITY - DIGEST_FIXED_OVERHEAD - HASH_BLOCK_BYTES; // 44
    let mut record_bytes: usize = 0;
    let mut included: usize = 0;
    for slot in events[..total].iter().rev() {
        let Some(event) = slot else {
            return Err(EncodeError::InternalOverflow);
        };
        let sz = size_of_record(event);
        if record_bytes + sz > RECORD_BUDGET {
            break;
        }
        record_bytes += sz;
        included += 1;
        if included == 255 {
            // Record count is a u8; cap at 255 - NUM_HASH_CHANNELS to
            // leave space for the hash records in the count field.
            break;
        }
    }

    let first_idx = total - included;
    // Total record count for the header includes both hash records and
    // raw event records.
    let total_records = NUM_HASH_CHANNELS + included;
    let total_len = DIGEST_HEADER_LEN + HASH_BLOCK_BYTES + record_bytes + DIGEST_TRAILER_LEN;
    if total_len > out.len() {
        return Err(EncodeError::InternalOverflow);
    }

    // Header.
    let mut pos: usize = 0;
    out[pos..pos + 4].copy_from_slice(&DIGEST_MAGIC);
    pos += 4;
    out[pos] = DIGEST_SCHEMA_VERSION;
    pos += 1;
    out[pos..pos + 4].copy_from_slice(&frame_counter.to_le_bytes());
    pos += 4;
    // Record count covers hash records + raw event records.
    out[pos] = total_records as u8;
    pos += 1;

    // Per-channel rolling-hash records in tag-numeric order
    // (0x11..=0x18). Each record: tag (1) + len=4 (1) + hash (4 LE).
    // These appear before raw event records so they survive any
    // capacity constraint; the raw-event budget is already reduced
    // by HASH_BLOCK_BYTES above.
    let hash_channels: [(u8, u32); NUM_HASH_CHANNELS] = [
        (TAG_HASH_KEYPRESS, channel_hashes.keypress),
        (TAG_HASH_LINE_RENDERED, channel_hashes.line_rendered),
        (TAG_HASH_SCENE_TRANSITION, channel_hashes.scene_transition),
        (
            TAG_HASH_BOOTLOADER_DECISION,
            channel_hashes.bootloader_decision,
        ),
        (TAG_HASH_PASTE_RECEIVED, channel_hashes.paste_received),
        (
            TAG_HASH_BOOTLOADER_TIMEOUT,
            channel_hashes.bootloader_timeout,
        ),
        (TAG_HASH_MODE_SWITCH, channel_hashes.mode_switch),
        (TAG_HASH_MODE_CYCLE, channel_hashes.mode_cycle),
    ];
    for (tag, hash) in &hash_channels {
        if pos + RECORD_HASH_SIZE > out.len() {
            return Err(EncodeError::InternalOverflow);
        }
        out[pos] = *tag;
        out[pos + 1] = 4; // length of value field
        out[pos + 2..pos + 6].copy_from_slice(&hash.to_le_bytes());
        pos += RECORD_HASH_SIZE;
    }

    // Raw event records, in chronological (forward) order.
    for slot in &events[first_idx..total] {
        let Some(event) = slot else {
            return Err(EncodeError::InternalOverflow);
        };
        pos = write_record(event, out, pos)?;
    }

    // Trailer: framebuffer hash, u32 LE.
    if pos + DIGEST_TRAILER_LEN > out.len() {
        return Err(EncodeError::InternalOverflow);
    }
    out[pos..pos + 4].copy_from_slice(&framebuffer_hash.to_le_bytes());
    pos += 4;

    Ok(pos)
}

/// Maximum on-wire size of a single TLV record across all event
/// variants. Used as the stack-buffer size in `event_tlv_bytes`.
/// `ModeSwitch` is the largest at 18 bytes.
pub(crate) const MAX_RECORD_SIZE: usize = 18;

// Compile-time guard: keep `MAX_RECORD_SIZE` in step with the
// largest arm of `size_of_record`. `event_tlv_bytes` writes
// per-variant byte offsets directly into `[u8; MAX_RECORD_SIZE]`;
// if a new variant exceeds this and `MAX_RECORD_SIZE` is not
// raised in step, the indexed writes panic on slice bounds. The
// failure mode is a panic-on-bounds (not memory corruption — the
// typed array length is the bound), but a compile-time check
// catches the drift before it ships. If you grow a variant or
// add one, raise the literal here AND in `MAX_RECORD_SIZE` above.
const _: () = assert!(
    MAX_RECORD_SIZE >= 18,
    "MAX_RECORD_SIZE must accommodate the largest size_of_record \
     arm. ModeSwitch is currently the largest at 18 bytes; update \
     both this assertion and MAX_RECORD_SIZE when adding or growing \
     an Event variant.",
);

/// Encode a single event as a TLV record into `buf`. Returns the
/// number of bytes written. The buffer must be at least
/// `MAX_RECORD_SIZE` bytes (18); the caller provides it as a fixed
/// `[u8; MAX_RECORD_SIZE]` so the size invariant is enforced at
/// compile time.
///
/// This is the single source of truth for "what bytes does a given
/// event produce on the wire". Both `write_record` (the encoder) and
/// `ChannelHashes::update` (the rolling-hash updater) call through
/// here so the bytes they see are identical by construction — drift
/// between "what was encoded" and "what was hashed" is prevented at
/// the API boundary rather than by convention.
pub(crate) fn event_tlv_bytes(event: &Event, buf: &mut [u8; MAX_RECORD_SIZE]) -> usize {
    let total = size_of_record(event);
    let value_len = (total - 2) as u8;
    match event {
        Event::Keypress {
            unicode,
            scancode,
            timestamp_ms,
        } => {
            buf[0] = TAG_KEYPRESS;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            // `char as u32` then truncate to u16. Production input is
            // ASCII; supplementary-plane code points would be lossy but
            // this codebase never emits them.
            let unicode_u16 = (*unicode as u32) as u16;
            buf[10..12].copy_from_slice(&unicode_u16.to_le_bytes());
            buf[12..14].copy_from_slice(&scancode.to_le_bytes());
            14
        }
        Event::LineRendered { row, timestamp_ms } => {
            buf[0] = TAG_LINE_RENDERED;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            let row_u16 = *row as u16;
            buf[10..12].copy_from_slice(&row_u16.to_le_bytes());
            12
        }
        Event::SceneTransition {
            from,
            to,
            timestamp_ms,
        } => {
            buf[0] = TAG_SCENE_TRANSITION;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            buf[10] = phase_wire(*from);
            buf[11] = phase_wire(*to);
            12
        }
        Event::BootloaderDecision {
            choice,
            attempt,
            timestamp_ms,
        } => {
            buf[0] = TAG_BOOTLOADER_DECISION;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            buf[10] = choice_wire(*choice);
            buf[11..15].copy_from_slice(&attempt.to_le_bytes());
            15
        }
        Event::PasteReceived {
            len,
            correct,
            timestamp_ms,
        } => {
            buf[0] = TAG_PASTE_RECEIVED;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            let len_u16 = *len as u16;
            buf[10..12].copy_from_slice(&len_u16.to_le_bytes());
            buf[12] = u8::from(*correct);
            13
        }
        Event::BootloaderTimeout { timestamp_ms } => {
            buf[0] = TAG_BOOTLOADER_TIMEOUT;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            10
        }
        Event::ModeSwitch {
            requested_w,
            requested_h,
            applied_w,
            applied_h,
            timestamp_ms,
        } => {
            buf[0] = TAG_MODE_SWITCH;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            // OVMF modes are well under u16::MAX; truncation is safe.
            let rw = *requested_w as u16;
            let rh = *requested_h as u16;
            let aw = *applied_w as u16;
            let ah = *applied_h as u16;
            buf[10..12].copy_from_slice(&rw.to_le_bytes());
            buf[12..14].copy_from_slice(&rh.to_le_bytes());
            buf[14..16].copy_from_slice(&aw.to_le_bytes());
            buf[16..18].copy_from_slice(&ah.to_le_bytes());
            18
        }
        Event::ModeCycle {
            count,
            interrupted,
            timestamp_ms,
        } => {
            buf[0] = TAG_MODE_CYCLE;
            buf[1] = value_len;
            buf[2..10].copy_from_slice(&timestamp_ms.to_le_bytes());
            buf[10..14].copy_from_slice(&count.to_le_bytes());
            buf[14] = u8::from(*interrupted);
            15
        }
    }
}

/// Per-channel rolling CRC32C accumulators; one per event variant.
///
/// Each field accumulates CRC32C over every TLV-encoded event of that
/// variant since boot, in push order. The hash is updated on every
/// `push_event` call and stored as the _finalized_ CRC32C value
/// (i.e., the value `Digest::finalize()` returns). An empty channel
/// carries `0x00000000` — the CRC32C of zero bytes.
///
/// ## Chaining invariant
///
/// CRC_32_ISCSI (`refin=true`, `refout=true`, `xorout=0xFFFFFFFF`):
/// given a finalized value `f`, the internal pre-finalization state is
/// `raw = f ^ 0xFFFF_FFFF`. Because `init()` applies
/// `initial.reverse_bits()` (for `refin=true`), the correct argument
/// to `Crc::digest_with_initial` for resuming from `f` is
/// `(f ^ 0xFFFF_FFFF).reverse_bits()`. The `update_channel` method
/// encapsulates this so callers stay algorithm-agnostic.
///
/// ## Field naming
///
/// Named fields (one per `Event` variant) rather than an indexed
/// array. Named fields are self-documenting at each call site, let
/// the compiler enforce completeness in `update`, and make step 2c's
/// encoder read-path (`channel_hashes.keypress`, etc.) explicit.
/// The tag-indexed array alternative would require a safe mapping from
/// `TAG_*` constants to array indices; the named approach avoids that
/// indirection at the cost of eight field names instead of an indexing
/// expression.
///
/// ## Reachability
///
/// All eight fields are read by `ChannelHashes::update` (write path)
/// and will be consumed by the step-2c TLV encoder. No
/// `#[allow(dead_code)]` annotation is added; if a field appears dead
/// before 2c lands, that is expected and benign — the compiler will
/// not warn because `update` writes every field on every matching push.
pub(crate) struct ChannelHashes {
    /// Running CRC32C over all `Event::Keypress` TLV records (tag 0x01).
    pub(crate) keypress: u32,
    /// Running CRC32C over all `Event::LineRendered` TLV records (tag 0x02).
    pub(crate) line_rendered: u32,
    /// Running CRC32C over all `Event::SceneTransition` TLV records (tag 0x03).
    pub(crate) scene_transition: u32,
    /// Running CRC32C over all `Event::BootloaderDecision` TLV records (tag 0x04).
    pub(crate) bootloader_decision: u32,
    /// Running CRC32C over all `Event::PasteReceived` TLV records (tag 0x05).
    pub(crate) paste_received: u32,
    /// Running CRC32C over all `Event::BootloaderTimeout` TLV records (tag 0x06).
    pub(crate) bootloader_timeout: u32,
    /// Running CRC32C over all `Event::ModeSwitch` TLV records (tag 0x07).
    pub(crate) mode_switch: u32,
    /// Running CRC32C over all `Event::ModeCycle` TLV records (tag 0x08).
    pub(crate) mode_cycle: u32,
}

impl ChannelHashes {
    /// Initialise all eight accumulators to the CRC32C of zero bytes
    /// (`0x00000000` — the finalized value of an empty `Digest`).
    pub(crate) const fn new() -> Self {
        Self {
            keypress: 0,
            line_rendered: 0,
            scene_transition: 0,
            bootloader_decision: 0,
            paste_received: 0,
            bootloader_timeout: 0,
            mode_switch: 0,
            mode_cycle: 0,
        }
    }

    /// Compute the `digest_with_initial` argument required to resume a
    /// CRC_32_ISCSI (`refin=true`, `refout=true`, `xorout=0xFFFF_FFFF`)
    /// stream from a previously-finalized value.
    ///
    /// `finalize` applied `raw ^ xorout`, so `raw = f ^ xorout`.
    /// `init` applies `initial.reverse_bits()` (for `refin=true`), so
    /// the `digest_with_initial` argument is `raw.reverse_bits()`.
    #[inline]
    fn resume_initial(finalized: u32) -> u32 {
        (finalized ^ 0xFFFF_FFFF).reverse_bits()
    }

    /// Extend one channel's running CRC32C with the TLV bytes of
    /// `event` and return the new finalized value.
    #[inline]
    fn extend(current: u32, event: &Event) -> u32 {
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let len = event_tlv_bytes(event, &mut buf);
        let mut d = CRC32C.digest_with_initial(Self::resume_initial(current));
        d.update(&buf[..len]);
        d.finalize()
    }

    /// Update the accumulator for the channel matching `event`'s variant.
    ///
    /// Dispatches on the event variant and extends the corresponding
    /// field via `extend`. Every variant is covered so the compiler
    /// enforces completeness; if a new `Event` variant is added without
    /// updating this match, the code will not compile.
    pub(crate) fn update(&mut self, event: &Event) {
        match event {
            Event::Keypress { .. } => {
                self.keypress = Self::extend(self.keypress, event);
            }
            Event::LineRendered { .. } => {
                self.line_rendered = Self::extend(self.line_rendered, event);
            }
            Event::SceneTransition { .. } => {
                self.scene_transition = Self::extend(self.scene_transition, event);
            }
            Event::BootloaderDecision { .. } => {
                self.bootloader_decision = Self::extend(self.bootloader_decision, event);
            }
            Event::PasteReceived { .. } => {
                self.paste_received = Self::extend(self.paste_received, event);
            }
            Event::BootloaderTimeout { .. } => {
                self.bootloader_timeout = Self::extend(self.bootloader_timeout, event);
            }
            Event::ModeSwitch { .. } => {
                self.mode_switch = Self::extend(self.mode_switch, event);
            }
            Event::ModeCycle { .. } => {
                self.mode_cycle = Self::extend(self.mode_cycle, event);
            }
        }
    }
}

/// Write a single TLV record into `out` starting at `pos`. Returns
/// the new write position. Errors with `InternalOverflow` if the
/// caller's bookkeeping was off (should never happen — `encode`
/// validates the total length up front).
fn write_record(
    event: &Event,
    out: &mut [u8; DIGEST_PAYLOAD_CAPACITY],
    pos: usize,
) -> Result<usize, EncodeError> {
    let total = size_of_record(event);
    if pos + total > out.len() {
        return Err(EncodeError::InternalOverflow);
    }
    let mut buf = [0u8; MAX_RECORD_SIZE];
    let written = event_tlv_bytes(event, &mut buf);
    out[pos..pos + written].copy_from_slice(&buf[..written]);
    Ok(pos + written)
}

#[cfg(test)]
mod tests {
    //! Host-side unit tests for the pure pieces of the digest module.
    //!
    //! These cover the encoder regression net for `event_tlv_bytes`
    //! (one per `Event` variant, asserting exact wire bytes) and the
    //! CRC32C chaining math used by `ChannelHashes::extend` /
    //! `ChannelHashes::resume_initial`. The QEMU digest-payload smoke
    //! still gates UEFI-side behaviour; these tests localise failures
    //! in the pure functions so a regression there doesn't masquerade
    //! as a renderer or scene bug.
    use super::*;
    use crate::event::{BootloaderChoice, Event, Phase};

    /// `Keypress` TLV: tag 0x01, len 0x0c, timestamp_ms LE (8),
    /// unicode u16 LE (2), scancode u16 LE (2). Total 14 bytes.
    #[test]
    fn keypress_encodes_to_expected_bytes() {
        let event = Event::Keypress {
            unicode: 'A',
            scancode: 0x1234,
            timestamp_ms: 0x0102_0304_0506_0708,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 14);
        assert_eq!(
            &buf[..14],
            &[
                TAG_KEYPRESS,
                0x0c, // value length: total (14) - tag/len overhead (2)
                0x08,
                0x07,
                0x06,
                0x05,
                0x04,
                0x03,
                0x02,
                0x01, // timestamp LE
                0x41,
                0x00, // 'A' as u16 LE
                0x34,
                0x12, // scancode 0x1234 LE
            ]
        );
    }

    /// `LineRendered` TLV: tag 0x02, len 0x0a, timestamp_ms LE (8),
    /// row u16 LE (2). Total 12 bytes.
    #[test]
    fn line_rendered_encodes_to_expected_bytes() {
        let event = Event::LineRendered {
            row: 0x00ab,
            timestamp_ms: 0x1122_3344_5566_7788,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 12);
        assert_eq!(
            &buf[..12],
            &[
                TAG_LINE_RENDERED,
                0x0a,
                0x88,
                0x77,
                0x66,
                0x55,
                0x44,
                0x33,
                0x22,
                0x11, // timestamp LE
                0xab,
                0x00, // row LE
            ]
        );
    }

    /// `SceneTransition` TLV: tag 0x03, len 0x0a, timestamp_ms LE (8),
    /// from u8, to u8. Total 12 bytes. Phase discriminants are matched
    /// by `phase_wire` (Awaiting=0, Booting=1, Parked=2).
    #[test]
    fn scene_transition_encodes_to_expected_bytes() {
        let event = Event::SceneTransition {
            from: Phase::Awaiting,
            to: Phase::Booting,
            timestamp_ms: 0x0000_0000_0000_002a,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 12);
        assert_eq!(
            &buf[..12],
            &[
                TAG_SCENE_TRANSITION,
                0x0a,
                0x2a,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00, // timestamp LE
                PHASE_AWAITING,
                PHASE_BOOTING,
            ]
        );
    }

    /// `BootloaderDecision` TLV: tag 0x04, len 0x0d, timestamp_ms LE
    /// (8), choice u8, attempt u32 LE. Total 15 bytes.
    #[test]
    fn bootloader_decision_encodes_to_expected_bytes() {
        let event = Event::BootloaderDecision {
            choice: BootloaderChoice::Ignore,
            attempt: 0xdead_beef,
            timestamp_ms: 0x0000_0000_0000_0001,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 15);
        assert_eq!(
            &buf[..15],
            &[
                TAG_BOOTLOADER_DECISION,
                0x0d,
                0x01,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00, // timestamp LE
                CHOICE_IGNORE,
                0xef,
                0xbe,
                0xad,
                0xde, // attempt LE
            ]
        );
    }

    /// `PasteReceived` TLV: tag 0x05, len 0x0b, timestamp_ms LE (8),
    /// len u16 LE, correct u8. Total 13 bytes.
    #[test]
    fn paste_received_encodes_to_expected_bytes() {
        let event = Event::PasteReceived {
            len: 0x0102,
            correct: true,
            timestamp_ms: 0x0000_0000_0000_0009,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 13);
        assert_eq!(
            &buf[..13],
            &[
                TAG_PASTE_RECEIVED,
                0x0b,
                0x09,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00, // timestamp LE
                0x02,
                0x01, // len LE
                0x01, // correct
            ]
        );
    }

    /// `BootloaderTimeout` TLV: tag 0x06, len 0x08, timestamp_ms LE
    /// (8). Total 10 bytes.
    #[test]
    fn bootloader_timeout_encodes_to_expected_bytes() {
        let event = Event::BootloaderTimeout {
            timestamp_ms: 0xffff_ffff_ffff_ffff,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 10);
        assert_eq!(
            &buf[..10],
            &[
                TAG_BOOTLOADER_TIMEOUT,
                0x08,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff, // timestamp LE
            ]
        );
    }

    /// `ModeSwitch` TLV: tag 0x07, len 0x10, timestamp_ms LE (8),
    /// requested_w/h u16 LE, applied_w/h u16 LE. Total 18 bytes.
    #[test]
    fn mode_switch_encodes_to_expected_bytes() {
        let event = Event::ModeSwitch {
            requested_w: 1024,
            requested_h: 768,
            applied_w: 800,
            applied_h: 600,
            timestamp_ms: 0x0000_0000_0000_0003,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 18);
        assert_eq!(
            &buf[..18],
            &[
                TAG_MODE_SWITCH,
                0x10,
                0x03,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00, // timestamp LE
                0x00,
                0x04, // requested_w 1024 LE
                0x00,
                0x03, // requested_h 768 LE
                0x20,
                0x03, // applied_w 800 LE
                0x58,
                0x02, // applied_h 600 LE
            ]
        );
    }

    /// `ModeCycle` TLV: tag 0x08, len 0x0d, timestamp_ms LE (8),
    /// count u32 LE, interrupted u8. Total 15 bytes.
    #[test]
    fn mode_cycle_encodes_to_expected_bytes() {
        let event = Event::ModeCycle {
            count: 0x0000_0007,
            interrupted: false,
            timestamp_ms: 0x0000_0000_0000_0005,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let written = event_tlv_bytes(&event, &mut buf);
        assert_eq!(written, 15);
        assert_eq!(
            &buf[..15],
            &[
                TAG_MODE_CYCLE,
                0x0d,
                0x05,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00, // timestamp LE
                0x07,
                0x00,
                0x00,
                0x00, // count LE
                0x00, // interrupted = false
            ]
        );
    }

    /// `resume_initial` must round-trip: a `Digest` resumed from a
    /// finalized value `f` and fed zero bytes must re-finalize to
    /// `f`. This is the algebraic identity that proves the formula
    /// `(f ^ 0xFFFF_FFFF).reverse_bits()` correct against the
    /// `CRC_32_ISCSI` parameters (`refin=true`, `refout=true`,
    /// `xorout=0xFFFF_FFFF`).
    #[test]
    fn resume_initial_round_trips_finalized_value() {
        // A spread of representative values: empty-stream sentinel,
        // a small one, a typical 32-bit value, and the all-ones edge.
        for f in [0x0000_0000_u32, 0x0000_0001, 0xdead_beef, 0xffff_ffff] {
            let mut d = CRC32C.digest_with_initial(ChannelHashes::resume_initial(f));
            d.update(&[]);
            assert_eq!(d.finalize(), f, "round-trip failed for f=0x{:08x}", f);
        }
    }

    /// CRC32C chaining must agree with a single-pass CRC32C over the
    /// concatenated bytes. Split a known byte string at every
    /// internal boundary and verify that resuming from the first
    /// half's finalized value, then feeding the second half, yields
    /// the same result as a single-pass digest of the whole string.
    /// This guards `ChannelHashes::extend`'s "resume + feed" pattern
    /// against any future drift in the `resume_initial` formula.
    #[test]
    fn chained_crc32c_matches_single_pass() {
        // "123456789" is the canonical CRC test vector; CRC32C
        // (CRC_32_ISCSI) of it is 0xe3069283. We don't hard-code that
        // here — `Crc::checksum` of the whole string is the oracle.
        let message: &[u8] = b"123456789";
        let one_shot = CRC32C.checksum(message);

        for split in 0..=message.len() {
            let (left, right) = message.split_at(split);
            let left_finalized = {
                let mut d = CRC32C.digest();
                d.update(left);
                d.finalize()
            };
            let chained = {
                let mut d =
                    CRC32C.digest_with_initial(ChannelHashes::resume_initial(left_finalized));
                d.update(right);
                d.finalize()
            };
            assert_eq!(
                chained, one_shot,
                "chained digest mismatch at split={}: chained=0x{:08x} expected=0x{:08x}",
                split, chained, one_shot,
            );
        }
    }

    /// `ChannelHashes::extend` over a single event must equal a
    /// fresh CRC32C of that event's TLV bytes. This pins the
    /// "extend == CRC32C over TLV bytes" contract that the on-wire
    /// per-channel hash records depend on.
    #[test]
    fn extend_single_event_matches_one_shot_crc() {
        let event = Event::Keypress {
            unicode: 'k',
            scancode: 0x0042,
            timestamp_ms: 0x0000_0000_1234_5678,
        };
        let mut buf = [0u8; MAX_RECORD_SIZE];
        let len = event_tlv_bytes(&event, &mut buf);

        let expected = CRC32C.checksum(&buf[..len]);
        let actual = ChannelHashes::extend(0, &event);
        assert_eq!(actual, expected);
    }

    /// `ChannelHashes::extend` over two events must equal a fresh
    /// CRC32C over the concatenated TLV bytes. This is the chaining
    /// property in production form: two `extend` calls compose into
    /// one running hash over the per-channel byte stream.
    #[test]
    fn extend_two_events_matches_concatenated_one_shot_crc() {
        let first = Event::Keypress {
            unicode: 'a',
            scancode: 0x0001,
            timestamp_ms: 0x0000_0000_0000_0010,
        };
        let second = Event::Keypress {
            unicode: 'b',
            scancode: 0x0002,
            timestamp_ms: 0x0000_0000_0000_0020,
        };

        let mut buf1 = [0u8; MAX_RECORD_SIZE];
        let len1 = event_tlv_bytes(&first, &mut buf1);
        let mut buf2 = [0u8; MAX_RECORD_SIZE];
        let len2 = event_tlv_bytes(&second, &mut buf2);

        let mut concat = [0u8; MAX_RECORD_SIZE * 2];
        concat[..len1].copy_from_slice(&buf1[..len1]);
        concat[len1..len1 + len2].copy_from_slice(&buf2[..len2]);
        let expected = CRC32C.checksum(&concat[..len1 + len2]);

        let after_first = ChannelHashes::extend(0, &first);
        let after_second = ChannelHashes::extend(after_first, &second);

        assert_eq!(after_second, expected);
    }

    /// Sanity check against a precomputed CRC32C reference value.
    /// "123456789" -> 0xe3069283 (per the CRC_32_ISCSI / CRC-32C
    /// reference in the CRC catalogue). If the underlying `crc`
    /// crate ever silently swapped algorithms, this test fails
    /// loudly rather than letting downstream chaining tests pass
    /// against an off-spec checksum.
    #[test]
    fn crc32c_known_vector() {
        assert_eq!(CRC32C.checksum(b"123456789"), 0xe306_9283);
    }
}
