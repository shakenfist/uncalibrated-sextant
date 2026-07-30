# Uncalibrated Sextant

A small UEFI Rust binary that boots inside a guest VM and exercises the
SPICE remote-display protocol end-to-end — display, cursor, keyboard,
pointer, audio, clipboard and USB redirection — by being a tiny game
the guest renders to its framebuffer and that a
[Ryll](https://github.com/shakenfist/ryll)-driven test harness drives
over a serial channel.

The codename is deliberate: a sextant that needs calibrating is exactly
what a SPICE channel under test is — an instrument whose readings you
have to verify before you can trust them.

## What it looks like

![Parking-screen capture from the first-playable build](https://raw.githubusercontent.com/shakenfist/uncalibrated-sextant/main/docs/images/boot-sequence.png)

The parking-screen frame after a keypress through the boot
sequence, captured by `make screenshot` and regenerated on demand.

## Building and running

```
make qemu            # build, assemble ESP, launch interactive QEMU window
make spice           # build, assemble ESP, launch QEMU with SPICE + remote-viewer
make release         # produce dist/uncalibrated-sextant.{img,qcow2}
make release-verify  # headless boot check of both release artifacts
make screenshot      # regenerate docs/images/boot-sequence.png via QMP
make digest-payload-smoke  # headless boot, decode parking-screen QR, assert TLV
make vendor-probes   # regenerate src/probes.rs from the vendor script
make build           # build the UEFI binary only (Docker, no host toolchain)
make clean           # remove dist/, target/, and the named Docker volume
```

`make qemu` is the primary interactive target. It opens a GTK window;
press any key to exit cleanly via ACPI shutdown. `make spice` is the
required launch path for any scene that exercises a SPICE channel; see
[docs/scenes.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/docs/scenes.md)
for the SPICE launch paths, the scene walkthroughs (including the
locked-bootloader paste scene and the display-mode keystrokes), and
host dependencies.

## Why a UEFI binary

Booting an entire OS to test a remote-display protocol is unpredictable
and slow. UEFI gives us a deterministic pre-OS environment with direct
access to the firmware's Graphics Output Protocol, Simple Pointer
Protocol, Simple Text Input Ex Protocol, and Serial I/O Protocol — all
the surfaces SPICE actually delivers events into — without an OS in the
way. The companion project
[uefi-latency-guest](https://github.com/shakenfist/uefi-latency-guest)
takes the same approach for latency probing and is intentionally
minimal; uncalibrated-sextant is the rich counterpart that asks "did
every channel work *correctly*", not just "did the path work at all".

## Why a game

Two reasons. First, a diegetic harness exercises SPICE channels in
combinations that synthetic tests don't think to try (a cursor sprite
changing over a hotspot is the cursor channel under realistic load,
not a unit test for the cursor channel). Second, when something
breaks, "the sword cursor doesn't change to a hand over the door" is
instantly obvious to a human reviewing a screenshot — far more so than
a hash mismatch in a log. Fun is a side benefit; debuggability is the
load-bearing reason.

## Contributing

Install the hooks once with `pre-commit install`, run the suite with
`pre-commit run --all-files`, and auto-fix rustfmt and clippy warnings
with `./scripts/check-rust.sh fix`. Contributor-side dependencies are
Docker (for the Rust checks) and `pre-commit` itself — no host Rust
toolchain is required.

## Documentation

- [docs/scenes.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/docs/scenes.md) - Running under SPICE and the scene walkthroughs
- [docs/visual-digest-format.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/docs/visual-digest-format.md) - The QR visual-digest wire format
- [docs/spice-test-inventory.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/docs/spice-test-inventory.md) - Which SPICE behaviours are exercised where
- [docs/plans/](https://github.com/shakenfist/uncalibrated-sextant/blob/main/docs/plans/index.md) - The planning documents behind each milestone
- [DESIGN.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/DESIGN.md) - Channel mapping, test architecture, and aesthetic direction
- [ARCHITECTURE.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/ARCHITECTURE.md) - Modules, scenes, serial message types, and build tooling
- [AGENTS.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/AGENTS.md) - Guide for AI coding assistants

## Sibling projects

- [ryll](https://github.com/shakenfist/ryll) — drives the test harness,
  parses serial events, asserts outcomes
- [kerbside](https://github.com/shakenfist/kerbside) and
  [kerbside-patches](https://github.com/shakenfist/kerbside-patches) —
  the SPICE proxy layer under test
- [uefi-latency-guest](https://github.com/shakenfist/uefi-latency-guest)
  — the minimal latency probe; this repo is its richer cousin
- [instar](https://github.com/shakenfist/instar) — source of the
  gRPC-over-serial transport pattern we plan to lift
