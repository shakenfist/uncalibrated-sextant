# Architecture

The initial skeleton (Phase 1) is implemented. The crate
`uncalibrated-sextant` targets `x86_64-unknown-uefi` and depends on
`uefi = "=0.37.0"` (panic handler, alloc, and global allocator
features enabled). The build runs entirely inside a `rust:1.88-slim`
Docker image orchestrated by `scripts/build.sh` and a minimal
`Makefile`; the compiled `.efi` binary lands in a named Docker volume
(`uncalibrated-sextant-target`). The current entry point initialises
the UEFI helpers, clears the screen, prints a banner, and waits for a
keypress before returning `EFI_SUCCESS`.

The remaining components are still planned. The intended high-level
shape is sketched in [DESIGN.md](DESIGN.md):

- A `no_std` UEFI Rust binary built with the `uefi-rs` crate
- A single in-memory ring buffer of test events (input arrivals,
  scene state changes, channel-relevant lifecycle events)
- A serial transport using gRPC-over-serial (pattern lifted from
  [instar](../instar/)) feeding events outbound to Ryll and
  accepting inbound commands
- A renderer using GOP that draws the current scene plus a periodic
  on-screen digest (QR or compact text) of the same ring buffer
- Input collectors using Simple Text Input Ex (keyboard) and Simple
  Pointer Protocol (mouse), each pushing into the ring buffer

This file will be expanded with concrete module/crate boundaries
once we commit to an implementation skeleton.
