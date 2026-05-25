// uncalibrated-sextant: SPICE channel exercise harness.
//
// Entry point: emit a startup banner to serial so the release-verify
// headless check can confirm reach-of-main, then hand off to the scene
// state machine which owns the rest of the run (and ends with ACPI
// shutdown). See DESIGN.md and docs/plans/PLAN-first-playable.md.

#![no_main]
#![no_std]

mod bootloader;
mod cursor;
mod digest;
mod event;
mod logo;
mod probes;
mod renderer;
mod scene;
mod serial;

use renderer::Renderer;
use scene::Scene;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    serial::write_startup_banner();

    let mut r = Renderer::new().expect("renderer init failed");
    serial::write_available_modes(r.available_modes());
    let mut scene = Scene::new();
    scene.run(&mut r)
    // scene.run is `-> !` (ends with ACPI shutdown), so this is
    // unreachable; the return type annotation is kept to satisfy
    // the #[entry] signature requirement.
}
