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

## Standalone plans

| Date | Plan | Intent | Status |
|------|------|--------|--------|
| 2026-04-25 | [Language probes](PLAN-language-probes.md) | Worldbuilding side quest: four boot-sequence lines in Mandarin / Hindi / Spanish / English establish that English is no longer the default in the fictional universe; introduces a generic text-bitmap renderer | Complete (commits bb41e70, 288a839) |
