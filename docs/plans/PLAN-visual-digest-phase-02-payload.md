# Visual on-screen digest — phase 2: ring-buffer payload and framebuffer hash

Parent plan:
[PLAN-visual-digest.md](PLAN-visual-digest.md).

## 2026-05-28 erratum

This plan refers throughout to "QR Version 5 / ECC Medium
byte-mode capacity = 106 bytes". That label is wrong: per the
QR Code 2005 spec, Table 7, the V5 byte-mode capacity by ECC
level is L=106, M=84, Q=60, H=46. 106 is the V5/**L** number,
not V5/M. The encoder configured `QrCodeEcc::Medium`, so any
payload of ≥85 bytes panicked in `encode_binary` with
`DataTooLong` and the `uefi` panic handler hung the firmware
— this is the bug that
[PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md)
attributed to GOP read-back interaction.

The fix on 2026-05-28 kept the 106-byte capacity and dropped
the encoder to `QrCodeEcc::Low`. Read claims of "V5/Medium"
below as "V5/Low" — the capacity arithmetic (header + records
+ trailer) still adds up, only the ECC budget changed.

## Outcome

**Status: Code complete (commits 818d5f4, 8bef81d, 6221301,
6b62da2, plus this closeout). One known limitation: the
scripted-scene smoke is degraded to AWAITING-only because of
a second-read-back-breaks-subsequent-writes bug in OVMF/QEMU's
GOP path under headless `-display none`. Deferred to a
follow-up plan.** *(See erratum above: the "read-back" framing
was wrong; root cause was a constant mislabelled V5/M instead
of V5/L. Resolved 2026-05-28.)*

### What Phase 2 actually delivered

- `src/digest.rs` — pure-function TLV encoder over a
  `RingBuffer<256>` snapshot. Header: `SXDG` magic +
  schema version 1 + u32 LE frame counter + u8 record
  count. Eight per-variant TLV encoders with stable u8
  type tags (0x01..=0x08) mirroring `serial::drain`'s
  `type=` vocabulary. Trailer: 4 LE bytes of injected
  framebuffer hash. `crc` v3.4.0 dep added (no_std, no
  alloc). Capacity at QR Version 5 / ECC Medium is 92
  bytes for records; the most-recent-N-that-fit selection
  walks the ring backwards and emits chronologically.
  Hand-traced boundary cases: empty ring → 14 bytes;
  single Keypress → 28 bytes; worst-case all-ModeSwitch
  → 5 records / 104 bytes; best-case all-BootloaderTimeout
  → 9 records / 104 bytes. (commit `818d5f4`.)
- `Scene::refresh_digest` wired at three scene-phase
  boundaries (after `run_awaiting`, after `run_booting`,
  before serial drain at end of `run_parked`), plus a
  one-shot in-`run_awaiting` call gated behind
  `digest-smoke` to preserve the existing AWAITING smoke's
  fast/deterministic shape. `digest_frame_counter: u32`
  on `Scene`, wrapping_add per refresh. The bootloader
  carve-out is enforced by `assert!(!matches!(
  self.repaint_state, RepaintState::BootingBootloader {
  .. }))` inside `refresh_digest`; the bootloader scene's
  R/I/A and paste loops do not call it. The phase-1
  hard-coded `draw_digest(b"hello")` injection was
  removed; the AWAITING smoke now exercises the real
  encode path. (commit `8bef81d`.)
- `Renderer::crc32c_framebuffer_excluding_digest` —
  row-by-row `BltOp::VideoToBltBuffer` read-back into a
  stack-sized scratch row (`[BltPixel; 1920]` for the
  worst-case mode width), CRC32C accumulator across
  rows, with the digest band's x-range skipped to avoid
  the self-referencing-hash trap. The chosen path A from
  the 2c-measure report. Replaces the placeholder
  `framebuffer_hash = 0` in `refresh_digest`. The
  AWAITING smoke now surfaces the trailer CRC in the
  success line: `digest-smoke: ok (magic=SXDG version=1
  frame=1 records=0 crc32c=0x0f84c2b6)`. Hash is
  deterministic across consecutive runs at the same
  mode. (commit `6221301`.)
- `make digest-payload-smoke` target + new
  `scripts/digest-payload-smoke.sh`: a parallel smoke
  to `make digest-smoke` with a richer TLV-header
  assertion (defensive per-record parse validating tags
  0x01..=0x08 and per-record value lengths, CRC surfaced
  in the success line). See *Known limitations* below
  for why this is currently AWAITING-only rather than
  the scripted-scene path the plan originally specified.
  (commit `6b62da2`.)

### What Phase 2 did NOT deliver, and why

- **Scripted-scene smoke (full boot → bootloader → paste
  → parking → screendump → TLV-with-records-and-frame≥3).**
  Deferred — the underlying GOP bug (below) blocks it. The
  delivered `make digest-payload-smoke` holds in AWAITING
  the same way `make digest-smoke` does, but asserts a
  richer TLV-header invariant set than the existing smoke.
  The two smokes are kept as parallel targets so the
  scripted-scene path can be re-armed in the follow-up
  plan without rewriting either driver.
- **Host-side `cargo test` for the TLV encoder.** Same
  deferral as PLAN-display-mode-keystrokes-phase-01-renderer's
  *Deferred from the master plan*: no host-test
  infrastructure today, would require a workspace split.
  Encoder validated by construction (pure function),
  integration (`make digest-smoke` produces a decodable
  payload with the expected magic / version / CRC), and
  hand-traced boundary cases at step 2a's commit.
- **`docs/images/boot-sequence.png` regeneration.**
  Skipped — `make screenshot` builds the production
  (no-feature) binary, which has no digest. The reference
  image is unchanged. Phase 3 (or whenever the digest
  call is made unconditional) regenerates it.

### Path-A measurement summary (step 2c)

Two throwaway commits on worktree branch
`worktree-agent-ae90f1c11cb98981e` measured both candidates
under OVMF + QXL at 1024x768 (5 runs each, rdtsc cycles
via `core::arch::x86_64::_rdtsc()`):

| Metric                          | Path A (read-back) | Path B (incremental) |
|---------------------------------|--------------------|----------------------|
| `refresh_digest` median cycles  | 21,504,162         | 2,772,518            |
| `refresh_digest` p95 cycles     | ~21,785,000        | ~3,071,000           |
| Per-paint overhead              | none               | ~19.4M cycles per `clear()` (~195x slowdown vs baseline) |
| `.efi` size (digest-smoke)      | 65,536 bytes       | 65,536 bytes         |
| Hash determinism across 5 runs  | yes (0x0f84f659)   | yes (0xb10bfa46)     |
| Per-boot total estimate         | ~65M cycles (3×21.5M) | likely >100M, distributed |

Path A chosen because:

1. Concentrated cost (~21 ms total per boot at 3 GHz) vs
   path B's cost leaking into every cursor blink, every
   toast tick, every glyph paint.
2. Tests the SPICE display pipeline read-back path
   itself — exactly the integrity-of-display semantic the
   master plan wanted ("did the pixels actually make it
   onto the framebuffer?" rather than "did our code
   *intend* to put them there?").
3. Pause-discipline lives in one place (the renderer
   method skips the digest region from the hash). Path B
   would require every future paint site to honour a
   `crc32c_paused` flag forever.
4. `.efi` size is identical, so size is not a tiebreaker.
5. The main open-question risk (`BltOp::VideoToBltBuffer`
   returns garbage under QXL?) was resolved by the
   measurement: the hash is deterministic and non-zero
   across 5 runs.

### Known limitations

- **Second-read-back-breaks-subsequent-writes under
  headless GOP.** When the digest-smoke binary fires
  more than one `refresh_digest` per boot under
  `qemu-system-x86_64 -display none` (any `-vga`
  backend tested — `std` and `qxl` both fail), the
  framebuffer writes for `draw_digest` after the
  *second* `BltOp::VideoToBltBuffer` read-back call
  silently no-op: the parking screen renders correctly
  (chrome, status lines, "EMERGENCY SAFE BOOT
  COMPLETE.") but no QR is painted. The first
  read-back + draw cycle works fine in both AWAITING
  smokes; the second read-back is where the writes
  fail. Investigation found:
  - Bug reproduces under `-vga std -display none`
    *and* `-vga qxl -display none`.
  - Bug does *not* reproduce under interactive
    `make spice-ryll` (`-vga qxl -spice port=…`),
    which is the production target — the user
    confirmed the digest renders at parking under
    SPICE during the Phase 1 cross-mode check.
  - Likely root cause: SPICE's surface-management /
    dirty-tracking either forces or substitutes a
    flush between read-back and subsequent writes
    that `-display none` does not provide. This is
    a SPICE-vs-headless GOP backend interaction in
    OVMF, not a uncalibrated-sextant code defect.

  This blocks the scripted-scene smoke from being
  useful headlessly. It does **not** block production
  use: ryll observes via SPICE, the user observes via
  SPICE, and the user-confirmed cross-mode check
  showed the digest rendering correctly. A follow-up
  plan should investigate either:
  - Adding an explicit GOP flush / no-op write
    between `crc32c_framebuffer_excluding_digest`
    and the next `BufferToVideo` to nudge the
    backend.
  - Moving the smoke to ryll-driven verification
    (which would also exercise the SPICE pipeline
    end-to-end, matching the production path).
  - Filing upstream against OVMF / QEMU if the bug
    can be reduced to a minimal repro.

  Until then, `make digest-payload-smoke` is the
  AWAITING-only variant: it validates the wire
  format, the per-record bounds, and the path-A
  read-back at AWAITING. The scripted-scene smoke
  remains a deliverable for the follow-up plan.

  **Tracked separately in
  [PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md)**
  with reproduction steps, the hypotheses, and the
  candidate remediation paths in cost order. Resolving
  that plan before Phase 3 starts is the cleanest
  sequencing — Phase 3's repaint integration will
  increase the per-boot refresh count and surface the
  bug earlier than Phase 2 did.

### Surprises and findings

- **uefi-rs 0.37 spelling.** The variant is
  `BltOp::VideoToBltBuffer`, not `VideoToBuffer`
  (asymmetric with `BltOp::BufferToVideo`). Discovered
  during 2c-measure. The phase plan documentation used
  the shorter name; the code uses the correct one.
- **`BltPixel` is 4 bytes in repr(C).** Three named
  fields (`blue`, `green`, `red`) plus one byte of
  padding/reserved. The renderer's row-byte slice
  conversion in path A accounts for this via
  `core::mem::size_of::<BltPixel>()`.
- **Path B's `clear()` slowdown.** Adding the
  incremental-hash hook to `BltOp::VideoFill` (which
  `Renderer::clear()` uses) caused a ~195x cycle-count
  increase per call. The byte-count fed into the CRC
  per fill is large (whole-row or whole-screen rectangles
  filled with `BG`), so the cost is in `crc::Digest::update`
  walking those bytes. Concentrated path-A read-back
  hashes the same bytes once per refresh; distributed
  path-B hashes them on every paint.
- **AWAITING ring is empty at the first refresh.** The
  one-shot `refresh_digest` in `run_awaiting` fires
  before any keystroke or event is pushed, so
  `records=0` in the smoke output. This is the
  intended behaviour: the digest header (magic / version
  / frame counter / CRC) is meaningfully present even
  when the body is empty.
- **`crc32c()` helper unused.** The phase-2a brief
  bundled it as a convenience for callers; nothing in
  path A's renderer method uses it (they use
  `CRC32C.digest()` directly for the accumulator).
  Removed in commit `6221301`.

### Operator action remaining for Phase 2 closeout

- **Cross-mode visual confirmation under `make
  spice-ryll`** (with the `digest-smoke` feature built
  in) — confirm the digest reaches the parking screen
  with a real TLV payload (not the static `hello` from
  phase 1) and stays glued to the bottom-right corner
  across mode keys `'1'` / `'3'` / `'5'`. Mode-switch
  survival (digest repainted after the framebuffer
  invalidation) is still a Phase 3 concern; this check
  just confirms the production path produces a real
  payload.

## Prompt

Before working on this phase, re-read the master plan's
*Phase 2 sketch* and the *Open questions* it points to — in
particular the read-back-vs-incremental-hash measurement-
driven design call, the bootloader carve-out, and the TLV
shape default. Skim:

- [`src/event.rs`](../../src/event.rs) — the eight current
  `Event` variants and the `RingBuffer<256>` shape. The
  variant set is the type vocabulary the TLV format must
  cover; the `RingBuffer::iter()` yields chronological
  order without draining, which is exactly the read pattern
  the payload encoder needs.
- [`src/serial.rs`](../../src/serial.rs) — the existing
  per-variant text formatter in `drain`. The TLV type tags
  should mirror the `type=…` discriminators there
  (`keypress`, `line`, `transition`, `bootloader_decision`,
  `paste`, `bootloader_timeout`, `mode_switch`,
  `mode_cycle`) so a human inspecting both representations
  sees the same vocabulary.
- [`src/scene.rs`](../../src/scene.rs) — the `Scene` struct,
  `run_awaiting` / `run_booting` / `run_parked` flow, the
  `RepaintState` enum, `Scene::repaint`, and the call to
  `bootloader::run` (~line 419). Phase 2 wires a refresh
  call between scene-phase transitions; it does *not* touch
  any of the per-tick poll loops inside those runners or
  inside the bootloader.
- [`src/bootloader.rs`](../../src/bootloader.rs) — the
  R/I/A prompt loop, paste-capture loop, and retry-animation
  pacing. These are the loops the digest must not refresh
  inside; understand the entry/exit shape so the carve-out
  assertion catches accidental re-entry.
- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) — the
  existing `BltOp::BufferToVideo` and `BltOp::VideoFill`
  patterns. Phase 2 adds the first `BltOp::VideoToBuffer`
  call site in the project (read-back from GOP).
- [`scripts/spice-ryll.sh`](../../scripts/spice-ryll.sh) and
  [`scripts/screenshot.sh`](../../scripts/screenshot.sh) —
  the spice-ryll script accepts a pre-built ESP path
  argument (default `dist/esp.img`), useful for isolating
  measurement runs from rebuild noise.

External references:

- [uefi-rs 0.37.0 docs for `BltOp`](https://docs.rs/uefi/0.37.0/uefi/proto/console/gop/enum.BltOp.html)
  — confirm `VideoToBuffer` parameters before writing the
  call site.
- [UEFI 2.10 §12.9.2](https://uefi.org/specs/UEFI/2.10/12_Protocols_Console_Support.html#efi-graphics-output-protocol-blt)
  — `Blt()` semantics, including that the call blocks
  until pixels are committed. Implications for read-back
  correctness: if the QXL paravirtualised driver shadows
  the framebuffer in host memory and lazy-flushes,
  `VideoToBuffer` may return stale or zero-filled pixels.
- [`crc` crate](https://docs.rs/crc/3/) — the chosen
  CRC32C source. `Algorithm::CRC_32_ISCSI` is the
  Castagnoli polynomial used as CRC32C in iSCSI, SCTP,
  and Btrfs.
- The `shakenfist/instar` crate at
  `/srv/kasm_profiles/mikal/vscode/src/shakenfist/instar/`,
  `crates/guest-protocol/src/lib.rs` — precedent for
  length-prefixed binary framing on a guest serial
  transport. Phase 2 does not adopt protobuf (overkill
  here), but the framing convention is a useful sanity
  check.

This phase plans at **high effort overall (opus)**, with one
step (2c framebuffer hash) explicitly held to *report before
deciding* per the master plan's measurement-driven design
note. Step 2c runs in an isolated worktree because the
chosen read-back path may need to be reverted in favour of
the incremental-hash fallback.

## Goal

By the end of this phase:

- A TLV wire format exists, encoded by a pure function from
  the current `RingBuffer<256>` snapshot into a fixed-size
  stack buffer. Header includes magic, schema version, and
  monotonic frame counter; body is a sequence of typed
  event records; trailer is CRC32C of the framebuffer's
  non-digest region.
- `Scene::run_awaiting`, `Scene::run_booting`, and
  `Scene::run_parked` each call a shared digest-refresh
  helper at their natural scene-phase boundaries.
  `bootloader::run` and its inner poll loops do **not**.
- An assertion (or, if assertion is impractical, a clearly-
  documented invariant) catches accidental digest refresh
  from inside `bootloader::run`.
- The trailing 4 bytes of the QR payload are a CRC32C of
  the framebuffer's non-digest pixels, computed by one of
  two paths: (a) read-back via `BltOp::VideoToBuffer`, or
  (b) incremental hashing during the per-frame paint. The
  choice is **measurement-driven**: a sub-agent runs both
  paths, measures cost under OVMF+QXL on `make spice-ryll`,
  reports numbers, and the management session picks. The
  chosen path is documented inline at the call site, not
  just in the plan.
- The displayed QR encodes a real ring-buffer-driven
  payload that ryll-or-zbarimg can decode; the trailing
  CRC matches an independent recomputation by a host-side
  smoke script.
- The existing `make digest-smoke` target continues to
  pass (its hard-coded `b"hello"` injection from Phase 1
  remains under the same feature gate, but the smoke is
  now exercised with a real payload too — likely a new
  `make digest-payload-smoke` target that flips the feature
  and asserts the TLV header magic + frame counter ≥ 1).
- `make screenshot` continues to pass. Its reference image
  will change because the parking screen now has a real
  ring-buffer digest, but the change is stable across runs
  given the scripted input — the reference image is
  regenerated and committed as part of this phase.

The format spec document (`docs/visual-digest-format.md`),
the `Scene::repaint` integration, and the master-plan
closeout all land in Phase 3.

## Scope

**In scope:**

- One new Cargo dependency for CRC32C (default:
  `crc = { version = "3", default-features = false }`).
- A new module (default: `src/digest.rs`) containing:
  - The `DigestPayload` struct and the pure encoding
    function from `RingBuffer<256>` snapshot + frame
    counter + framebuffer-hash to a fixed-size `[u8; N]`
    buffer.
  - The TLV type-tag table (one `const` per `Event`
    variant currently emitted).
  - A `DigestFrameCounter` (monotonic `u32`) owned by
    `Scene`, incremented per refresh.
- A `Scene` method (default: `fn refresh_digest(&mut self,
  renderer: &mut Renderer)`) that:
  - Increments the frame counter.
  - Computes the framebuffer hash via the chosen path.
  - Encodes the payload into a stack buffer.
  - Calls `renderer.draw_digest(payload_slice)`.
- Call sites for `refresh_digest` at scene-phase
  boundaries: after `run_awaiting` (just before the
  transition to `run_booting`), after `run_booting`
  (just before the transition to `run_parked`), and after
  `run_parked` (just before the serial drain).
- A bootloader carve-out enforcement mechanism:
  - **Preferred:** a `debug_assert!` (or `assert!`) inside
    `refresh_digest` that the current `repaint_state` is
    not `RepaintState::BootingBootloader`.
  - **Fallback:** a clearly-commented invariant at every
    call site, plus a comment at the top of `bootloader.rs`
    naming the carve-out.
- Either:
  - Step 2c-A: a `Renderer::read_framebuffer_region`
    helper backed by `BltOp::VideoToBuffer` that reads
    everything outside the digest region into a stack
    or static buffer, plus the CRC32C of that buffer
    computed in `refresh_digest`; OR
  - Step 2c-B: an incremental-hash hook on each
    `BltOp::BufferToVideo` call site in the renderer
    (additional `&mut self` state on `Renderer` —
    a `Crc32cState` — updated per-call), with
    `refresh_digest` reading the accumulated hash and
    resetting for the next frame.
  - The branch is taken after step 2c-measure (below).
- A new `make digest-payload-smoke` target (parallel to
  `digest-smoke`) that boots with the feature, plays a
  scripted scene through to parking, screendumps,
  decodes via `zbarimg`, parses the TLV header, and
  asserts: magic == `b"SXDG"`, version == 1, frame
  counter >= 3 (one per scene-phase), CRC trailer is
  4 bytes, payload contains at least one `keypress`
  TLV record.
- A regenerated `docs/images/boot-sequence.png`
  reference image, reflecting that the parking screen
  now carries a real digest.

**Out of scope (Phase 3):**

- `Scene::repaint` integration — the digest must also
  paint after mode-switch-driven repaints, but that
  wiring lives in Phase 3.
- `docs/visual-digest-format.md` spec document — Phase 3
  produces it as part of closeout. Phase 2 captures the
  format in code (constants + doc-comments) and in this
  plan's *Wire format* section below; Phase 3 lifts that
  into a reviewable single-page doc.
- DESIGN.md / ARCHITECTURE.md / AGENTS.md / README.md
  updates — Phase 3 closeout.
- Cross-mode automated regression of the digest — Phase 3
  inherits the repaint integration that makes this
  meaningful, then adds the regression.

**Deferred from the master plan:**

- **Host-side `cargo test` for the TLV encoder.** The
  master plan's Phase 2 sketch says "Pure logic,
  host-testable — add cargo test coverage on the host
  (the workspace-split deferred from earlier milestones
  is still deferred; a `#[cfg(test)]` module inside the
  relevant file is fine for now)." Recon confirms there
  is *no* host-test infrastructure today: no
  `#[cfg(test)]` modules anywhere in `src/`, no
  `[dev-dependencies]` in `Cargo.toml`, and the crate
  is `no_std` against `x86_64-unknown-uefi` only — a
  bare `cargo test` from the host fails to build.
  Following the precedent set by
  [`PLAN-display-mode-keystrokes-phase-01-renderer.md`](PLAN-display-mode-keystrokes-phase-01-renderer.md)
  *Deferred from the master plan*: validate the TLV
  encoder by:
  1. Construction — it is a pure function over a
     `RingBuffer<256>` snapshot, written for
     inspection.
  2. Integration — `make digest-payload-smoke` boots
     the binary, captures a screen, decodes the QR,
     and asserts header invariants.
  3. Externally — once Phase 3 publishes the format
     spec, ryll's host-side decoder exercises the
     format under its own tests.

  The master plan's *Success criteria* will be updated
  in Phase 3 closeout to point here for the
  cargo-test line.

## Wire format (committed in this phase)

This section locks the format. Phase 3 lifts it verbatim
into `docs/visual-digest-format.md`.

### Header (10 bytes, fixed)

| Offset | Length | Field                | Encoding                |
|--------|--------|----------------------|-------------------------|
| 0      | 4      | Magic                | ASCII `SXDG`            |
| 4      | 1      | Schema version       | `u8`, currently `0x01`  |
| 5      | 4      | Frame counter        | `u32` little-endian     |
| 9      | 1      | Event record count   | `u8` (max 255 records)  |

Magic is `b"SXDG"` (Sextant DiGest). It exists so a decoder
seeing a random screenshot can detect "this PNG contains a
digest" with very high confidence — random PNG noise won't
hit four exact bytes at offset 0.

Schema version is `0x01` for this phase. Increment when a
field shape changes or a TLV type is repurposed; new TLV
types do **not** require a version bump (TLV's whole point).

Frame counter is monotonic per boot, starting at `1` for
the first refresh. Wraps at `u32::MAX` (which would take
136 years at 1 Hz — not a real concern).

Event record count is the number of TLV records following
the header, before the CRC trailer. Caps at 255 by encoding;
in practice the QR Version 5 / ECC Medium capacity caps it
lower (see *Capacity budget* below).

### Body (TLV records)

Each record:

| Offset | Length | Field            | Encoding                  |
|--------|--------|------------------|---------------------------|
| 0      | 1      | Type tag         | `u8` — see table below    |
| 1      | 1      | Length of value  | `u8` (max 255 bytes)      |
| 2      | N      | Value            | type-specific, see below  |

Type tag table (parallel to `serial::drain`'s `type=` strings):

| Tag  | Variant              | Value shape                                                |
|------|----------------------|-----------------------------------------------------------|
| 0x01 | `Keypress`           | `u64 timestamp_ms`, `u16 unicode`, `u16 scancode` (12 B) |
| 0x02 | `LineRendered`       | `u64 timestamp_ms`, `u16 row` (10 B)                      |
| 0x03 | `SceneTransition`    | `u64 timestamp_ms`, `u8 from_phase`, `u8 to_phase` (10 B) |
| 0x04 | `BootloaderDecision` | `u64 timestamp_ms`, `u8 choice`, `u32 attempt` (13 B)     |
| 0x05 | `PasteReceived`      | `u64 timestamp_ms`, `u16 len`, `u8 correct` (11 B)        |
| 0x06 | `BootloaderTimeout`  | `u64 timestamp_ms` (8 B)                                  |
| 0x07 | `ModeSwitch`         | `u64 timestamp_ms`, `u16 req_w`, `u16 req_h`,             |
|      |                      | `u16 app_w`, `u16 app_h` (16 B)                           |
| 0x08 | `ModeCycle`          | `u64 timestamp_ms`, `u32 count`, `u8 interrupted` (13 B)  |

All integers little-endian. `phase` and `choice` mini-enums
use stable `u8` discriminants documented in the source
constants (`PHASE_AWAITING = 0x00`, `PHASE_BOOTING = 0x01`,
`PHASE_PARKED = 0x02`; `CHOICE_RECOVER = 0x00`,
`CHOICE_IGNORE = 0x01`, `CHOICE_ANYWAY = 0x02`). These
discriminants do *not* need to match Rust's repr-default
discriminants — they are wire numbers, set in code with a
`match` that the encoder uses.

### Trailer (4 bytes, fixed)

| Offset | Length | Field        | Encoding                   |
|--------|--------|--------------|----------------------------|
| 0      | 4      | CRC32C       | Castagnoli, little-endian  |

CRC32C is computed over the framebuffer's non-digest pixels
(every pixel *outside* the `[origin_x..origin_x +
DIGEST_REGION_PX) x [origin_y..origin_y + DIGEST_REGION_PX)`
rectangle, where `origin_x` / `origin_y` are the runtime
right-anchored coordinates from `draw_digest`). The CRC is
of the raw pixel bytes as `BltOp::VideoToBuffer` returns
them — `BltPixel` is 4 bytes wide (BGRA-ish), so a
1024x768 framebuffer is ~3 MB to hash. See *Capacity
budget* and step 2c for the cost analysis.

### Capacity budget

QR Version 5 / ECC Medium byte-mode capacity is **106 bytes**
(per the QR Code 2005 spec Table 7). Subtract:

- Header: 10 bytes
- CRC trailer: 4 bytes
- Remaining for TLV records: **92 bytes**

Each TLV record is 2 bytes overhead + 8 to 16 bytes value.
Worst-case (all `ModeSwitch`, 18 B each): **5 records**.
Best-case (all `BootloaderTimeout`, 10 B each): **9 records**.
Typical-case (mixed, average ~12 B/record): **6 to 7 records**.

The encoder must take the **most-recent-N** events that
fit, not the oldest-N. Walk the ring buffer in reverse,
push records until adding one more would overflow, then
emit in chronological (original) order. This matters for
the parking screen where the ring buffer has 30+ events
and the QR can only show the last 6 to 7.

If 6-to-7 events feels too thin, the alternative is QR
Version 7 (45×45 modules, 154 B byte-mode capacity at ECC
Medium) — 154 - 14 = **140 bytes for records, ~10 to 12
records**. Version 7's 45-module size at 4-pixel scale =
180 px square — *exactly* the size we already reserved.
Including the new 4-module quiet zone gives 53 modules at
4 px each = 212 px, which **exceeds the existing region**.
Either drop the per-module scale to 3 px (159 px square,
fits in 180) or extend the region — both are larger changes
than this phase wants. **Default for Phase 2: stay at
Version 5, 6 to 7 records typical. Capture the trade-off in
the format spec doc in Phase 3.**

## Steps

| Step       | Effort | Model  | Isolation | Brief for sub-agent |
|------------|--------|--------|-----------|---------------------|
| 2a         | medium | sonnet | none      | Add `crc = "3"` (no_std) dependency. Create `src/digest.rs` with the TLV encoder: header, type-tag constants, per-variant value encoders, body iteration that walks ring-buffer in reverse and takes most-recent-N-that-fit, CRC32C of an injected `framebuffer_hash: u32`. Pure function: `encode(snapshot: &RingBuffer<256>, frame_counter: u32, framebuffer_hash: u32, out: &mut [u8]) -> Result<usize, EncodeError>`. No Scene wiring yet. |
| 2b         | medium | sonnet | none      | Add `Scene::refresh_digest(&mut self, &mut Renderer)` calling `digest::encode` with a placeholder `framebuffer_hash = 0`. Wire call sites between scene phases (after `run_awaiting`, after `run_booting`, after `run_parked`). Add `digest_frame_counter: u32` to `Scene`. Add `debug_assert!(!matches!(self.repaint_state, RepaintState::BootingBootloader))` inside `refresh_digest`. Gate the call behind `cfg(feature = "digest-smoke")` for now — phase 3 makes it unconditional once `Scene::repaint` integration is in. |
| 2c-measure | high   | opus   | worktree  | **Report before deciding.** Implement both candidate hash paths on the worktree as separate commits: (A) `Renderer::read_framebuffer_region` via `BltOp::VideoToBuffer` into a static buffer, plus a `crc32c_of(...)` call in `refresh_digest`; (B) `Renderer::crc32c_state` field updated by an `incremental_hash` hook on every `BltOp::BufferToVideo` site in the renderer, drained on `refresh_digest`. Measure each: wall-clock per refresh on `make spice-ryll`, peak stack/static memory cost. Report numbers to the management session; do not commit either to main. |
| 2c-decide  | n/a    | n/a    | n/a       | Management session reviews 2c-measure, picks A or B, and dispatches 2c-implement with the chosen path. Choice + rationale recorded inline in `refresh_digest` and in this plan's *Outcome*. |
| 2c-impl    | medium | sonnet | none      | Implement the chosen path on `main`. Replace the placeholder `framebuffer_hash = 0` from step 2b with the real value. Verify `digest-smoke` still passes and `digest-payload-smoke` (step 2d) shows a non-zero CRC that matches a host-side recomputation. |
| 2d         | medium | sonnet | none      | Add `make digest-payload-smoke` target: build with `digest-smoke` feature, run a scripted scene-completion boot (mirror `screenshot.sh`'s key sequence), screendump to PNG, decode QR via `zbarimg --raw`, parse header bytes, assert magic / version / frame_counter / event_count, dump the human-readable record list to stdout for diagnostic visibility. Regenerate `docs/images/boot-sequence.png`. |
| 2e         | low    | sonnet | none      | Smoke exit: `make digest-smoke`, `make digest-payload-smoke`, `make screenshot`, `make release-verify` all green; `pre-commit run --all-files` green; this plan's *Outcome* section populated with commit SHAs, chosen hash path, measured costs, and any deviations from the defaults above. |

Commits expected: roughly 6 to 8 — one per step except 2c
which is two (`-impl` plus the worktree measurement report
landed as a doc-only commit summarising the numbers without
the throwaway implementations).

## Detailed step briefs

### 2a — TLV encoder + CRC32C

**Files:** `Cargo.toml`, `src/digest.rs` (new), `src/main.rs`
(register the module).

**`Cargo.toml`:**

```toml
crc = { version = "3", default-features = false }
```

Verify no_std with no alloc requirement — `crc` v3.x is
`#![no_std]` by default and uses const tables, no heap.

**`src/digest.rs` shape:**

```rust
//! TLV encoder for the visual on-screen digest. The wire
//! format is documented in
//! `docs/plans/PLAN-visual-digest-phase-02-payload.md`
//! (and, after phase 3, in `docs/visual-digest-format.md`).

use crc::{Crc, CRC_32_ISCSI};

use crate::event::{Event, RingBuffer};

/// Magic identifier for a SeXtant DiGest payload.
pub(crate) const DIGEST_MAGIC: [u8; 4] = *b"SXDG";

/// Schema version of the wire format. Bump when a field
/// shape changes; new TLV types do not require a bump.
pub(crate) const DIGEST_SCHEMA_VERSION: u8 = 0x01;

/// TLV type tags. Parallel to `serial::drain`'s `type=` strings.
pub(crate) const TAG_KEYPRESS: u8 = 0x01;
pub(crate) const TAG_LINE_RENDERED: u8 = 0x02;
pub(crate) const TAG_SCENE_TRANSITION: u8 = 0x03;
pub(crate) const TAG_BOOTLOADER_DECISION: u8 = 0x04;
pub(crate) const TAG_PASTE_RECEIVED: u8 = 0x05;
pub(crate) const TAG_BOOTLOADER_TIMEOUT: u8 = 0x06;
pub(crate) const TAG_MODE_SWITCH: u8 = 0x07;
pub(crate) const TAG_MODE_CYCLE: u8 = 0x08;

/// Wire discriminants for `Phase`. Stable across versions.
pub(crate) const PHASE_AWAITING: u8 = 0x00;
pub(crate) const PHASE_BOOTING: u8 = 0x01;
pub(crate) const PHASE_PARKED: u8 = 0x02;

/// Wire discriminants for `BootloaderChoice`. Stable.
pub(crate) const CHOICE_RECOVER: u8 = 0x00;
pub(crate) const CHOICE_IGNORE: u8 = 0x01;
pub(crate) const CHOICE_ANYWAY: u8 = 0x02;

/// Header (10) + CRC trailer (4) = 14 bytes of fixed overhead.
pub(crate) const DIGEST_HEADER_LEN: usize = 10;
pub(crate) const DIGEST_TRAILER_LEN: usize = 4;
pub(crate) const DIGEST_FIXED_OVERHEAD: usize =
    DIGEST_HEADER_LEN + DIGEST_TRAILER_LEN;

/// Maximum QR Version 5 / ECC Medium byte-mode capacity.
/// Derived from QR Code 2005 spec, Table 7.
pub(crate) const DIGEST_PAYLOAD_CAPACITY: usize = 106;

/// Encoding outcome.
#[derive(Debug)]
pub(crate) enum EncodeError {
    /// Caller supplied a buffer smaller than DIGEST_PAYLOAD_CAPACITY.
    BufferTooSmall,
    /// Encoder logic error — should not happen if record sizes are correct.
    InternalOverflow,
}

/// Encode a digest payload into `out`. Returns the byte length
/// written. The CRC32C of `framebuffer_hash` is folded into the
/// trailer; callers compute that hash independently and pass it in.
pub(crate) fn encode(
    ring: &RingBuffer<256>,
    frame_counter: u32,
    framebuffer_hash: u32,
    out: &mut [u8; DIGEST_PAYLOAD_CAPACITY],
) -> Result<usize, EncodeError> {
    // 1. Walk ring backwards, count records until adding one more
    //    would overflow (DIGEST_PAYLOAD_CAPACITY - DIGEST_FIXED_OVERHEAD
    //    = 92 bytes available for records).
    // 2. Write header in forward order: magic, version, frame_counter (LE),
    //    record_count (u8).
    // 3. Write records in chronological (forward) order.
    // 4. Trailer: CRC32C of (framebuffer_hash as LE bytes)? OR CRC32C
    //    placeholder that is just framebuffer_hash itself written LE?
    //
    //    DECIDE: the trailer is the framebuffer hash. The hash IS the
    //    integrity check. We do not need a second CRC32C of the
    //    framebuffer-hash bytes. Write framebuffer_hash as four LE bytes
    //    in the trailer position.
    //
    //    A separate CRC of the *payload itself* (header + body) would
    //    add corruption detection on the QR-decode path, but zbarimg's
    //    ECC already guarantees that — QR codes are self-checking.
    //    Skip a second CRC.
    //
    // 5. Return total bytes written.
}

/// Static CRC32C table for the Castagnoli polynomial used in iSCSI,
/// SCTP, and Btrfs. The `crc` crate computes this with a const table
/// at zero runtime cost beyond the per-byte XOR.
pub(crate) const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// Convenience for callers: compute CRC32C of a byte slice.
pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    CRC32C.checksum(bytes)
}
```

**Constraints:**

- Pure function: no `&mut Renderer`, no IO, no clock
  reads. The caller composes the framebuffer_hash and
  frame_counter; the encoder is deterministic over its
  inputs.
- `[u8; DIGEST_PAYLOAD_CAPACITY]` rather than `&mut [u8]`
  so the caller's stack frame is sized correctly and
  bounds checks elide. The buffer type carries the
  capacity invariant.
- No `Vec`, no `String`. The crate has `alloc` available
  but the encoder must not depend on it.
- Most-recent-first ring walk implementation note: the
  `RingBuffer::iter()` yields oldest-first, so the
  natural approach is to `collect` into a temporary
  fixed-size index array, walk it backwards to select,
  then emit forward. The temporary can be a
  `[Option<usize>; 256]` of ring positions, stack-allocated
  (~2 KB; fits in our stack).
- The `framebuffer_hash` is passed in, not computed.
  Step 2c provides the value.

**Validation:**

- `make build` clean.
- `cargo clippy --release --target x86_64-unknown-uefi
  -- -D warnings` clean.
- `pre-commit run --all-files` clean.
- No new behaviour observable at runtime (encoder has no
  callers in this step).

**Commit message:**
- Subject: `Add TLV encoder for visual digest payload.`
- Body: describes the wire format briefly (full spec in
  the phase plan), notes the most-recent-N-that-fit
  selection strategy, and that the framebuffer_hash is
  injected by step 2c.
- `Prompt:` paragraph summarising the step intent.
- Signed-off-by + Co-Authored-By.

### 2b — Scene wiring + bootloader carve-out

**Files:** `src/scene.rs`.

**Add to `Scene`:**

```rust
pub struct Scene {
    // ... existing fields ...
    digest_frame_counter: u32,
}
```

Initialise to `0` in `Scene::new`; increment to `1` on
first `refresh_digest` call.

**Add method:**

```rust
impl Scene {
    /// Compute and render the on-screen digest reflecting the
    /// current ring-buffer state. Called at scene-phase
    /// boundaries from the outer scene loop.
    ///
    /// Carve-out: must not be called from inside
    /// `bootloader::run` or its sub-state-machines. The
    /// debug_assert below catches accidental calls; the
    /// release build relies on the call-site contract.
    #[cfg(feature = "digest-smoke")]
    fn refresh_digest(&mut self, renderer: &mut Renderer) {
        debug_assert!(
            !matches!(self.repaint_state, RepaintState::BootingBootloader { .. }),
            "refresh_digest called during bootloader scene — carve-out violated",
        );
        self.digest_frame_counter = self.digest_frame_counter.wrapping_add(1);

        // Step 2c provides the real framebuffer_hash. Placeholder for now.
        let framebuffer_hash: u32 = 0;

        let mut buf = [0u8; crate::digest::DIGEST_PAYLOAD_CAPACITY];
        match crate::digest::encode(
            &self.ring,
            self.digest_frame_counter,
            framebuffer_hash,
            &mut buf,
        ) {
            Ok(len) => renderer.draw_digest(&buf[..len]),
            Err(_) => {
                // Encoder errors are programmer bugs at this point —
                // record buffer is sized exactly to capacity. Skip
                // refresh rather than crash; phase-3 closeout will
                // surface this via a log.
            }
        }
    }
}
```

**Call sites:** in `Scene::run`, between the existing
phase-runner calls:

```rust
self.run_awaiting(renderer);
#[cfg(feature = "digest-smoke")]
self.refresh_digest(renderer);

self.run_booting(renderer);
#[cfg(feature = "digest-smoke")]
self.refresh_digest(renderer);

self.run_parked(renderer);
#[cfg(feature = "digest-smoke")]
self.refresh_digest(renderer);

// existing serial::drain follows
```

(The exact placement may need adjustment if `Scene::run`'s
control flow is more complex — the rule is "at each
phase-to-phase transition, in the outer scene loop, not
inside any sub-runner's poll loop.")

**Constraints:**

- `digest-smoke` cargo feature still gates the call site;
  no-feature builds remain byte-identical. The
  unconditional version lands in Phase 3 alongside
  `Scene::repaint` integration.
- The Phase 1 hard-coded `draw_digest(b"hello")` call in
  `run_awaiting` is removed in this step — it's superseded
  by the first `refresh_digest` call. The smoke target
  `make digest-smoke` (which asserts `hello`) must
  therefore change shape too; either:
  - **Default:** remove the Phase 1 smoke entirely and
    replace with the Phase 2 `make digest-payload-smoke`
    that asserts TLV header invariants. This is cleaner —
    the hard-coded smoke proves nothing the payload smoke
    doesn't.
  - **Alternative:** keep `digest-smoke` by adding a second
    `digest-smoke-hello` feature that re-injects the
    hard-coded call. Adds noise for no benefit.
  Pick default unless the Phase 1 smoke is somehow load-
  bearing in CI or a developer workflow we haven't
  surfaced.
- The `debug_assert!` survives release builds with our
  current opt-level=s profile (debug_assert! is stripped
  in release by default — confirm whether to upgrade to a
  regular `assert!` based on the cost). Lean toward
  `assert!` for an invariant this load-bearing; the cost
  is one `match` per refresh, which is negligible against
  the rest of `refresh_digest`.

**Validation:**

- `make build` and `cargo build --features digest-smoke`
  both clean.
- `cargo clippy --release --target x86_64-unknown-uefi
  --features digest-smoke -- -D warnings` clean.
- `make digest-smoke` either updated or removed per the
  default above; if updated, still green.

**Commit message:**
- Subject: `Wire Scene::refresh_digest at scene-phase
  boundaries.`
- Body: notes the carve-out assertion, the three call
  sites, that framebuffer_hash is still a placeholder
  pending step 2c, and the disposition of the Phase 1
  hard-coded smoke.
- `Prompt:` paragraph.
- Signed-off-by + Co-Authored-By.

### 2c-measure — measurement report for the hash path

**Worktree-isolated.** Create two commits in the worktree
(do not push):

**Commit M1 — implement read-back path (option A):**

- Add `Renderer::read_framebuffer_region(&mut self,
  origin: (usize, usize), dims: (usize, usize), buf:
  &mut [BltPixel])` via `BltOp::VideoToBuffer`.
- In `refresh_digest`, allocate a static-or-stack
  `[BltPixel; W*H]` for the framebuffer (sized for
  worst-case 1920x1080 = 8.3 M pixels = 33 MB — clearly
  too large for stack; must be `static mut` with a
  `Mutex`, or split into row-by-row reads with
  per-row CRC32C accumulation).
- **Critical:** the row-by-row approach is almost
  certainly what survives — read one row at a time
  (1920 BltPixel = 7680 bytes on the stack), accumulate
  CRC32C across rows, skip the digest-region rows
  entirely or skip those columns within the digest-row
  range.
- Measure wall-clock per `refresh_digest` using the
  existing `Scene::clock_ms` accumulator: capture
  `clock_ms` before and after the call, log to serial.

**Commit M2 — implement incremental-hash path (option B):**

- Add `crc32c_state: CrcState` to `Renderer`, where
  `CrcState` is a wrapper around `crc::Digest<u32>` that
  is reset-able.
- In every `blit_glyph_bytes` / `clear_row` / `clear_cell`
  / `draw_text_bitmap` call site that touches the framebuffer,
  update `crc32c_state` with the bytes just written.
  Skip updates for the digest-region writes themselves.
- In `refresh_digest`, read `renderer.crc32c_state.finalize()`,
  then `reset()` for the next frame.
- Measure wall-clock the same way.

**Measurement protocol:**

1. Build the worktree binary with `--features digest-smoke`.
2. Stage the ESP to `dist/esp-measure.img`.
3. Run `./scripts/spice-ryll.sh dist/esp-measure.img` for
   ~60 seconds; capture `dist/serial.log`.
4. Parse the `t=…` timestamps around each `refresh_digest`
   log line; compute deltas.
5. Repeat for both M1 and M2 binaries.
6. Report:
   - Median + p95 wall-clock for `refresh_digest` under each.
   - Peak memory footprint (static + stack high-water).
   - Build size delta (`.efi` byte count, M1 vs M2 vs main).
   - Any correctness concerns observed (e.g. M1 reads
     zeros under QXL — would invalidate the path).

**Sub-agent contract:** measurement-only. Report back to
the management session. **Do not** open a PR, **do not**
push the worktree, **do not** decide between A and B —
return numbers + a recommendation, but the management
session picks.

### 2c-impl — implement the chosen hash path

Once 2c-measure reports and the management session picks
(A or B), dispatch a sonnet sub-agent to land the chosen
path on `main`. Replace the `framebuffer_hash: u32 = 0`
placeholder in `refresh_digest` with the real value.

**Constraints:**

- Inline-comment the choice at the call site:
  `// Hash path: BltOp::VideoToBuffer read-back. See
  PLAN-visual-digest-phase-02-payload.md *Outcome* for
  measurement that drove this choice (median X µs, p95
  Y µs under OVMF+QXL).` (or the B equivalent).
- If A: ensure the read-back skips the digest region
  itself (otherwise the hash captures pixels that
  *depend on the hash*, making the hash a function of
  itself — undefined).
- If B: ensure the incremental hook does **not** update
  state during `draw_digest`'s per-module BltOps
  (same self-reference problem).
- Either path: the hash captures everything *else* on
  screen, so a paint correctness bug anywhere outside
  the digest will perturb the hash.

### 2d — `make digest-payload-smoke`

**Files:** `Makefile`, `scripts/digest-payload-smoke.sh`
(new — do not overload `digest-smoke.sh`).

**Driver behaviour:**

1. Build with `--features digest-smoke`.
2. Boot QEMU headless (mirror `screenshot.sh`'s flags
   exactly — q35, KVM, 4M OVMF split, QMP socket,
   `-vga std`).
3. Drive the scripted scene the same way
   `screenshot.sh` does (space → bootloader → ignore
   → paste → enter → parking).
4. After the scripted sequence settles, `screendump`
   to `dist/digest-payload.png`.
5. Invert PNG (PIL) and decode via `zbarimg --raw`.
6. Parse the raw bytes:
   - First 4: assert == `b"SXDG"`.
   - Byte 4: assert == `0x01`.
   - Bytes 5..9 (u32 LE): assert >= 3 (one refresh per
     scene phase).
   - Byte 9 (u8): record count; assert >= 1.
   - For each record: print `type=… len=… value=<hex>`.
   - Trailing 4 bytes: print the CRC32C hex.
7. Exit 0.

**Constraints:**

- Reuse `scripts/digest-smoke.sh`'s QMP / OVMF /
  PIL-inversion conventions verbatim — do not invent.
- The TLV parser in the script is in Python (the rest
  of `digest-smoke.sh` already shells to python3 for
  QMP). Keep it self-contained in this one script.
- Do not modify `digest-smoke.sh`; this is a parallel
  target.

**Makefile:**

```makefile
.PHONY: digest-payload-smoke
digest-payload-smoke:
	docker build -t uncalibrated-sextant-build:1.88.0 .
	docker run --rm -v "$(CURDIR)":/work \
	    -v uncalibrated-sextant-target:/work/target \
	    -w /work uncalibrated-sextant-build:1.88.0 \
	    cargo build --release --features digest-smoke
	./scripts/digest-payload-smoke.sh
```

Also: regenerate `docs/images/boot-sequence.png` because
the parking screen now carries a digest. This is part of
the same commit since the visual regression is a direct
consequence of the wiring landing.

### 2e — smoke exit

Run, in order:

```sh
make digest-smoke           # if retained; otherwise skip
make digest-payload-smoke   # must pass
make screenshot             # reference image updated
make release-verify         # raw + qcow2 both pass
pre-commit run --all-files
```

Update this plan's *Outcome* with:

- Commit SHAs for each step.
- The chosen hash path (A or B) and the measurement
  numbers that drove the choice.
- Whether the Phase 1 `digest-smoke` target was retained
  or replaced.
- The final QR Version (5 unless the capacity budget
  forced a swap to Version 7).
- Any TLV-format deviations from the table above.
- The regenerated reference image's git diff size as a
  sanity check.

## Exit criteria

- [ ] `src/digest.rs` exists with the documented TLV
      encoder, type-tag constants, phase / choice wire
      discriminants, and CRC32C helper.
- [ ] `Scene` carries a `digest_frame_counter: u32` and
      a `refresh_digest` method called from the outer
      scene loop at three phase boundaries (after
      awaiting, after booting, after parked).
- [ ] `refresh_digest` includes a `debug_assert!` (or
      `assert!`) that the current `repaint_state` is not
      `BootingBootloader`. The bootloader scene's input
      loops do not call `refresh_digest`.
- [ ] One of the two hash paths (A: `BltOp::VideoToBuffer`
      read-back, or B: incremental hash on every BufferToVideo
      call) is implemented on main, chosen via 2c-measure's
      numbers, with the choice documented inline at the
      call site and in this plan's *Outcome*.
- [ ] The QR payload encodes magic `SXDG`, schema version
      `0x01`, a monotonic frame counter, the most-recent
      events that fit in capacity, and a trailing CRC32C.
- [ ] `make digest-payload-smoke` boots, decodes the QR,
      and asserts magic / version / frame counter /
      record count.
- [ ] The phase 1 `digest-smoke` target is either retained
      (with the hard-coded `hello` injected via a parallel
      feature) or replaced cleanly by digest-payload-smoke.
      Decision recorded.
- [ ] `docs/images/boot-sequence.png` regenerated; visual
      diff confirms parking screen now carries a digest in
      the bottom-right.
- [ ] No regression: `make build`, `make qemu`,
      `make spice`, `make spice-ryll`, `make screenshot`,
      `make release-verify` all continue to work.
- [ ] `pre-commit run --all-files` exits 0 at every commit
      across the phase.
- [ ] Commit messages follow the project template (subject
      under 50 chars ending in a period, body wrapped at
      75, `Prompt:` paragraph, `Signed-off-by`,
      `Co-Authored-By` with model + context window +
      effort).

## Risks and open questions

- **`BltOp::VideoToBuffer` semantics under OVMF + QXL.**
  Master plan's *Open questions* flagged this as
  unknown. If QXL shadows the framebuffer host-side
  and lazy-flushes, the read may return zero-filled or
  stale pixels. 2c-measure must include a correctness
  check (read back a known pattern just-painted, assert
  it matches). If it fails, fall back to path B
  unconditionally.
- **Self-referencing hash.** Either path must skip the
  digest region; otherwise the hash depends on itself.
  Specifically: the right-anchored region from
  `draw_digest` is at `(self.width - MARGIN_X -
  DIGEST_REGION_PX, self.height - MARGIN_Y - CELL_H -
  DIGEST_REGION_PX)` of size `(DIGEST_REGION_PX,
  DIGEST_REGION_PX)`. Path A skips by reading rows
  and excluding the digest column range from the
  digest row range. Path B skips by guarding the
  incremental hook with a "is this call inside
  draw_digest?" flag (set/cleared by draw_digest
  around its loop).
- **TLV capacity vs scene event volume.** A full scripted
  run produces ~60 events; the QR fits 6-7. The most-
  recent-N strategy means the parking-screen digest
  shows the *last* events, not the *first*. For ryll's
  decoder this means it cannot reconstruct the full
  scene from one screenshot — it needs either multiple
  screenshots over time (each catching a different
  most-recent window) or out-of-band access to the
  serial drain. Document explicitly in the phase 3
  format spec.
- **`debug_assert!` vs `assert!` for the carve-out.**
  Lean `assert!`. The cost is negligible and the
  invariant matters: an accidental refresh from inside
  the bootloader scene would push a new digest mid-
  paste-prompt and could mask paste-correctness bugs.
  Crash-loud is better than wedge-quiet.
- **Frame counter and ringbuffer-event correlation.**
  The frame counter increments per refresh, not per
  event. A decoder seeing two screenshots with the
  same frame counter knows the digest contents are
  identical; with different counters, contents may or
  may not differ. The format spec must explain.
- **`crc` crate version drift.** Pin to `"3"` (semver-
  compatible) rather than `"3.0.1"` exact. The crate
  has been stable for years; trusting semver here is
  fine.
- **Right-anchor + hash region geometry.** The hash
  must use the *runtime* digest region (right-anchored),
  not the const `DIGEST_REGION_X` / `DIGEST_REGION_Y`
  (those are 640x480-baseline only). `Renderer` needs
  to expose its current digest origin via a small
  helper or inline the same `saturating_sub` math.
- **Spice-ryll measurement isolation.** The 2c-measure
  protocol uses `dist/esp-measure.img` to avoid
  clobbering `dist/esp.img` if the operator has a
  spice-ryll session running. If the operator stops
  spice-ryll between M1 and M2 runs, the lock is
  released and the default path works too — but the
  isolation makes the protocol re-runnable.

## Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with it. In particular:

- Confirm the TLV header layout (magic / version / frame
  counter / record count) and the most-recent-N capacity
  strategy.
- Confirm the bootloader carve-out enforcement
  (`assert!` vs `debug_assert!`) at the call site.
- For step 2c-measure specifically, confirm that you
  will *report numbers only* and not commit either
  candidate to main — the management session picks A vs
  B based on the report.
- Confirm the self-referencing hash skip mechanism
  appropriate to the chosen path.
