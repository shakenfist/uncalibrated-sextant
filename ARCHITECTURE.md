# Architecture

This repo is in the design phase. No implementation exists yet.

The intended high-level shape is sketched in [DESIGN.md](DESIGN.md):

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
