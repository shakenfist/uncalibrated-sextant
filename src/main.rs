// uncalibrated-sextant: SPICE channel exercise harness.
//
// Phase 4 entry point. The Phase 1 SimpleTextOutput banner is replaced
// by a GOP-backed renderer.  See DESIGN.md and
// docs/plans/PLAN-first-playable-phase-04-renderer.md for context.

#![no_main]
#![no_std]

mod renderer;

use core::time::Duration;
use renderer::Renderer;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    let mut r = Renderer::new().expect("renderer init failed");

    run_test_scaffold(&mut r);

    // Block until a key is pressed, using the canonical UEFI
    // wait_for_event pattern rather than polling read_key.
    uefi::system::with_stdin(|stdin| {
        let event = stdin.wait_for_key_event().unwrap();
        uefi::boot::wait_for_event(&mut [event]).unwrap();
        let _ = stdin.read_key();
    });

    // Ask the platform to shut down. QEMU translates ResetType::SHUTDOWN
    // into an ACPI shutdown and exits cleanly.
    uefi::runtime::reset(uefi::runtime::ResetType::SHUTDOWN, Status::SUCCESS, None)
}

/// Phase 4 test scaffold: render three telemetry lines to confirm
/// GOP rendering, phosphor palette, dot-leader alignment, and pacing.
///
/// Phase 5 replaces this function with the real boot-sequence content.
fn run_test_scaffold(r: &mut Renderer) {
    r.draw_telemetry_line("DISPLAY SUBSYSTEM", "OK", 2);
    uefi::boot::stall(Duration::from_millis(200));

    r.draw_telemetry_line("POINTER", "OK", 3);
    uefi::boot::stall(Duration::from_millis(200));

    r.draw_telemetry_line("KEYBOARD", "OK", 4);
    uefi::boot::stall(Duration::from_millis(200));

    r.draw_line("PRESS ANY KEY TO CONTINUE", 6);
}
