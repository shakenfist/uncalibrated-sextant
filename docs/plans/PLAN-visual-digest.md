# Visual on-screen digest — render the ring buffer into the framebuffer

## Prompt

Before responding to questions or discussion points in this
document, explore the uncalibrated-sextant codebase thoroughly.
Read the existing renderer (`src/renderer/mod.rs`), the scene
state machine and its repaint logic (`src/scene.rs`,
particularly `RepaintState` and `Scene::repaint`), the ring
buffer (`src/event.rs`), and the existing serial drain
(`src/serial.rs`). Read [DESIGN.md](../../DESIGN.md)'s
*Two-channel test architecture* and *Wire-level control*
sections in full; this plan implements the *Visual (on-screen
digest)* half. Read [ARCHITECTURE.md](../../ARCHITECTURE.md)'s
final paragraph that lists the on-screen digest as a deferred
component.

Where a question touches `uefi-rs` semantics — particularly
`BltOp::VideoToBuffer` (read-back from the GOP framebuffer)
and the ordering guarantees between successive `Blt` calls
on the same display — research as needed and flag any
residual uncertainty explicitly. Do not assume that what
works on bare hardware works identically under OVMF + QXL,
and vice versa.

External references: the QR Code 2005 specification
(ISO/IEC 18004), and whichever no_std-friendly QR encoder
crate we choose (`qrcodegen` and `qrcode` are the obvious
candidates; both have no_std variants). The decoder side is
ryll's problem; this plan only commits to the *bytes
existing* in the framebuffer, not to the decode path.

All planning documents live in `docs/plans/`. Phase plans
are separate files named `PLAN-visual-digest-phase-NN-...md`
and tracked in the *Execution* table below. One commit per
logical change; each commit builds, passes
`pre-commit run --all-files`, and has a clear message in the
project's existing style.

## Situation

The two-channel test architecture in `DESIGN.md` has been the
intended shape of this harness from the beginning, but only
the groundwork has been laid:

- `src/event.rs` exists and records events into a 256-slot
  ring buffer. `iter()` yields chronological order.
- `src/serial.rs` drains the ring buffer to plain-text
  serial at scene-end. This is the *partial* serial path
  that the eventual gRPC-over-serial transport (lifted from
  `instar`) will replace; it is not framed, not bidirectional,
  and not live.
- The on-screen digest does not exist at all. `ARCHITECTURE.md`
  records it as a deferred phase.

The serial-side gRPC work and the visual-digest work are
separable. They share only the ring buffer (which already
exists and serves both). Either can be built first; the
deciding factor is which one unblocks more downstream value
per unit of plumbing. The visual side is contained in this
repo plus a ryll-side decoder; the serial side requires
lifting the `instar` transport, opening a second guest serial
port, and confronting the Nova API's one-console-per-instance
limit (see *Future work*). The visual side is the cheaper
first move.

This plan also lets us validate, at low cost, whether
read-back-and-hash from `FrameBufferBase` is a useful
integrity test for the SPICE display pipeline. If the
read-back path proves slow or unreliable under OVMF+QXL,
that's a finding worth knowing before we invest in the
larger gRPC transport.

## Mission and problem statement

Implement the visual half of `DESIGN.md`'s two-channel test
architecture: render a periodic, ring-buffer-driven digest
into a designated region of the framebuffer, encoded in a
form that an external decoder (ryll) can read from a
screenshot.

By the end of this plan:

- A designated digest region exists on screen, at a known
  pixel rectangle, that fits the smallest GOP mode this
  harness supports (640×480) without colliding with the
  Shaken Fist logo, the boot transcript, the bootloader
  scene's prompt rows, or the bottom-row mode-switch toast.
- The region is populated with a QR encoding of the last N
  events from the ring buffer, refreshed at a defined cadence,
  and re-rendered after every `Scene::repaint` so it survives
  mode switches.
- Within the same region (or an adjacent one — design call in
  Phase 2) a hash digest of the framebuffer's *non-digest*
  pixels is encoded. The hash is computed by reading the
  framebuffer back via `BltOp::VideoToBuffer` after the rest
  of the frame is drawn, and the hash cells are written
  *last* so they cannot perturb their own input.
- Per Principle 6, the digest is rendered cell-by-cell
  (one `BltOp::BufferToVideo` per QR module), not as a
  monolithic memcpy.
- Repaint integration: a new `RepaintState::digest`-equivalent
  path or — preferred — every existing repaint path includes
  the digest as part of its output, so mode switching never
  leaves the digest stale.
- The locked-bootloader carve-out (`AGENTS.md` *LOAD-BEARING
  CARVE-OUT*) is respected: the digest is not redrawn from
  inside `bootloader.rs`'s polling loops at a cadence that
  would compete for serial / framebuffer time during the
  R/I/A or paste-prompt phases. Either the digest pauses for
  the duration of the bootloader scene, or it is driven from
  the outer scene loop only.
- A host-side smoke check decodes the QR from a
  `make screenshot`-equivalent capture and validates that the
  decoded payload matches the events the ring buffer should
  have produced for that scripted run.
- DESIGN.md's *Visual (on-screen digest)* bullet and
  ARCHITECTURE.md's final-paragraph deferred-component list
  are updated to reflect that this is now implemented.

The serial side (gRPC-over-serial from `instar`) and any
ryll-side test-driver assertions that depend on it are
**explicitly out of scope** for this plan. They are the
subject of a future master plan once this one is complete.

## Open questions

Defaults below are strong but worth confirming or iterating
at the relevant phase. Capture changes inline rather than
letting them drift.

- **QR vs compact text vs other encoding.** **Default: QR.**
  DESIGN.md leaves this open ("QR or compact text"); QR has
  ECC, mature decoders on the ryll side, and survives the
  CRT-scruff overlay we eventually want to layer on top.
  Compact text has no resilience to overlay or downsampling.
  Pick QR unless Phase 1 surfaces a concrete blocker.

- **QR encoder crate.** **Default: `qrcodegen` with
  `no_std` feature.** Smaller surface than `qrcode` and the
  reference encoder is well-understood. Confirm in Phase 1
  that it builds against our `uefi = "=0.37.0"` pin and
  doesn't drag in `alloc`-heavy code we don't already
  tolerate. If it doesn't, fall back to `qrcode` or
  hand-roll a minimal encoder for fixed payload sizes.

- **Region location.** **Default: bottom-right corner,
  inside the overscan margin, square sized to fit the
  smallest supported mode (640×480).** The top-right is
  occupied by the logo, the top-left by the AWAITING
  cursor, the centre by the boot transcript, the bottom row
  by the mode-switch toast. The bottom-right corner is the
  least-contested rectangle. Confirm in Phase 1 that a
  Version-5 QR (37×37 modules at 4-pixel scale = 148×148
  pixels) fits inside `640 - 2*MARGIN_X - logo_width =
  640 - 32 - 128 = 480` horizontal pixels and
  `480 - 2*MARGIN_Y = 448` vertical pixels with room to
  spare for the toast row.

- **Hash region: separate or QR-internal.** **Default:
  carry the hash inside the QR payload.** A separate hash
  region duplicates the encoder/decoder work and competes
  for the same scarce screen real estate. The QR payload
  becomes `<event_bytes><hash_bytes>`; ryll computes the
  hash of the screenshot's non-digest region after
  decoding, and asserts equality with the trailing bytes.
  The "write hash cells last" requirement reduces to "the
  QR is written last in the per-frame paint sequence,"
  which is the natural place for it anyway.

- **Hash function.** **Default: CRC32C (Castagnoli).**
  Deterministic, no_std-trivial, well-understood, 4 bytes.
  The hash is for integrity-of-display, not cryptographic
  authentication. If a future milestone needs preimage
  resistance (untrusted screenshot pipeline), upgrade to
  BLAKE2s — but document the upgrade reason.

- **Refresh cadence.** **Default: once per `Scene::repaint`,
  plus once per scene-loop tick at the outer level (not
  inside `bootloader.rs`'s loops).** Per-event refresh would
  cause flicker; per-second is too coarse for fast scenes.
  The repaint path is already the synchronisation point for
  framebuffer correctness; piggybacking is cheap.

- **Read-back cost under OVMF+QXL.** Unknown. `BltOp::Vid
  eoToBuffer` may be slow if the QXL driver shadows the
  framebuffer in host memory and round-trips a flush. **No
  default; measure in Phase 2 before committing to a
  design.** If read-back is too expensive to do every frame,
  fall back to *computing* the hash incrementally as we
  draw (every `BltOp::BufferToVideo` updates the hash with
  the bytes we just wrote). That preserves the integrity
  semantic ("the hash is what we *intended* to put on
  screen"), at the cost of no longer testing the read-back
  path itself.

- **Bootloader-scene behaviour.** **Default: digest pauses
  during `bootloader::run` and resumes when it returns.**
  The bootloader scene has tight timing for its R/I/A
  countdown and paste validation; injecting a digest
  refresh from inside its loops risks subtle serialisation
  bugs. The displayed QR can stay frozen on its last value
  for the duration of the bootloader scene without being
  misleading — the scene transition events bracket the
  pause.

- **Host-side decode tooling.** **Default: `zbarimg` (apt
  package `zbar-tools`) invoked from a new
  `scripts/decode-digest.sh`.** It's the canonical
  command-line QR decoder on Debian and matches the
  shellcheck-and-host-tools idiom of the existing
  `screenshot.sh`. If `zbarimg` cannot decode at our chosen
  module-pixel scale, increase the scale or downgrade QR
  ECC level before reaching for a heavier decoder.

- **What goes in the payload.** **Default: a tagged binary
  blob — magic bytes, schema version, monotonic frame
  counter, last-N events as TLV, trailing CRC32C of the
  framebuffer's non-digest region.** N to be sized in
  Phase 2 once we know the ring buffer's typical churn rate
  per repaint and the QR Version's payload capacity. Using
  TLV from the start keeps the wire format extensible
  without re-versioning every time we add an event variant.

- **Cross-repo coordination with ryll.** **Default: this
  plan ships the bytes; ryll's decoder is a separate
  branch in the ryll repo, tracked by their own plans.**
  We coordinate the wire format via a one-page spec
  committed to this repo (`docs/visual-digest-format.md`,
  produced in Phase 2 closeout). No ryll-side code lands
  in this repo. The acceptance gate is a host-side decode
  script that proves the bytes are well-formed.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. Region + QR encoder | [PLAN-visual-digest-phase-01-region.md](PLAN-visual-digest-phase-01-region.md) | Not started |
| 2. Ring-buffer payload + framebuffer hash | [PLAN-visual-digest-phase-02-payload.md](PLAN-visual-digest-phase-02-payload.md) | Not started |
| 3. Repaint integration, format spec, closeout | [PLAN-visual-digest-phase-03-closeout.md](PLAN-visual-digest-phase-03-closeout.md) | Not started |

### Phase 1 sketch — region and QR encoder

Single-purpose phase: prove we can lay a scannable QR into
a chosen region of the framebuffer, at every GOP mode the
harness supports, with a hard-coded payload. No ring-buffer
wiring, no read-back, no repaint integration. Just pixels in
the right place.

- **1a — Pick QR crate, vendor it, prove no_std build.**
  Add the chosen QR encoder to `Cargo.toml` with appropriate
  feature flags. Confirm `make build` still succeeds inside
  the `rust:1.88-slim` image with no new toolchain ask.
  Confirm clippy / rustfmt still pass.

- **1b — Region geometry constants.** Add `DIGEST_*` pixel
  constants to a sensible module (probably
  `src/renderer/mod.rs` next to `MARGIN_X` / `MARGIN_Y`).
  Compute and document the available rectangle at
  640×480 — the smallest mode — and assert at compile time
  via `const _: () = assert!(...)` that the chosen Version
  fits.

- **1c — `Renderer::draw_digest(payload: &[u8])` method.**
  Encodes the payload via the QR crate, then renders the
  resulting bit matrix one module per `BltOp::BufferToVideo`
  call (Principle 6). Black-and-white only — match the
  existing palette's black background, use the
  phosphor-green foreground. Quiet zone is the overscan
  margin already built into the region geometry.

- **1d — Smoke target.** A new `make digest-smoke` target
  that boots the binary headless, holds in AWAITING (which
  never paints anything beyond the cursor and the logo), and
  injects a hard-coded `Renderer::draw_digest(b"hello")`
  call from a debug-build-only path or a feature-gated test
  scene. QMP `screendump` to PNG; `zbarimg` decodes; assert
  the decoded bytes are exactly `b"hello"`.

End-of-phase exit: `make build`, `make screenshot`, and the
new `make digest-smoke` all pass; pre-commit clean; the
binary on `make spice-ryll` shows a hard-coded QR in the
chosen region across at least three different mode keys
(`'1'`, `'3'`, `'5'`). **Recommend planning at medium
effort (sonnet)** — the QR rendering path is mechanical
once the crate is selected; the design call is the region
geometry.

### Phase 2 sketch — ring-buffer payload + framebuffer hash

The harder phase. Three concerns to resolve in order:
payload encoding, refresh wiring, hash mechanism.

- **2a — Payload wire format.** Define the TLV layout
  (magic, version, frame counter, last-N events as
  type-length-value tuples, trailing CRC32C). Implement
  encoding from the ring buffer in a no_std-friendly way
  (probably a fixed-size `[u8; N]` stack buffer; no `Vec`).
  Pure logic, host-testable — add `cargo test` coverage on
  the host (the workspace-split deferred from earlier
  milestones is still deferred; a `#[cfg(test)]` module
  inside the relevant file is fine for now).

- **2b — Refresh wiring at the scene-loop level.** Wire
  `Scene::run_*` to call `Renderer::draw_digest` once per
  outer-loop tick, with the payload encoded from the
  current ring buffer. Add a deliberate carve-out so
  `bootloader::run` does not see digest refreshes during
  its tight loops. Verify with `make spice-ryll` that the
  QR updates as scenes progress and freezes appropriately
  inside the bootloader scene.

- **2c — Framebuffer hash via `BltOp::VideoToBuffer`.**
  Try the read-back path first. After the per-frame paint
  is otherwise complete (chrome + boot script row + cursor
  + toast if present, but *before* `draw_digest`), read
  the entire framebuffer back into a stack or static
  buffer, exclude the digest region, compute CRC32C,
  splice it into the payload bytes, then call
  `draw_digest(payload)`. Measure the cost; if
  unacceptable, switch to the incremental-hash fallback
  per the open question above and document the swap in
  Phase 2's plan file.

End-of-phase exit: the displayed QR carries a real
ring-buffer-driven payload; ryll-or-zbarimg can decode it;
the trailing CRC matches an independent recomputation by
the host-side smoke script; `make screenshot` regression
still passes (the static parking-screen QR will of course
be different from before, but stable across runs given the
scripted input). **Recommend planning at high effort
(opus)** — this phase has subtle ordering, no_std-buffer
sizing, and potentially a measurement-driven design pivot.

### Phase 3 sketch — repaint integration, format spec, closeout

- **3a — Repaint path integration.** Make `Scene::repaint`
  call `draw_digest` after replaying the boot script
  prefix, and update each `RepaintState` variant if any
  needs to carry digest-relevant state (probably none —
  the digest pulls from the ring buffer directly, which
  survives the framebuffer wipe). Press `'3'` mid-scene
  under `make spice-ryll`, confirm the QR is repainted in
  the new mode's coordinates with the correct payload
  for the events that occurred up to that point.

- **3b — Format spec.** Write
  `docs/visual-digest-format.md`. One page: magic, version,
  frame counter, event TLV catalogue (one row per
  `Event` variant currently emitted), CRC32C placement.
  Cross-reference the Rust types in `src/event.rs` so
  drift is detectable by a human reviewer. Link from
  `DESIGN.md`'s *Two-channel test architecture* section
  and from `ARCHITECTURE.md`.

- **3c — Closeout.** Mark Phase 3 Complete in this plan's
  *Execution* table; tick *Success criteria* with commit
  refs; update `docs/plans/index.md` master-plans row to
  *Complete*. Update DESIGN.md to remove the
  "or compact text" hedge if QR was the choice; update
  ARCHITECTURE.md's deferred-components paragraph to drop
  the on-screen digest entry; update AGENTS.md *Most
  recently landed* with a one-paragraph summary; update
  README.md to mention the digest region briefly.

End-of-phase exit: every `make` target in `AGENTS.md`
*Build commands* still works; `make digest-smoke` and
`make screenshot` both produce decodable digests with
expected payloads; docs reflect implemented state. **Recommend
planning at medium effort (sonnet)** — repaint integration
needs care but the heavy thinking is in Phase 2; docs are
mechanical.

## Agent guidance

### Execution model

Same as previous plans: all implementation work goes to
sub-agents, the management session reviews and commits.
The Phase 2 measurement-driven design call (read-back vs
incremental hash) is the one place where a sub-agent
should *report* before *deciding* — return numbers, let
the management session pick the path.

Use `isolation: "worktree"` for Phase 2 specifically, since
the read-back approach may need to be reverted in favour of
incremental hashing and a worktree makes that revert clean.
Phases 1 and 3 are safe in the main tree.

### Planning effort

- Phase 1: medium (sonnet) — QR rendering path is well-
  understood; the design call is region geometry, which is
  bounded and verifiable.
- Phase 2: high (opus) — payload encoding is fiddly,
  ring-buffer-to-stack-buffer in no_std is fiddly,
  read-back timing is unknown territory under OVMF+QXL.
- Phase 3: medium (sonnet) — repaint integration mostly
  follows existing `RepaintState` patterns; docs are
  mechanical.

The master plan itself was written at high effort — it
required reading DESIGN.md, ARCHITECTURE.md, AGENTS.md,
the existing renderer / scene / event / serial sources,
and the prior plan structure to find the right scope and
phase boundaries.

### Step-level guidance

Each phase plan should include the standard step table:

```
| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | ... |
```

### Management session review checklist

Same as earlier milestones:

- [ ] Files that were supposed to change actually changed.
- [ ] No unrelated files modified.
- [ ] `pre-commit run --all-files` clean.
- [ ] Code matches intent of brief.
- [ ] Commit message follows project template (including
      Co-Authored-By with model, context window, effort,
      and any other settings).

Phase-2-specific checks:

- [ ] The chosen hash path (read-back vs incremental) is
      documented inline at the call site, not just in the
      plan.
- [ ] The `bootloader::run` carve-out for digest refreshes
      is asserted (a comment is fine; an assertion is
      better) so a future scene runner can't accidentally
      re-introduce a refresh from inside a bootloader-style
      tight loop.

## Administration and logistics

### Success criteria

This plan is complete when:

- [ ] A QR digest renders in a fixed region at every GOP
      mode the harness supports (640×480 through
      1920×1080), without colliding with the logo, boot
      transcript, bootloader scene rows, or mode-switch
      toast.
- [ ] The digest payload reflects the ring buffer's last N
      events, refreshed at the scene-loop cadence and
      paused for the duration of `bootloader::run`.
- [ ] The digest is rendered cell-by-cell per Principle 6
      (one `BltOp::BufferToVideo` per QR module).
- [ ] The displayed QR carries a CRC32C of the framebuffer's
      non-digest pixels, computed either via
      `BltOp::VideoToBuffer` read-back or the incremental
      fallback, with the choice documented inline.
- [ ] `Scene::repaint` repaints the digest correctly after
      every mode switch.
- [ ] `make digest-smoke` boots, captures, decodes the QR
      via `zbarimg`, and asserts the payload matches the
      scripted ring buffer for that run.
- [ ] `docs/visual-digest-format.md` documents the wire
      format with a row per `Event` variant.
- [ ] DESIGN.md, ARCHITECTURE.md, AGENTS.md, README.md
      all reflect the implemented state.
- [ ] `pre-commit run --all-files` clean at every commit.
- [ ] Existing `make screenshot`, `make qemu`,
      `make spice`, `make spice-ryll`, `make release-verify`
      all continue to work (the screenshot's reference
      image will change because the digest is now part of
      the parking screen — that update is expected and
      committed alongside Phase 3).
- [ ] `docs/plans/index.md` master-plans row updated to
      *Complete* with the commit range; `docs/plans/order.yml`
      contains the entry added at plan-creation time.

### Future work

- **Serial side of the two-channel architecture
  (gRPC-over-serial from `instar`).** This is the other
  half of DESIGN.md's two-channel design and the natural
  next master plan. It includes lifting the transport
  pattern from `instar`, framing the ring buffer events as
  protobuf messages, opening a second guest serial port,
  and adding ryll-side decoder + assertion logic.

- **Nova API constraint on the second serial port.**
  DESIGN.md notes "Nova supports this via
  `hw:serial_port_count`" — that's libvirt-level true, but
  the OpenStack Compute REST API only exposes one
  serial console per instance via `os-getRemoteConsole`,
  and Kolla-Ansible's `nova-serialproxy` follows that
  limit. Reaching a second console therefore requires
  either a Shaken Fist extension, a non-Nova path (e.g. a
  guest-side agent over the tenant network or virtio-vsock),
  or muxing markers into the primary console. Out of scope
  for this plan but worth flagging early so the serial-side
  master plan starts with eyes open.

- **CRT-scruff overlay interaction.** DESIGN.md's "poor
  video signal" aesthetic uses localised sprite overlays.
  The digest region must be on the *base* layer, not the
  scruff layer, so the QR stays clean. Coordinate when
  the scruff overlay lands.

- **Adaptive refresh.** Currently we redraw the digest
  every scene-loop tick regardless of whether the ring
  buffer changed. A "skip if last-payload-bytes equal"
  optimisation is straightforward and would reduce wire
  bandwidth on the SPICE display channel during quiescent
  periods (parking screen). Defer until refresh cost
  measurably matters.

- **Compact-text fallback encoding.** If a future scene
  needs a digest region too narrow for any QR Version
  (unlikely but possible — e.g. a side-bar layout), a
  compact-text encoding is an alternative. Not built now,
  but the `Renderer::draw_digest` signature should be
  encoding-agnostic enough that adding a second encoder
  later doesn't require rewriting the call sites.

- **Cryptographic hash upgrade.** CRC32C is fine for
  integrity-of-display. If a future milestone passes
  screenshots through an untrusted pipeline (e.g.
  third-party CI artefact uploads), upgrade to BLAKE2s
  with a project-scoped key.

### Bugs fixed during this work

(None yet — populated during execution.)

### Documentation index maintenance

On creation of this plan, `docs/plans/index.md` and
`docs/plans/order.yml` get a new master-plan entry. As
phases complete, update the status column in the
*Execution* table above and in `index.md`.

When all phases of this plan complete, update `index.md`'s
status column to *Complete* with the commit range.

### Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with it. In particular:
confirm the bootloader-scene digest pause, confirm the
chosen hash path (read-back vs incremental) before
implementing it in Phase 2, and confirm the region
geometry fits at 640×480 before committing to a QR Version
in Phase 1.
