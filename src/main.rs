// uncalibrated-sextant: SPICE channel exercise harness.
//
// Entry point: emit a startup banner to serial so the release-verify
// headless check can confirm reach-of-main, then hand off to the scene
// state machine which owns the rest of the run (and ends with ACPI
// shutdown). See DESIGN.md and docs/plans/PLAN-first-playable.md.

#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]
// Under the test profile, only the pure modules (`digest`, `event`)
// compile; their consumers in `renderer`/`scene`/`bootloader`/etc.
// are gated out below. That leaves `pub(crate)` items in `digest`
// and `event` looking unused to the compiler — which is correct for
// the test profile but noisy. Silence the lint at-test-time only;
// production builds still catch genuine dead code.
#![cfg_attr(test, allow(dead_code))]

// UEFI-dependent modules are excluded from the test profile so host
// `cargo test` (on `x86_64-unknown-linux-gnu`) does not try to compile
// `uefi::*` against a non-UEFI target. The `event` module (pure,
// re-exports the wire-format types from `shakenfist-visual-digest`)
// stays available for `#[cfg(test)]` unit tests; the TLV encoder
// itself moved to that shared crate in step 1h, so Sextant no longer
// hosts the host-side encoder tests.
#[cfg(not(test))]
mod bootloader;
#[cfg(not(test))]
mod cursor;
mod event;
#[cfg(not(test))]
mod logo;
#[cfg(not(test))]
mod probes;
#[cfg(not(test))]
mod renderer;
#[cfg(not(test))]
mod scene;
#[cfg(not(test))]
mod serial;

#[cfg(not(test))]
use renderer::Renderer;
#[cfg(not(test))]
use scene::Scene;
#[cfg(not(test))]
use uefi::prelude::*;

#[cfg(not(test))]
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
