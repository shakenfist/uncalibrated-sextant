# Headless GOP read-back-then-write silently no-ops

Standalone plan, investigation pending.

## Status

**Status: RESOLVED on 2026-05-28. The bug was a one-line
mislabelling, not a read-back interaction.**

`src/digest.rs:111` declared
`DIGEST_PAYLOAD_CAPACITY = 106` with the comment
"QR Version 5 / ECC Medium byte-mode capacity". Per the
QR Code 2005 spec, Table 7, V5 byte-mode capacity is in
fact L=106, M=84, Q=60, H=46 — 106 is the V5/**L**
figure. `src/renderer/mod.rs` separately requested
`QrCodeEcc::Medium` in the `encode_binary` call, so any
encoded payload of ≥85 bytes returned `DataTooLong`,
which the `.expect(...)` on the result then panicked on,
and the `uefi` crate's panic handler hung the firmware.
Because `refresh_digest` is called immediately after
`run_booting` returns (which prints
"OPERATOR ASSISTANCE REQUIRED" as its last line), the
hang appeared as a phase-specific wedge at that exact
on-screen position.

The fix landed on 2026-05-28 was to keep the 106-byte
capacity and drop the encoder to `QrCodeEcc::Low` —
preserving the planned record budget at the cost of QR
error-correction headroom. The doc comments at both
sites now cross-reference each other and explicitly cite
the spec table cell.

**Investigation post-mortem.** Most of the body of this
plan documents falsified hypotheses (path-A/B split,
GOP read-back interaction, `BltOp` flush behaviour, the
`run_booting → run_parked` seam, partial remediations
A–F). None of these were load-bearing on the actual
bug — `BltOp::VideoToBltBuffer` worked correctly the
whole time; `crc32c_framebuffer_excluding_digest`
behaved as designed; the wedge had nothing to do with
seam-painting. The plan kept reaching for firmware-level
explanations because the symptom was a hang and the
hang's on-screen position kept pointing at the same
seam. Two earlier-than-warranted commits ("A is a
partial fix", "bug also reproduces under SPICE")
locked in the read-back framing and made the actual
root cause — a constant mislabelled in `digest.rs` —
invisible to the search.

The bisection that found the real cause was: take a
minimal scene that wedges, replace `refresh_digest`
with `renderer.draw_digest(&[0u8; N])` for varying N,
binary-search N. The wedge threshold landed exactly at
N=85, which matches the V5/M=84 spec entry to the byte.
That precision was what finally rotated the search away
from firmware behaviour and onto the constant. The
broader lesson: when a symptom's position is
suggestive, the falsification bar for the suggested
explanation should be unusually high, and bisecting
*on the bytes flowing through the suspect function*
(not on the call graph above it) gets to a precise
threshold faster.

The body of this plan is kept verbatim below as a
historical record of the false-trail investigation.

## Prompt

> **Historical note (2026-05-28):** the prompt below was the
> original brief written when this bug was thought to be a
> firmware read-back / Blt interaction. It is preserved as
> written so future readers can see the investigation context
> that led down the wrong path. Live references to
> `scripts/digest-smoke.sh`, the `digest-smoke` cargo feature,
> and the `make digest-smoke` target are all stale — those
> were removed in commit `07aadfa` after the actual root
> cause (a mislabelled QR capacity constant) was found and
> fixed in commit `d66c7f6`.

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

### Experimental results (2026-05-27 session)

Tried A, B, C, and D in sequence, all under the
worktree-isolated headless harness at
`/tmp/digest-experiment.sh` (full scripted scene through
to parking + screendump + zbarimg). **All four failed.**

- **A (no-op `BltOp::VideoFill` after read-back loop)**:
  parking screen renders correctly but no QR. zbarimg
  rc=4. Hypothesis 1 disconfirmed for the simplest
  nudge form.
- **B (re-query `current_mode_info` after read-back)**:
  same symptom as A. Disconfirmed.
- **C (50 ms `uefi::boot::stall` after read-back)**:
  not landed because the diagnostic experiments below
  shifted the suspected root cause away from the
  read-back path entirely.
- **D (revert to path B — incremental hash during
  paint)**: cleanly implemented (per-paint
  `hash_buffer` / `hash_fill` hooks on every renderer
  paint method, plus `crc32c_paused` discipline around
  `draw_digest`'s self-paint), `make digest-smoke`
  passed (new CRC `0x0bb0bdef` at AWAITING), but the
  parking-screen QR was still missing. This is the
  **critical finding**: path B never touches the GOP
  read-back path at all, so the bug cannot be specific
  to `BltOp::VideoToBltBuffer`. Reverted (not
  committed).

### Diagnostic experiments after D failed

Direct probing showed the bug is not what we thought.

1. **Two `refresh_digest` calls back-to-back inside
   `run_awaiting` produce `frame=2` in the smoke
   output** (the decoded QR's frame counter advances
   from 1 to 2 between paints, and the second QR
   overwrites the first). So `draw_digest` does NOT
   universally fail on its Nth invocation; back-to-back
   at AWAITING works fine.

2. **Calling `refresh_digest` from inside `run_parked`
   (after `draw_line(SYSTEM_ONLINE_TEXT, …)`) instead
   of between `run_booting` and `run_parked` does not
   help.** Neither the digest QR nor the
   SYSTEM_ONLINE text appears on the parking-screen
   screendump.

3. **Sentinel paints at multiple checkpoints reveal
   the binary never reaches `CKPT-AFTER-BOOTING`** —
   a `draw_text_at(...)` call placed immediately after
   `run_booting` returns. The screendump consistently
   stops at run_booting's last visible line ("EMERGENCY
   SAFE BOOT COMPLETE. OPERATOR ASSISTANCE REQUIRED.")
   and renders nothing painted after it. Increasing
   the harness wait from 5 s to 15 s changes nothing
   in the visible screen.

4. **The user's external observation under
   `make spice-ryll-digest` matches**: they see the
   "operator assistance required" screen with no QR.
   They have never seen "SYSTEM ONLINE. AWAITING
   INSTRUCTIONS." (the actual `run_parked` opening
   text). So the binary is wedged between
   `run_booting` returning and `run_parked` painting
   its first line, in **both** headless and
   interactive SPICE configurations.

### Revised hypothesis

The root cause is **not** a read-back bug. It is a hang
or silent paint-failure that occurs after `run_booting`
returns (or possibly at the end of its POST script's
stall), regardless of whether `refresh_digest` is even
called. `refresh_digest` was the first thing called
after `run_booting` in the experimental harness, which
made it look like the culprit; in fact the same
symptom appears when `refresh_digest` is moved into
`run_parked` (the SYSTEM_ONLINE_TEXT draw call also
fails to paint).

Candidate causes worth investigating next:

- **UEFI resource exhaustion** after many `BltOp`
  operations + `uefi::boot::stall` calls during the
  PRE + bootloader + POST sequence. The bootloader
  alone issues hundreds of `BltOp::BufferToVideo`
  calls during the paste-capture echo loop.
- **A pending key in the input queue** (perhaps a
  spurious paste-character event) consumed by
  `stall_with_keys`' polling and routed somewhere that
  diverges or blocks.
- **A panic inside `digest::encode` or `draw_digest`
  on a payload shape we haven't seen** — though this
  would not explain SYSTEM_ONLINE_TEXT also failing
  to paint after a moved refresh call site.
- **An interaction between `uefi::boot::stall` (boot
  services) and the GOP protocol handle held
  exclusively by `Renderer`** that takes effect after
  some N events.

### Recommended next investigation

- Remove `refresh_digest` entirely and verify
  `run_parked`'s SYSTEM_ONLINE_TEXT renders correctly
  in a control run. If it does, the bug is in
  `refresh_digest`'s call site placement; if it does
  not, the bug is upstream in `run_booting` or the
  bootloader scene and the digest is downstream of
  it.
- If the bug is upstream, instrument
  `bootloader::run_success` and `play_script`'s last
  iteration with sentinel paints to find the exact
  point where the framebuffer stops accepting paints.

### Experimental results (2026-05-28 session)

**Critical methodology error in prior sessions.** All
A/B/D experiments and previous "the bug reproduces
everywhere" findings were against **stale ESP images**:
`make build` writes the binary into a docker volume
(`uncalibrated-sextant-target`), and the host's
`dist/esp.img` is only refreshed when
`scripts/mkesp.sh` runs. The earlier headless harness
ran `mkesp.sh` once, then subsequent `make build`
invocations updated the volume but the headless tests
kept booting the original binary. The previous
findings recorded against these stale builds therefore
do **not** falsify what they appeared to falsify.

After re-running with strict `make build &&
./scripts/mkesp.sh && /tmp/digest-experiment.sh`
ordering, the picture is materially different:

**Control experiment (refresh_digest disabled at all
four call sites):** `run_parked` paints
`SYSTEM ONLINE. AWAITING INSTRUCTIONS.` correctly,
plus all diagnostic CKPT markers. So with
`refresh_digest` out of the picture entirely, the
scene completes cleanly. **`refresh_digest` IS what's
causing the wedge.**

**Remediations re-tested with proper restaging:**

- **A alone (no-op `BltOp::VideoFill` at (0,0) after
  read-back loop in `crc32c_framebuffer_excluding_digest`)**:
  the wedge clears — `run_parked` now runs and
  `SYSTEM ONLINE` paints. But the QR itself still does
  not appear in the bottom-right.
- **A+B (re-query `current_mode_info` layered on A)**:
  same as A — wedge clears, QR absent.
- **A+B+C (50 ms `uefi::boot::stall` layered on A+B)**:
  same — wedge clears, QR absent.

So A alone unblocks the post-`run_booting` GOP state
enough that `run_parked` paints, but **not enough that
`draw_digest`'s own `BltOp` calls land on the
framebuffer**.

**Diagnostic isolation of where paints fail.** Tested
by stripping `draw_digest` to a single explicit
operation:

| Diagnostic                                     | Result          |
|------------------------------------------------|-----------------|
| `draw_digest` does one `BltOp::BufferToVideo` (sentinel block at (200, 600), 32x4 px) | Sentinel **missing** in screendump. |
| `draw_digest` does one `BltOp::VideoFill` at (200, 600), 64x16 px (no buffer ptr)     | Block **missing**. |
| `draw_digest` calls `self.clear()` (whole-screen `VideoFill`) and returns             | Screen NOT cleared — boot transcript stays visible. `clear()` itself did not paint. |
| Same `self.clear()` outside `draw_digest`, in `run_parked` (control)                  | Works — wipes the screen. |
| `BltOp` result captured into `Result`, ok/err branch each paints a marker             | **Neither** marker appears (so the result branch is taken, but neither marker's BltOp paints either). |

**So the failure mode is:** with A applied, any
`BltOp` issued from inside `draw_digest` silently
fails to commit pixels to the framebuffer.
Immediately after `draw_digest` returns, the SAME
`gop` handle's BltOps in `run_parked` commit
correctly. **The bug is genuinely state-bound to
"inside this function call".**

### Revised hypotheses (2026-05-28)

The previous "post-read-back GOP state" hypothesis is
still partly right — A is necessary to unblock the
wedge — but insufficient. There is a **second**
effect that suppresses paints specifically inside
`draw_digest`. Candidate causes:

- **uefi-rs ScopedProtocol borrow interaction.**
  `Renderer` holds `gop: ScopedProtocol<GraphicsOutput>`.
  When `draw_digest` is called with `&mut self`, the
  protocol is borrowed through `self.gop`. If something
  in the call chain (refresh_digest → digest::encode →
  draw_digest) temporarily releases-and-reacquires GOP
  or shadows it, the gop handle inside `draw_digest`
  might be a stale view.
- **`payload: &[u8]` lifetime intersecting with `&mut
  self`.** The shared borrow of `buf` from
  refresh_digest could trigger some optimisation that
  reorders operations; ruled out as unlikely under
  rustc's standard inlining.
- **A "second-call-of-refresh_digest in this boot"
  trigger** specific to the codegen — possibly a
  static-mut or `core::mem::replace` somewhere that we
  haven't found.
- **An OVMF firmware quirk** where Blt operations
  from a particular call address range fail. Hard to
  test without a debugger.

### Recommended next investigation (revised 2026-05-28)

1. **Try calling `draw_digest`'s body inline at line
   317 in `Scene::run` (instead of through
   `refresh_digest`).** If that works, the bug is in
   the indirection layer; if not, it's in the
   draw_digest body itself.
2. **Try moving the gop reference out of Renderer
   and passing it as a parameter.** If that changes
   anything, the issue is borrow-related.
3. **Capture and panic on `result` of a Blt inside
   draw_digest** — see whether it actually returns
   `Err` (currently swallowed by `let _ = ...`).
4. **Test with a simpler payload shape** — bypass
   `digest::encode`, just call `draw_digest(b"x")`
   directly to rule out anything in encoding state.

In the meantime, **A is a partial fix**: it stabilises
the system enough that the parking screen renders
correctly, even if the digest itself does not. That
might be worth committing on its own with a clear
caveat, so production at least doesn't wedge.

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
