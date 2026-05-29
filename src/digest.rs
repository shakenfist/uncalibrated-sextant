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

/// Schema version of the wire format. Bump when a field shape changes
/// or a TLV type is repurposed; adding a new TLV type does not require
/// a bump (TLV's whole point).
pub(crate) const DIGEST_SCHEMA_VERSION: u8 = 0x01;

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
///    one more would exceed the 92-byte record budget. Remember the
///    oldest included index.
/// 3. Write the 10-byte header in forward order: magic, version,
///    frame counter (u32 LE), record count (u8).
/// 4. Write the included records in chronological (forward) order.
/// 5. Write the 4-byte trailer: `framebuffer_hash` as u32 LE.
///
/// Pure function: no IO, no clock reads, no `&mut Renderer`. Step 2c
/// computes `framebuffer_hash` against the framebuffer's non-digest
/// pixels and passes it in.
pub(crate) fn encode(
    ring: &RingBuffer<256>,
    frame_counter: u32,
    framebuffer_hash: u32,
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
    // that fits in the record budget.
    const RECORD_BUDGET: usize = DIGEST_PAYLOAD_CAPACITY - DIGEST_FIXED_OVERHEAD;
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
            // Record count is a u8.
            break;
        }
    }

    let first_idx = total - included;
    let total_len = DIGEST_HEADER_LEN + record_bytes + DIGEST_TRAILER_LEN;
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
    out[pos] = included as u8;
    pos += 1;

    // Records, in chronological (forward) order.
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
