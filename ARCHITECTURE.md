# Architecture

The Phase 1 and Phase 2 skeletons are implemented. The crate
`uncalibrated-sextant` targets `x86_64-unknown-uefi` and depends on
`uefi = "=0.37.0"` (panic handler, alloc, and global allocator
features enabled). The build runs entirely inside a `rust:1.88-slim`
Docker image orchestrated by `scripts/build.sh` and a minimal
`Makefile`; the compiled `.efi` binary lands in a named Docker volume
(`uncalibrated-sextant-target`). The entry point initialises the UEFI
helpers, constructs the Phase 4 renderer (see below), runs the current
test scaffold, blocks on a keypress via `uefi::boot::wait_for_event`,
then calls `uefi::runtime::reset` with `ResetType::SHUTDOWN` so QEMU
receives an ACPI shutdown signal and exits cleanly without operator
intervention.

Phase 2 added a host-native launch path. `scripts/mkesp.sh` runs a
disposable Alpine container (no Dockerfile; `apk add mtools dosfstools`
on demand) to format a 33 MiB FAT32 image and install the binary as
`EFI/BOOT/BOOTX64.EFI`. `scripts/qemu.sh` launches `qemu-system-x86_64`
directly on the host with OVMF pflash firmware (two separate
`OVMF_CODE_4M.fd` / `OVMF_VARS_4M.fd` drives, VARS copied fresh each
run), KVM acceleration, a Q35 machine, a GTK display, and serial output
to `dist/serial.log`. The release path (`make release`) copies the ESP
image to `dist/uncalibrated-sextant.img` and converts it to
`dist/uncalibrated-sextant.qcow2` via `qemu-img convert`. The
`make release-verify` target boots both artifacts headless and polls
the serial log for the banner string within a 30-second timeout.

Phase 4 added `src/renderer/`, a GOP-backed text renderer. The
`Renderer` struct owns a `ScopedProtocol<GraphicsOutput>`, attempts
to switch to 1024x768 on construction (falling back to the current
mode if unavailable), and clears the screen to solid black. Every
glyph is sent to the firmware as a separate `BltOp::BufferToVideo`
call, honouring principle 6 — no monolithic framebuffer memcpy.
`draw_telemetry_line` implements dot-leader layout: label from column
0, dots filling to column 40, status field following one space past
the leader; every dot is itself a separate `draw_glyph` call.
The palette is phosphor-green foreground `rgb(51, 150, 51)` on pure
black. Glyph bitmaps come from `src/renderer/font.rs`, which holds
the spleen 8x16 font (BSD-2-Clause; see `LICENSES/FONT_SPLEEN.txt`)
as a `[[u8; 16]; 128]` ASCII table generated once by
`scripts/vendor-font.py` and committed for reproducibility.

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

Style enforcement is declared in `.pre-commit-config.yaml` and
executed by `scripts/check-rust.sh`, which reuses the Phase 1 Docker
build image (`uncalibrated-sextant-build:1.88.0`) so Rust checks
never require a host toolchain. The GitHub Actions workflow at
`.github/workflows/pre-commit.yml` runs all non-Rust hooks (trailing
whitespace, YAML, shellcheck, secret scanning) on every push and pull
request; the `rust-check` hook is skipped there and enforced locally.

This file will be expanded with concrete module/crate boundaries
once we commit to an implementation skeleton.
