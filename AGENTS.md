# Agent guidance for uncalibrated-sextant

## What this repo is

Codename **uncalibrated-sextant**. A SPICE channel exercise harness
delivered as a tiny retrofuturist sci-fi mini-game running as a
`no_std` UEFI Rust binary inside a guest VM. The harness is driven by
[Ryll](../ryll/) over a serial link.

The codename is intentional: pick a true name later, when the content
tells us what it should be.

## Build commands

- `make build` — build the UEFI binary inside Docker. Docker is the
  only build-time dependency; no host Rust toolchain is required.
- `make qemu` — build, assemble a FAT32 ESP image (via Alpine/mtools
  in Docker), and launch an interactive QEMU/OVMF GTK window on the
  host. Requires `qemu-system-x86_64` and `ovmf` on the host.
- `make release` — build + assemble, then produce
  `dist/uncalibrated-sextant.img` (raw) and
  `dist/uncalibrated-sextant.qcow2` via `qemu-img convert`. Requires
  `qemu-utils` in addition to the `make qemu` dependencies.
- `make release-verify` — runs `make release`, then boots both
  artifacts headless and checks that the serial log contains the
  expected banner within 30 s.
- `make clean` — remove the named Docker volume
  (`uncalibrated-sextant-target`) and the local `target/` and `dist/`
  directories.

## Where to read first

1. [README.md](README.md) — project framing
2. [DESIGN.md](DESIGN.md) — channel mapping table, two-channel test
   architecture, design principles, aesthetic direction, open
   questions
3. [ARCHITECTURE.md](ARCHITECTURE.md) — implementation shape (stub
   until code lands)

## Current phase

Phase 2 landed. The crate builds to a valid PE32+ UEFI binary that
boots under QEMU with OVMF, prints a two-line banner, waits for a key
via `wait_for_event`, then shuts down the platform via ACPI rather
than returning to the firmware boot manager. Release artifacts (raw
and qcow2) are produced and verified headless. The open questions in
DESIGN.md — art direction, audio scope, scene sequencing — remain
unresolved and should be addressed before Phase 3 work begins.

## Design principles to respect

These are easy to violate by accident. Re-read them before suggesting
features:

- The game would survive having its test purpose stripped — diegetic
  mechanics, not "ClipboardQuest"
- No fourth-wall breaks; deadpan voice
- Debuggability over fidelity — a broken channel should be visually
  obvious in a single screenshot
- Stay in the genre idiom (atompunk / vault-terminal / Asimov-robot
  flavour) without lifting names, characters, or assets from any
  specific IP

## Sibling projects to be aware of

- [ryll](../ryll/) — the test driver
- [kerbside](../kerbside/) / [kerbside-patches](../kerbside-patches/)
  — the SPICE proxy under test
- [uefi-latency-guest](../uefi-latency-guest/) — the minimal probe
  this project complements (do not merge into it)
- [instar](../instar/) — source of the gRPC-over-serial transport
  pattern intended for this project
