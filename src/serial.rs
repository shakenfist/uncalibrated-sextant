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
use crate::scene::RefreshStats;

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

/// Emit a one-line dump of every GOP mode the firmware exposes.
///
/// Written once at boot, immediately after the renderer is initialised,
/// so the full list is visible in `dist/serial.log` without requiring
/// interactive keystrokes. Format:
///
///   available GOP modes: 640x480 800x600 1024x768 ...
pub fn write_available_modes<I>(modes: I)
where
    I: IntoIterator<Item = (usize, usize)>,
{
    with_serial(|serial| {
        let _ = write!(serial, "available GOP modes:");
        for (w, h) in modes {
            let _ = write!(serial, " {w}x{h}");
        }
        let _ = writeln!(serial, "\r");
    });
}

/// One-shot plain-text dump of the event ring buffer followed by a
/// `refresh_stats` summary line.
///
/// Called immediately before ACPI shutdown. Format: one line per event,
/// CRLF-terminated, chronological order. Stable lowercase tags for the
/// `type=` and phase fields so Ryll's future parser can match literally.
///
/// The final line is always:
/// ```text
/// type=refresh_stats count=<n> total_ms=<n> mean_us=<n> max_us=<n> p99_us=<n>
/// ```
/// When `ticks_per_ms` is zero (calibration was skipped or overflowed),
/// all derived values are emitted as zero.
pub fn drain<const N: usize>(ring: &RingBuffer<N>, stats: &RefreshStats, ticks_per_ms: u64) {
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
                Event::ModeSwitch {
                    requested_w,
                    requested_h,
                    applied_w,
                    applied_h,
                    timestamp_ms,
                } => {
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=mode_switch requested={requested_w}x{requested_h} \
                         applied={applied_w}x{applied_h}\r",
                    );
                }
                Event::ModeCycle {
                    count,
                    interrupted,
                    timestamp_ms,
                } => {
                    let _ = writeln!(
                        serial,
                        "t={timestamp_ms} type=mode_cycle count={count} interrupted={interrupted}\r",
                    );
                }
            }
        }

        // Emit the refresh_stats summary line. All time values are zero
        // when the call count is zero or the calibration returned zero.
        let (total_ms, mean_us, max_us, p99_us) = if stats.count == 0 || ticks_per_ms == 0 {
            (0u64, 0u64, 0u64, 0u64)
        } else {
            let total_ms = stats.total_ticks / ticks_per_ms;
            let mean_us = (stats.total_ticks * 1000) / (ticks_per_ms * stats.count as u64);
            let max_us = stats.max_ticks * 1000 / ticks_per_ms;

            // p99 from the sample ring. Sort a stack copy; only the first
            // min(count, 256) slots are valid.
            let valid = (stats.count as usize).min(256);
            let mut ring_copy = [0u64; 256];
            ring_copy[..valid].copy_from_slice(&stats.sample_ring[..valid]);
            let valid_slice = &mut ring_copy[..valid];
            valid_slice.sort_unstable();
            let p99_idx = (valid * 99 / 100).saturating_sub(1);
            let p99_ticks = valid_slice[p99_idx];
            let p99_us = p99_ticks * 1000 / ticks_per_ms;

            (total_ms, mean_us, max_us, p99_us)
        };

        let count = stats.count;
        let _ = writeln!(
            serial,
            "type=refresh_stats count={count} total_ms={total_ms} \
             mean_us={mean_us} max_us={max_us} p99_us={p99_us}\r",
        );
    });
}
