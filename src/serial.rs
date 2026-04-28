// Serial protocol writers for uncalibrated-sextant.
//
// Two consumers for now: a one-line startup banner emitted before the
// scene runs (so `make release-verify` has something to grep for), and
// the end-of-scene event ring buffer drain. Both acquire the Serial
// protocol briefly and drop it — no long-lived handle held across the
// scene, which would otherwise need to be threaded through `Scene`.
//
// Silent no-op if no Serial protocol is present: under QEMU configs
// without a -serial backend the scene must still play and shut down
// cleanly. Callers cannot tell the difference and intentionally so.

use core::fmt::Write;

use uefi::proto::console::serial::Serial;

use crate::event::{Event, RingBuffer};

/// Briefly acquire the Serial protocol, hand it to `f`, drop it on exit.
fn with_serial<F>(f: F)
where
    F: FnOnce(&mut Serial),
{
    let Ok(handle) = uefi::boot::get_handle_for_protocol::<Serial>() else {
        return;
    };
    let Ok(mut serial) = uefi::boot::open_protocol_exclusive::<Serial>(handle) else {
        return;
    };
    f(&mut serial);
}

/// Emit a one-line startup banner to serial before the scene begins.
///
/// The exact string is what `scripts/verify-release.sh` greps for to
/// confirm the binary is reaching its entry point; do not change it
/// without updating that script.
pub fn write_startup_banner() {
    with_serial(|serial| {
        let _ = writeln!(serial, "Hello from Uncalibrated Sextant\r");
    });
}

/// One-shot plain-text dump of the event ring buffer.
///
/// Called immediately before ACPI shutdown. Format: one line per event,
/// CRLF-terminated, chronological order. Stable lowercase tags for the
/// `type=` and phase fields so Ryll's future parser can match literally.
pub fn drain<const N: usize>(ring: &RingBuffer<N>) {
    with_serial(|serial| {
        for event in ring.iter() {
            match event {
                Event::Keypress {
                    unicode,
                    scancode,
                    timestamp_ms,
                } => {
                    let unicode_hex = *unicode as u32;
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=keypress unicode={unicode_hex:04x} scancode={scancode:04x}\r",
                    );
                }
                Event::LineRendered { row, timestamp_ms } => {
                    let _ = writeln!(serial, "t={timestamp_ms} type=line row={row}\r");
                }
                Event::SceneTransition {
                    from,
                    to,
                    timestamp_ms,
                } => {
                    let from_tag = from.tag();
                    let to_tag = to.tag();
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=transition from={from_tag} to={to_tag}\r",
                    );
                }
                Event::BootloaderDecision {
                    choice,
                    attempt,
                    timestamp_ms,
                } => {
                    let tag = choice.tag();
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=bootloader_decision choice={tag} attempt={attempt}\r",
                    );
                }
                Event::PasteReceived {
                    len,
                    correct,
                    timestamp_ms,
                } => {
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=paste len={len} correct={correct}\r",
                    );
                }
                Event::BootloaderTimeout { timestamp_ms } => {
                    let _ = writeln!(serial, "t={timestamp_ms} type=bootloader_timeout\r");
                }
            }
        }
    });
}
