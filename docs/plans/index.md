# Plans index

This page summarises every planning document in chronological order.
Master plans decompose work into numbered phases, each with its own
detailed plan file. Standalone plans track issues, follow-ups, or
design decisions that do not require phased execution.

New plans should follow the structure in `PLAN-TEMPLATE.md` at the
repo root. For pre-push audits of our own work see `PUSH-TEMPLATE.md`.

## Master plans

| Date | Plan | Intent | Status | Phases |
|------|------|--------|--------|--------|
| 2026-04-24 | [First playable](PLAN-first-playable.md) | Bootable UEFI binary via `make qemu` that plays the opening boot sequence in human-play mode, to prove concept and build pipeline | Complete | [1. Skeleton](PLAN-first-playable-phase-01-skeleton.md), [2. Build](PLAN-first-playable-phase-02-build.md), [3. Style](PLAN-first-playable-phase-03-style.md), [4. Renderer](PLAN-first-playable-phase-04-renderer.md), [5. Boot sequence](PLAN-first-playable-phase-05-boot-sequence.md), [6. Packaging](PLAN-first-playable-phase-06-packaging.md) |
| 2026-04-25 | [Locked bootloader](PLAN-locked-bootloader.md) | First real SPICE-channel test as a scene — clipboard paste round-trip dressed as a bootloader-decryption flow; introduces SPICE-client testing infrastructure | Complete (commits a7b261d through this closeout) | [1. SPICE infra](PLAN-locked-bootloader-phase-01-spice-infra.md), [2. Scene](PLAN-locked-bootloader-phase-02-scene.md), [3. Closeout](PLAN-locked-bootloader-phase-03-docs.md) |
| 2026-04-30 | [Display-mode keystrokes](PLAN-display-mode-keystrokes.md) | Interactive GOP mode switching as a first-class test affordance, so a human operator can drive the SPICE client (ryll) through guest mode changes at any tempo | Complete (commits 455d2b5 through this closeout) | [1. Renderer](PLAN-display-mode-keystrokes-phase-01-renderer.md), [2. Keystrokes](PLAN-display-mode-keystrokes-phase-02-keystrokes.md), [3. Closeout](PLAN-display-mode-keystrokes-phase-03-docs.md) |
| 2026-05-01 | [Audit cleanup](PLAN-audit-cleanup.md) | Bug fixes, structural dedup, and headless coverage gaps surfaced by the first PUSH-TEMPLATE.md audit run | Complete (commits 1482eb0 through this closeout) | [1. Bugs](PLAN-audit-cleanup-phase-01-bugs.md), [2. Structural](PLAN-audit-cleanup-phase-02-structural.md), [3. Tests](PLAN-audit-cleanup-phase-03-tests.md) |
| 2026-05-01 | [Visual on-screen digest](PLAN-visual-digest.md) | Implement the visual half of DESIGN.md's two-channel test architecture: render a periodic, ring-buffer-driven QR digest into the framebuffer that an external decoder (ryll) can read from a screenshot | Complete (commits 55844a5 through this closeout) | [1. Region](PLAN-visual-digest-phase-01-region.md), [2. Payload](PLAN-visual-digest-phase-02-payload.md), [3. Closeout](PLAN-visual-digest-phase-03-closeout.md) |
| 2026-05-28 | [Continuous multi-channel visual digest](PLAN-continuous-digest.md) | Convert the visual digest from a phase-boundary snapshot into a continuous per-line oracle covering every I/O surface — the substitute for the never-built second-serial gRPC channel | Not started | [1. Cadence](PLAN-continuous-digest-phase-01-cadence.md), [2. Multi-channel](PLAN-continuous-digest-phase-02-multi-channel.md), [3. Closeout](PLAN-continuous-digest-phase-03-closeout.md) |

## Standalone plans

| Date | Plan | Intent | Status |
|------|------|--------|--------|
| 2026-04-25 | [Language probes](PLAN-language-probes.md) | Worldbuilding side quest: four boot-sequence lines in Mandarin / Hindi / Spanish / English establish that English is no longer the default in the fictional universe; introduces a generic text-bitmap renderer | Complete (commits bb41e70, 288a839) |
| 2026-05-27 | [Headless GOP read-back bug](PLAN-headless-readback-bug.md) | Investigation: misattributed wedge during PLAN-visual-digest phase 2 step 2d. Original framing (GOP read-back / `BltOp` flush interaction) was falsified; root cause was a mislabelled QR capacity constant (V5/Medium = 84 bytes, but the encoder was fed up to 106), which panicked the firmware. | Resolved (commit d66c7f6) |
