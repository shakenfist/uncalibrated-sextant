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

Phase 5 added the scene state machine (`src/scene.rs`), event ring
buffer (`src/event.rs`), cursor glitch module (`src/cursor.rs`), and
logo pipeline (`src/logo.rs`, `scripts/vendor-logo.py`). `Scene`
drives three phases — Awaiting, Booting, Parked — in sequence.
Awaiting and Parked blink a cursor and unblock on the first keypress.
Booting walks a `static BOOT_SCRIPT` of `SceneStep::Telemetry` and
`SceneStep::Line` entries, rendering each with `draw_telemetry_line`
or `draw_line` and pausing 200 ms between lines. The parking screen
appends its SYSTEM ONLINE prompt below the boot transcript; the
screen is not cleared between Booting and Parked, keeping the full
boot log visible. `Scene::draw_chrome` places the Shaken Fist logo in
the top-right corner by calling `Renderer::draw_logo`; the logo is
re-painted after every `renderer.clear()` call.

`CursorState` in `src/cursor.rs` implements a 1 Hz blink (500 ms on,
500 ms off). On every sixth blink-on transition a 16-bit Fibonacci
LFSR selects one of four broken-glyph variants — `GLYPH_MISSING_PIXEL`
(3x3 centre hole), `GLYPH_SMEARED_EDGE` (right two columns dark),
`GLYPH_SHIFTED_COLUMN` (whole block shifted two pixels right),
`GLYPH_PHOSPHOR_TRAIL` (top three rows dark) — substituted in place of
the canonical solid-block glyph. `CursorState` is shared between
Awaiting and Parked so the LFSR and blink counter carry across
phases.

`RingBuffer<256>` in `src/event.rs` records `Event::Keypress`,
`Event::LineRendered`, and `Event::SceneTransition` events as they
occur, overwriting the oldest entry on overflow. `RingBuffer::iter`
yields events in chronological order; `Phase::tag` returns stable
lowercase phase names for serialised output.

The logo pipeline: `scripts/vendor-logo.py` rasterises
`shakenfist-logo-small.svg` via ImageMagick at 300 DPI, resizes to
128x128, thresholds at 50% grey, then flips 2% of pixels using a PRNG
seeded to `0x5EAFED` for reproducibility, and emits `src/logo.rs` as
a row-major packed-bit `[u8; 2048]` const. The logo and the language
probes (below) share `Renderer::draw_text_bitmap`, which tiles a
packed 1-bit-per-pixel bitmap as an 8x16-cell grid (one
`BltOp::BufferToVideo` per cell, principle 6) and accepts widths and
heights that are not multiples of the cell size.

The boot sequence opens with four language-probe lines —
`检测中文支持 ........ 失败`, `हिन्दी समर्थन की जाँच ........ विफल`,
`Detectando soporte para español ........ FALLO`, and
`Probing for English support ........ OK` — establishing in
worldbuilding terms that English is no longer the default.
`scripts/vendor-language-probes.py` rasterises each label and status
via ImageMagick + GNU Unifont (used under SIL OFL 1.1; see
`LICENSES/FONT_UNIFONT.txt`) at pointsize 16, packs them as
row-major MSB-leftmost bytes, and emits `src/probes.rs`.
`SceneStep::Probe` carries references to the bitmap pairs;
`Renderer::draw_probe_line` composes a hybrid layout — bitmap
label at column 0, ASCII space + dot leader ending at
`DOT_LEADER_COL = 40`, bitmap status starting at column 41 — so
column alignment with the rest of the boot transcript is preserved.

`MARGIN_X = MARGIN_Y = 16` overscan margins are applied as a
renderer-level pixel offset added to every `draw_glyph` call, keeping
content clear of scan-line overshoot at the screen edges.

The narrator-leak parentheticals described in DESIGN.md's
`Voice: unreliable narration leaks` section are not present in the
default boot sequence. This is deliberate: they are deferred pending
a diagnostic-mode mechanism that will gate them in a future phase.

Phase 6 added `src/serial.rs` — two Serial-protocol writers sharing a
`with_serial` helper that briefly opens
`uefi::proto::console::serial::Serial` via
`open_protocol_exclusive`, hands it to a closure, and drops the
handle on exit. `write_startup_banner` emits a single
`Hello from Uncalibrated Sextant` line at `main` entry; the
release-verify harness greps for this to confirm the binary reached
its entry point. `drain` walks the scene's ring buffer with
`RingBuffer::iter` and writes one CRLF-terminated line per event
(`t=<ms> type=<keypress|line|transition> ...`) with stable
lowercase tags so a future Ryll-side parser can match literally. The
drain is called immediately before `uefi::runtime::reset`, so it
fires only after the operator has advanced past the parking screen.
If no Serial protocol is present (e.g. QEMU invoked without a
`-serial` backend), both writers are silent no-ops and the scene
still shuts down cleanly.

Phase 6 also added `scripts/screenshot.sh` and a `make screenshot`
target. The script boots the ESP image with `-display none` and a
QMP Unix socket, waits for the startup banner in the serial log,
sends a synthetic space keypress via QMP `send-key` to advance
AWAITING into the boot sequence, waits for the parking screen to
settle, then issues QMP `screendump` with `format=png` directly to
`docs/images/boot-sequence.png`. A second synthetic keypress
releases the parking screen so the drain runs and the script can
confirm `type=` lines are present in the serial log — the script
fails if the drain produced nothing, making it a full end-to-end
smoke test.

The remaining components still to be built:

- **gRPC-over-serial transport** — structured Ryll-facing event
  channel (pattern from [instar](../instar/)). The current plain-text
  drain is groundwork; the real transport, inbound commands, and
  streaming transmission during the scene are all future work.
- **Simple Pointer Protocol** — mouse / pointer input collector
  pushing into the ring buffer. Deferred; the Booting handshake
  currently requires a keypress only.
- **On-screen digest** — QR or compact-text rendering of buffered
  events. Future phase.

Style enforcement is declared in `.pre-commit-config.yaml` and
executed by `scripts/check-rust.sh`, which reuses the Phase 1 Docker
build image (`uncalibrated-sextant-build:1.88.0`) so Rust checks
never require a host toolchain. The GitHub Actions workflow at
`.github/workflows/pre-commit.yml` runs all non-Rust hooks (trailing
whitespace, YAML, shellcheck, secret scanning) on every push and pull
request; the `rust-check` hook is skipped there and enforced locally.
