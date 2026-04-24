// uncalibrated-sextant: SPICE channel exercise harness.
//
// Phase 1 entry point. See DESIGN.md and
// docs/plans/PLAN-first-playable-phase-01-skeleton.md for context.
// Uses uefi crate 0.37.0 with the new (globals-based) entry-point API.

#![no_main]
#![no_std]

use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    uefi::system::with_stdout(|stdout| {
        stdout.clear().unwrap();
        stdout
            .output_string(cstr16!(
                "Hello from Uncalibrated Sextant\r\n\
                 Press any key to exit.\r\n"
            ))
            .unwrap();
    });

    // Block until a key is pressed, using the canonical UEFI
    // wait_for_event pattern rather than polling read_key. The event,
    // the subsequent wait_for_event, and the drain-read all happen
    // inside a single with_stdin closure so the Event's lifetime is
    // unambiguously tied to the stdin borrow.
    uefi::system::with_stdin(|stdin| {
        let event = stdin.wait_for_key_event().unwrap();
        uefi::boot::wait_for_event(&mut [event]).unwrap();
        let _ = stdin.read_key();
    });

    // Ask the platform to shut down rather than returning control to
    // OVMF's boot manager. Returning would leave QEMU running (and
    // its GTK window with the keyboard grabbed) until the operator
    // killed it externally. QEMU translates ResetType::SHUTDOWN into
    // an ACPI shutdown and exits cleanly.
    uefi::runtime::reset(uefi::runtime::ResetType::SHUTDOWN, Status::SUCCESS, None)
}
