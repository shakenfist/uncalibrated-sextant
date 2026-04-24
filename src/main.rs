// uncalibrated-sextant: SPICE channel exercise harness.
//
// Phase 1 entry point. See DESIGN.md and
// docs/plans/PLAN-first-playable-phase-01-skeleton.md for context.
// Uses uefi crate 0.37.0 with the new (globals-based) entry-point API.

#![no_main]
#![no_std]

use core::time::Duration;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    // Clear screen.
    uefi::system::with_stdout(|stdout| stdout.clear().unwrap());

    // Print the banner.
    uefi::system::with_stdout(|stdout| {
        stdout
            .output_string(cstr16!("Hello from Uncalibrated Sextant\r\n"))
            .unwrap()
    });

    // Wait for any keypress before returning.
    loop {
        let key = uefi::system::with_stdin(|stdin| stdin.read_key());
        if let Ok(Some(_)) = key {
            break;
        }
        uefi::boot::stall(Duration::from_millis(10));
    }

    Status::SUCCESS
}
