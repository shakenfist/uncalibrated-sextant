# Headless GOP read-back-then-write silently no-ops

Standalone plan, investigation pending.

## Status

**Status: Open. Surfaced during PLAN-visual-digest phase 2
step 2d (commit `6b62da2`). Operator confirmation under
`make spice-ryll-digest` then revealed the bug also
reproduces under interactive SPICE — the parking screen
carries no QR despite the post-`run_booting`
`refresh_digest` call firing. The Phase 2 closeout's
"production reality works fine" claim was an incorrect
extrapolation from an AWAITING-screen observation; the
bug affects the production path too.**

## Prompt

Before working on this plan, read the *Known limitations*
section of
[PLAN-visual-digest-phase-02-payload.md](PLAN-visual-digest-phase-02-payload.md)
in full — it has the most detailed account of the
investigation that surfaced this bug. Skim:

- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) —
  particularly `Renderer::crc32c_framebuffer_excluding_digest`
  (added in commit `6221301`), which is the only caller of
  `BltOp::VideoToBltBuffer` in the project.
- [`src/scene.rs`](../../src/scene.rs) —
  `Scene::refresh_digest` (commit `8bef81d`) and its three
  call sites in `Scene::run`. The bug surfaces specifically
  on the second-and-onward `refresh_digest` call within
  the same boot.
- [`scripts/digest-payload-smoke.sh`](../../scripts/digest-payload-smoke.sh)
  — the degraded smoke driver, whose long top-of-file
  comment documents the immediate downstream impact.
- [`scripts/digest-smoke.sh`](../../scripts/digest-smoke.sh)
  — the AWAITING-only smoke that *does* work (one read-back
  per boot).

External references:

- UEFI 2.10 §12.9.2 `EFI_GRAPHICS_OUTPUT_PROTOCOL.Blt()` —
  the spec says `Blt()` "transfers a rectangle of pixel
  data" and is synchronous. No documented requirement for
  a flush between `BltOperation` modes, so the symptom
  appears to be implementation behaviour rather than spec
  ambiguity.
- OVMF source for `OvmfPkg/QemuVideoDxe/` — relevant to
  the bochs / std-vga side; the QXL side lives in
  `OvmfPkg/QemuVideoDxe/Qxl.c`.
- QEMU source `hw/display/qxl-render.c` — the host-side
  surface management that interacts with QXL's
  paravirtualised flush model.

## Symptom

Under `qemu-system-x86_64 -display none` (any `-vga`
backend tested — `std` and `qxl` both):

1. First `refresh_digest` call: reads framebuffer via
   `BltOp::VideoToBltBuffer`, writes 2025 modules via
   `BltOp::BufferToVideo`. Subsequent screendump shows
   the QR rendered correctly. AWAITING smoke captures
   this case and passes (`make digest-smoke`).
2. Second `refresh_digest` call (or any subsequent call
   within the same boot): the read-back appears to
   succeed (CRC is computed and folded into the payload
   bytes), but the immediately-following
   `BltOp::BufferToVideo` writes for `draw_digest` paint
   nothing visible. The rest of the screen (chrome,
   status lines, EMERGENCY SAFE BOOT COMPLETE row) is
   intact; only the digest region's bottom-right
   rectangle stays untouched.

Under interactive `make spice-ryll-digest` (which uses
`-vga qxl -spice port=… -display none` with a SPICE
display server attached): operator-confirmed observation
shows the bug **does** reproduce here too. The AWAITING
screen carries the first-refresh QR correctly, but the
parking screen — after the post-`run_booting` refresh #2
should have painted — shows no QR. The earlier hypothesis
that SPICE's surface-management masked the bug was wrong;
the prior "QR appears under spice-ryll" observation was
captured at AWAITING (one read-back), not parking (two
read-backs).

The bug therefore appears to be universal across every
`-display`/`-vga`/`-spice` combination we have tested.
The original "headless vs interactive" framing was a
red herring driven by an under-specified test. Re-cast:
the failure mode is "second `BltOp::VideoToBltBuffer`
read-back per boot breaks subsequent `BltOp::BufferToVideo`
writes, regardless of the display backend."

## Reproduction

(Assumes `dist/esp.img` is staged from `scripts/mkesp.sh`
against a `--features digest-smoke` build, and no other
QEMU instance holds the image.)

```sh
# Pass case (one read-back):
make digest-smoke
# Final line: "digest-smoke: ok (magic=SXDG version=1 frame=1 records=0 crc32c=…)"

# Pass case (also one read-back):
make digest-payload-smoke
# Same shape — currently holds in AWAITING per its top-of-file comment.

# Failure case (three read-backs — what we want to work):
# Run the same QMP launch as digest-payload-smoke but
# script the full scene through to parking (space → 'i' →
# paste → enter → wait for parking), then screendump.
# The screendump shows a parking screen with no QR in
# the bottom-right.
```

A reduced manual reproduction lives at
`/tmp/qxl-test.sh` from the original investigation; it
should be re-derived if the file is gone.

## What we know

From the 2c-measure and step 2d investigations:

| Configuration                                              | Reads | Result          |
|------------------------------------------------------------|-------|-----------------|
| `-vga std -display none`                                   | 1     | QR painted ✓    |
| `-vga std -display none`                                   | 2+    | QR missing ✗    |
| `-vga qxl -display none`                                   | 1     | QR painted ✓    |
| `-vga qxl -display none`                                   | 2+    | QR missing ✗    |
| `-vga qxl -spice -display none` (`make spice-ryll-digest`) | 1     | QR painted ✓    |
| `-vga qxl -spice -display none` (`make spice-ryll-digest`) | 2+    | QR missing ✗    |

- The bug is universal across the display backends we
  have tested: `-vga std -display none`,
  `-vga qxl -display none`, and `-vga qxl -spice` (with
  a SPICE client attached) all show the same failure
  pattern on the second read-back.
- The bug is therefore most likely in OVMF's GOP driver
  layer (above the display backend), or in uefi-rs 0.37's
  `Blt` wrapper, or in EDK2's `EFI_GRAPHICS_OUTPUT_PROTOCOL`
  implementation that all backends share. The display
  backend is a red herring.
- The hash is computed correctly (the CRC bytes appear in
  the decoded payload from the AWAITING smoke). The bug
  is purely in the write-back side after the second
  read.
- Hash is deterministic across runs at the same mode
  (`0x0f84c2b6` at 1024×768 AWAITING), so
  `VideoToBltBuffer` is returning real pixels rather
  than garbage or zeros.

## Hypotheses

In rough order of plausibility (revised after the
SPICE-also-fails observation):

1. **OVMF / uefi-rs leaves the GOP in a stuck state
   after `VideoToBltBuffer`** — possibly the BltBuffer
   pointer or geometry stays cached, and the next
   `BufferToVideo` either targets the stale buffer or
   no-ops because the protocol thinks it's still in
   read-mode. Testable cheaply with hypothesis-A
   remediation (no-op fill between read-back and next
   paint) and hypothesis-B remediation (re-query
   `current_mode_info` to nudge state).

2. **`BltOp::VideoToBltBuffer` semantically requires a
   specific cleanup or barrier call** documented
   nowhere we've looked, which uefi-rs's safe wrapper
   doesn't emit. Testable by reading the OVMF
   `OvmfPkg/QemuVideoDxe/Gop.c` source and the
   EDK2 `MdeModulePkg/Universal/Console/GraphicsConsoleDxe/`
   to find any state machine governing `BltOperation`
   sequencing.

3. **`BltOp::VideoToBltBuffer` leaves the GOP in a state
   where its `Mode->FrameBufferBase` is stale or
   re-allocated, and subsequent `BltOp::BufferToVideo`
   calls succeed (no error) but write into a buffer
   that the display backend no longer renders from.**
   Testable by re-querying `gop.current_mode_info()`
   after the read-back and re-cap­tu­ring the framebuffer
   base before the next paint.

4. **A timing race in QEMU between QMP `screendump`
   and the in-flight `BltOp::BufferToVideo`s.** Testable
   by adding a `stall(50_ms)` after the last
   `BufferToVideo` of `draw_digest` before letting the
   scene loop return — if the writes simply hadn't
   committed when screendump fired, a sleep would
   surface them. Unlikely given the writes appear lost,
   not delayed.

5. **uefi-rs 0.37 has a bug in how it handles a
   `BltOp::VideoToBltBuffer` followed by a
   `BltOp::BufferToVideo` on the same `GraphicsOutput`
   handle.** Testable by reading the crate source
   around `gop.blt()` and looking for any per-call
   state mutation that's directional.

## Candidate remediations

In rough order of cost-to-fix:

**A. Explicit GOP nudge between read-back and write.**
Insert a no-op `BltOp::VideoFill { color: <transparent>,
dest: (0, 0), dims: (1, 1) }` immediately after
`crc32c_framebuffer_excluding_digest` returns. Tests
hypothesis 1 and is essentially free if it works (one
extra Blt per refresh).

**B. Re-query GOP state after read-back.** Call
`self.gop.current_mode_info()` and discard the result
after `crc32c_framebuffer_excluding_digest` returns;
the act of re-querying may cause the GOP backend to
refresh its internal state. Tests hypothesis 3.

**C. Stall briefly after the digest writes.**
`uefi::boot::stall(50_000)` (50 ms) after `draw_digest`'s
loop. Tests hypothesis 4. Adds 50 ms per refresh × 3
refreshes = 150 ms per boot; visible but tolerable.

**D. Revert to path B (incremental hash).** Path B was
measured at ~195x per-paint slowdown on `clear()`, which
sounded prohibitive when path A appeared to work in
production. Now that path A is confirmed broken on the
second read-back across every backend we have tested,
path B's worst-case cost might be the right trade-off:
the digest payload becomes "what we *intended* to put on
screen" rather than "what GOP says is there", but the
writes that paint the digest itself actually land.
Documented as a step-2c revisit if A/B/C all fail.

**E. Move the smoke to a ryll-driven path.** Have ryll
connect over SPICE, drive keystrokes via the SPICE input
protocol, and decode the QR from a SPICE-captured frame
rather than QMP screendump. **This no longer sidesteps
the bug** (since SPICE also reproduces it), but it does
align the smoke with the production observation path. If
the bug is fixed by A/B/C/D, the ryll-driven smoke is the
right long-term shape regardless. Defer until either the
bug is fixed or ryll-side QR decode lands.

**F. File upstream against OVMF / QEMU.** Construct a
minimal C / EDK2 repro outside uncalibrated-sextant and
file against `qemu-devel` and / or `tianocore` /
`edk2-devel` lists. Highest leverage if a fix lands
upstream; longest timeline.

A and B are cheap enough (each a ~one-line addition
inside `Renderer::crc32c_framebuffer_excluding_digest`)
that they are the obvious first experiments. If either
fixes it, both production (parking-screen digest)
**and** the headless smoke immediately start working.
If neither does, escalate to C (stall), then D (revert
to path B), then F (upstream).

## Success criteria

This plan is closed when:

- [ ] The root cause is identified to at least
      hypothesis-level confidence (i.e. one of the four
      hypotheses above is confirmed by a targeted
      experiment, or a fifth is identified and
      confirmed).
- [ ] Either the bug is fixed in-tree (remediations A,
      B, or C — short term) or worked around by
      switching the smoke to a ryll-driven path
      (remediation D — medium term) or filed upstream
      with a minimal repro (remediation E — long term,
      independent of any in-tree change).
- [ ] `make digest-payload-smoke` is upgraded to drive
      the full scripted scene (space → bootloader → 'i'
      → paste → enter → parking screendump) and asserts
      `frame_counter >= 3 / records >= 1` per the
      original PLAN-visual-digest-phase-02-payload.md
      step 2d brief. The top-of-file divergence comment
      in `scripts/digest-payload-smoke.sh` is replaced
      with a normal docstring.
- [ ] The *Known limitations* section of
      PLAN-visual-digest-phase-02-payload.md is updated
      to reference the resolution (commit / upstream
      bug ID / accepted workaround).
- [ ] If the resolution is a code change, AGENTS.md
      *Most recently landed* gets a one-paragraph
      summary.

## Risks and notes

- **Upstream filing is best-effort.** Even with a
  minimal repro, OVMF and QEMU display-backend bugs can
  take months to triage. Do not block uncalibrated-
  sextant on upstream.
- **Remediation D (ryll-driven smoke) is the natural
  alignment with DESIGN.md's two-channel architecture.**
  It may be worth pursuing in parallel with A/B/C
  investigation regardless, since it's the smoke shape
  we'll want long-term anyway.
- **Do not delete the AWAITING-only smoke when the
  scripted-scene smoke comes online.** The AWAITING
  smoke exercises a different code path (one read-back
  cycle, no scene-loop transitions) and is the only
  current regression net against the "first refresh
  paints correctly" property. Keep both targets.
- **The bug may interact with `Scene::repaint`
  integration in PLAN-visual-digest phase 3.** If
  phase 3 makes `refresh_digest` unconditional (no
  longer feature-gated) and calls it after every
  `Scene::repaint`, the per-boot refresh count climbs
  significantly. The bug surfaces from the second
  call; phase 3 will hit it much earlier than phase 2
  did. Resolving this plan before phase 3 is the
  cleanest sequencing.

## Future work

- Once resolved, consider whether to add a regression
  test specifically for "many sequential
  read-back-then-write cycles work correctly under
  headless GOP" — the bug is exactly the kind that
  could regress silently if OVMF / QEMU is upgraded.

## Back brief

Before executing the investigation, please back brief
the operator with: which hypothesis you'll test first
(probably A — the cheapest nudge), how you'll
distinguish a successful nudge from a different bug
masking the symptom, and what the next experiment is if
A fails. The plan does not commit to a fix path until
the first experiment surfaces evidence.
