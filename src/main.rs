// uncalibrated-sextant: SPICE channel exercise harness.
//
// Phase 5 entry point. Replaces the Phase 4 test scaffold with the
// real boot-sequence scene. See DESIGN.md and
// docs/plans/PLAN-first-playable-phase-05-boot-sequence.md.

#![no_main]
#![no_std]

mod cursor;
mod event;
mod logo;
mod renderer;
mod scene;

use renderer::Renderer;
use scene::Scene;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    let mut r = Renderer::new().expect("renderer init failed");
    let mut scene = Scene::new();
    scene.run(&mut r)
    // scene.run is `-> !` (ends with ACPI shutdown), so this is
    // unreachable; the return type annotation is kept to satisfy
    // the #[entry] signature requirement.
}
