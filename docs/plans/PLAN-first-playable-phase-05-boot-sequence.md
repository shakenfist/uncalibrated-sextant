# First playable — phase 5: Boot sequence scene

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read `DESIGN.md` carefully — in
particular:

- *Connection handshake (avoiding the UEFI-fast / client-slow race)*
- *SPICE channel mapping* — so the text content reflects real
  channel exercises where possible
- *Design principles* (principle 6 is still load-bearing)
- *Voice: unreliable narration leaks* — the tonal contract
- *Boot sequence (first scene)* — the concrete target, including
  the sample output block that this phase implements
- *First milestone scope* — the scoping envelope this phase lives
  inside

Also re-read the Phase 1-4 plans so you understand the existing
project layout; Phase 4 specifically introduced the renderer and
its per-glyph BitBlt discipline that Phase 5 builds on.

This is the phase that makes the project *feel* like the DESIGN.md
artefact rather than a scaffold. Everything prior to Phase 5 has
been infrastructure: Cargo, Docker, Makefile, pre-commit, GOP
renderer. Phase 5 turns it into a short, screenshottable piece of
content — and decides, for real, whether the concept has legs.

Scope discipline is the single biggest risk here. DESIGN.md is a
deep document full of evocative ideas: multi-valent self-doubt,
Kerbside distance measurement, scanline overlays, mode-walk
exercises, audio chimes, "direct hardware control: OK" flipping,
Murderbot voice, cursor corruption. **Almost all of that is out of
scope for Phase 5.** The *First milestone scope* section of
DESIGN.md is the floor and the ceiling. If a design question
surfaces mid-phase, capture it under *Open questions* or *Future
work* here and move on.

Cross-refs in-repo:

- `src/renderer/mod.rs` — the existing `Renderer` with
  `draw_glyph` / `draw_line` / `draw_telemetry_line`.
- `src/renderer/font.rs` — the spleen 8x16 font.
- `src/main.rs` — currently drives a Phase 4 test scaffold via
  `run_test_scaffold`. Phase 5 replaces that function with
  something that orchestrates the real scene.
- `docs/creator-notes/2026-04-concepts.md` — useful source of
  the aesthetic thinking, but **not player-facing content**.

All planning documents go in `docs/plans/`.

## Situation

Phase 4 landed at commits `eeb6503` (renderer scaffolding) and
`3958fec` (docs). The repo now has:

- A working `Renderer` that owns GOP, mode-switches to 1024×768
  with fallback, and draws phosphor-green-on-black glyphs at
  `rgb(51, 150, 51)` via per-glyph `BltOp::BufferToVideo`
  (principle 6).
- A `draw_telemetry_line("LABEL", "STATUS", row)` helper that
  lays out a dot-leader column at column 40.
- A spleen 8x16 font (BSD-2-Clause) in
  `src/renderer/font.rs` covering printable ASCII.
- A Phase 4 test scaffold in `src/main.rs` rendering three
  placeholder lines (`DISPLAY SUBSYSTEM OK`, `POINTER OK`,
  `KEYBOARD OK`) plus `PRESS ANY KEY TO CONTINUE`, paced at
  200 ms, then blocking on keypress and ACPI-shutting-down.
- `pre-commit run --all-files` exits 0; `make qemu` /
  `make release` / `make release-verify` all work.

No event ring buffer exists, no scene state machine, no cursor
blink, no narrator-leak formatting, no AWAITING screen. Phase 5
adds all of those and replaces the placeholder telemetry lines
with the real boot-sequence content from DESIGN.md.

## Mission and problem statement

Make `make qemu` show the opening boot sequence from DESIGN.md
end to end: the AWAITING OPERATOR screen (with blinking cursor
that occasionally glitches) holds until the operator presses any
key, at which point the boot sequence plays out with its
telemetry lines and narrator-leak parentheticals in the voice
described in DESIGN.md; when it completes, the system parks on a
"system online" screen with its own blinking cursor, and a final
keypress ACPI-shuts-down the VM.

Under the surface, an in-memory ring buffer records every
significant event (keypress, line rendered, scene transition).
The buffer is not yet serialised to anything — that's Phase 6 —
but it exists and is populated so Phase 6's dump-to-serial work
has something to drain.

The phase is done when an operator can run `make qemu`, press
any key, watch the sequence play out with pacing and voice that
feels right, reach the parking screen, press a key, and have
QEMU exit cleanly. `pre-commit run --all-files` still passes.

## Open questions

Several decisions have strong defaults but should be confirmed or
iterated at the relevant step. Collecting them up front so they
can be addressed explicitly rather than drifting.

- **First-input handshake: keypress only, or keypress + pointer?**
  DESIGN.md's *Connection handshake* names both as acceptable
  signals for "someone is watching". Keypress-only is what
  Phase 2 already handles via `SimpleTextInputEx`; pointer adds
  the `SimplePointer` protocol which we have not yet touched.
  **Default: keypress-only for Phase 5.** Pointer support
  deferred to a later phase (probably part of the real input-
  channel work when Ryll-driven mode arrives). If the AWAITING
  screen feels awkward without a cursor hint that "click counts
  too", we can reconsider.
- **Audio chime for the DAC line.** DESIGN.md's sample shows
  `AUDIO DAC: 44.1 kHz sine [o]` with "a chime plays". Real audio
  is non-trivial in UEFI (PC-speaker simple but coarse, AC97/HDA
  requires a guest driver). **Default: render the line visually
  but do not actually play sound.** Use `[ ]` or `FAILED` with a
  narrator leak acknowledging it ("(audio subsystem not
  responding — is that a hardware fault or am I deaf?)").
  Keeps scope tight; the fiction works either way. Real audio is
  a later-milestone concern.
- **Mode walk execution.** DESIGN.md's sample and principle 4
  both call for stepping through resolutions as a real SPICE
  display-channel renegotiation test. Actually executing that
  walk means closing and reopening GOP against different modes
  mid-scene — real work. **Default: render the mode-walk lines
  as text only; stay at 1024×768 throughout.** The narrator
  reports what a mode walk *would* show; we do not actually
  walk. Real mode-walking is explicitly Future work in the
  master plan.
- **Cursor glyph shape.** Classic options: solid block, half-
  block, underscore. **Default: solid 8×16 block** for the
  canonical glyph. Broken variants then modify the block in
  small, recognisable ways (missing pixel, smeared edge, shifted
  column, trailing ghost row). This is the easiest to distinguish
  from a "real" character if the glitch overlaps a text row.
- **Blink rate.** **Default: 1 Hz** (500 ms on, 500 ms off).
  Classic CRT terminal cadence; 1.5 Hz feels twitchy.
- **Glitch substitution rate.** Of every N blinks, how many use
  a broken variant? **Default: 1 in 6 blinks (~17%)**, chosen
  from a four-variant pool. Should feel "occasional, not
  constant" — noticeable during a ~5 second AWAITING wait without
  dominating the aesthetic. Tune at Step 3.
- **Sequence ending.** Master plan's Open questions flagged this.
  **Default: park on a "SYSTEM ONLINE. AWAITING INSTRUCTIONS."
  screen with a blinking (and occasionally glitching) cursor.**
  A further keypress triggers ACPI shutdown. Not a loop back to
  AWAITING — one pass through the sequence per QEMU boot is
  plenty.
- **Pacing.** The plan in DESIGN.md says "reasonably fast — tens
  of seconds". Phase 4 uses 200 ms per line. Phase 5 will
  probably want varied pacing: faster for simple OK lines,
  slower for FAILED lines and their narrator leaks, slightly
  longer pauses between "subsystems". **Default first cut: 200
  ms default per line, 400 ms before a narrator leak, 600 ms
  after.** Tune during Step 3 visual review.
- **Module layout.** The renderer is already a submodule under
  `src/renderer/`. Phase 5 code can either join it or live at
  the same level. **Default:** new sibling modules
  `src/event.rs` (the ring buffer + Event enum), `src/scene.rs`
  (scene state machine and the boot-sequence script), and
  `src/cursor.rs` (cursor state + glitch variants + blink
  timing), promoted to `src/cursor/mod.rs` + a generated
  `src/cursor/glyphs.rs` if the glyph data grows. If the total
  non-renderer Phase 5 code stays under ~400 lines, flat-file
  each of those. Decide at Step 2.
- **Timestamps for the ring buffer.** UEFI Runtime Services
  provides `get_time` (wall-clock) and Boot Services provides
  various counters. The simplest source is an incrementing
  `u64` counter of milliseconds since `Renderer::new()`,
  maintained by a small helper that wraps `boot::stall` and
  accumulates. Phase 6 can replace this with real time if
  needed. **Default: monotonic ms counter.**

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | low    | sonnet | none      | Confirm uefi 0.37 surface for SimpleTextInputEx polling (not blocking) and read DESIGN.md's sample output. Brief report. See Step 1 below. |
| 2    | high   | sonnet | none      | Implement the event ring buffer, cursor glitch + blink timing, scene state machine, AWAITING screen, boot-sequence script matching DESIGN.md, and parking screen. Replace `run_test_scaffold` in `src/main.rs`. See Step 2 below. |
| 3    | low    | sonnet | none      | `make qemu` and walk the operator through visual verification. Iterate on pacing, blink rate, glitch rate, narrator-leak formatting, and any specific boot-sequence lines that don't feel right. See Step 3 below. |
| 4    | low    | sonnet | none      | Update `README.md`, `ARCHITECTURE.md`, `AGENTS.md` to describe the scene and ring buffer modules. See Step 4 below. |

### Step 1 — confirm the small bits before writing a lot of code

**Check:**

- uefi 0.37 `SimpleTextInputEx` — how do you poll for a key
  without blocking? We need to render the AWAITING screen's
  blinking cursor in a loop and check for an arriving key on
  each iteration. The `wait_for_event` pattern used in Phase 1
  blocks; for a polling loop we want `read_key()` returning
  `Ok(None)` on no-key. Confirm against docs.rs that this
  is the current shape. (Per Phase 1's own bug-fix entry, the
  original code assumed Ok(None) and it worked in principle —
  we are just confirming nothing has moved.)
- DESIGN.md's *Boot sequence* sample output block. Copy it into
  the Step 1 report verbatim. That block is the authoritative
  content spec for Step 2; write it down now so the Step 2
  sub-agent does not have to re-derive it.

**Do NOT:**

- Read every DESIGN.md section and re-derive open questions;
  the plan above has made the decisions that matter. Confirm
  the two points above and stop.

**Output:** a tight paragraph on the `SimpleTextInputEx`
polling shape plus a verbatim copy of the DESIGN.md sample
block. Under 300 words.

### Step 2 — implement the scene

Create the following Rust modules (flat-file defaults; promote
to submodules if any one exceeds ~400 lines):

**`src/event.rs`** — event ring buffer.

```rust
pub enum Event {
    Keypress { scancode: u16, unicode: char, timestamp_ms: u64 },
    LineRendered { row: usize, timestamp_ms: u64 },
    SceneTransition { from: Phase, to: Phase, timestamp_ms: u64 },
}

pub enum Phase { Awaiting, Booting, Parked }

pub struct RingBuffer<const N: usize> {
    entries: [Option<Event>; N],
    head: usize,
    len: usize,
}

impl<const N: usize> RingBuffer<N> {
    pub const fn new() -> Self { /* ... */ }
    pub fn push(&mut self, event: Event) { /* ... */ }
    pub fn len(&self) -> usize { self.len }
    pub fn iter(&self) -> impl Iterator<Item = &Event> { /* ... */ }
}
```

Capacity 256 is fine. No `std::collections::VecDeque` because
we are `no_std`; a fixed-size array of `Option<Event>` with
head/len indices works.

**`src/cursor.rs`** — cursor glyph variants and blink state.

```rust
pub struct CursorState {
    phase: BlinkPhase,
    variant: GlyphVariant,
    since_last_switch_ms: u64,
    blinks_since_last_glitch: u32,
}

enum BlinkPhase { On, Off }
enum GlyphVariant { Canonical, MissingPixel, SmearedEdge, ShiftedColumn, PhosphorTrail }

impl CursorState {
    pub fn new() -> Self { /* ... */ }
    pub fn tick(&mut self, elapsed_ms: u64) { /* ... */ }
    pub fn current_glyph_bytes(&self) -> Option<[u8; 16]> { /* None when Off */ }
}

pub const GLYPH_CANONICAL: [u8; 16] = [0xFF; 16]; // solid 8x16 block
pub const GLYPH_MISSING_PIXEL: [u8; 16] = /* block with one bit cleared */;
pub const GLYPH_SMEARED_EDGE: [u8; 16] = /* block with right column bleeding */;
pub const GLYPH_SHIFTED_COLUMN: [u8; 16] = /* block shifted 1 pixel right */;
pub const GLYPH_PHOSPHOR_TRAIL: [u8; 16] = /* block with ghost row below */;
```

The variant-selection policy: every blink cycle (once per On
transition) increment a counter; every 6th blink, pick a
variant from the broken pool using a deterministic pseudo-random
sequence (`u32` LFSR is fine — tiny, no_std-friendly, reproducible
for Phase 6 assertion). All other blinks use the canonical glyph.

Add a `Renderer::draw_cursor_glyph(&mut self, bytes: &[u8; 16],
col: usize, row: usize)` method — same as `draw_glyph` but
takes raw glyph bytes rather than a `char`, so cursor variants
do not need to be shoehorned into ASCII codepoints. One BltOp
call per frame, per principle 6.

**`src/scene.rs`** — scene orchestration and the boot-sequence
script.

```rust
pub struct Scene {
    phase: Phase,
    ring: RingBuffer<256>,
    cursor: CursorState,
    clock_ms: u64,
}

impl Scene {
    pub fn run(&mut self, renderer: &mut Renderer) -> ! { /* loop-forever-into-shutdown */ }
}
```

The `run` method:

1. Renders the AWAITING OPERATOR screen. Polls for a keypress on
   each iteration, advancing the cursor state every ~50 ms
   (enough to feel smooth at 1 Hz blink) and redrawing the
   cursor cell. On first keypress, push a Keypress event and
   transition to `Booting`.
2. Walks a hard-coded script of boot-sequence lines (see below),
   rendering each, pushing `LineRendered` events, and
   `stall`-ing per the pacing rules.
3. Pushes a `SceneTransition` event and renders the parking
   screen. Polls for a keypress with the same cursor-blink
   loop.
4. On the parking-screen keypress, calls
   `uefi::runtime::reset(ResetType::SHUTDOWN, ...)`.

The boot-sequence script should match DESIGN.md's sample output
block as closely as reasonably possible, with these adaptations:

- **Audio DAC line:** render but mark FAILED (audio is out of
  scope for this phase per Open questions); include the narrator
  leak `(audio subsystem not responding — is that a hardware
  fault or am I deaf?)`.
- **Mode walk lines:** render as literal text (we do not switch
  modes — see Open questions). Cosmetic telemetry only for now.
- **THRUSTER CONTROL FAILED line:** keep the DESIGN.md-supplied
  narrator leak `(is it possible I am on a test bench in a
  workshop? Why wouldn't they tell me?)` verbatim — that is the
  canonical example of the in-character-speculation-that-is-
  literally-true device.
- **VIDEO SUBSYSTEM direct hardware FAILED → supervised mode OK:**
  keep the DESIGN.md-supplied narrator leak.
- **All lines:** render via `Renderer::draw_telemetry_line` where
  a label+status pair exists; use `Renderer::draw_line` for full-
  width lines (e.g. "SENSORIUM: nominal", "BOOT COMPLETE IN
  23.4s"). Narrator leaks use `Renderer::draw_line` with a
  four-space indent and parentheses; multi-line leaks advance
  the row counter per line.

**`src/main.rs`** — replace `run_test_scaffold` with a single
call:

```rust
let mut r = Renderer::new().expect("renderer init failed");
let mut scene = Scene::new();
scene.run(&mut r);
// unreachable — scene.run ends with ACPI shutdown
```

Keep the `#[entry]` signature and `uefi::helpers::init().unwrap()`
as today. Remove the Phase 4 `stall` pacing and the direct
keypress/shutdown calls — `Scene::run` owns them.

**Non-goals in Step 2** (do **not** implement):

- Actual audio playback
- Actual mode switching
- Scanline overlay (this is the Phase 5 stretch if time permits
  — see Step 3)
- Drifting horizontal tear, out-of-focus halo
- Pointer support for the handshake
- Any serial output
- Any Ryll integration

**Compile and lint but do NOT run.**

- `make build` should succeed.
- `pre-commit run --all-files` should exit 0. Expect new clippy
  lints — the Rust surface is now considerably bigger than in
  Phase 4. Fix in place; only narrow-scope `#[allow(...)]`s with
  a one-line justification.

### Step 3 — visual verification and tuning

Run `make qemu`. Walk the operator through the sequence:

1. **AWAITING OPERATOR screen** should appear quickly after OVMF
   boots. A blinking cursor should be visible, blinking at ~1 Hz.
   The cursor should occasionally render as a broken variant
   (the operator should see at least one glitched blink within
   ~10 seconds of AWAITING).
2. **Keypress triggers scene.** The boot sequence plays out with
   pacing that feels "reasonably fast — tens of seconds".
3. **Narrator leaks** appear indented with parentheses after
   their triggering line, spread across multiple lines where
   appropriate.
4. **Parking screen** appears after BOOT COMPLETE line, with a
   similar blinking-and-glitching cursor.
5. **Final keypress** shuts QEMU down cleanly via ACPI (Phase 2
   behaviour preserved).

**The operator's judgment is load-bearing here.** Sub-agent
cannot see the screen. Ask about:

- Cursor blink rate (too fast / too slow / right)
- Glitch frequency (too rare / too often)
- Pacing between lines and around narrator leaks
- Readability of narrator leaks
- Overall "does it feel like DESIGN.md describes"

Iterate on constants until the operator is happy. Common knobs:
blink period, glitch-per-N-blinks, per-line stall, extra stall
around leaks. No architectural changes — just constants.

If time permits and the operator wants it, add the **stretch
scanline overlay**: a tiled pattern drawn over the whole frame
with a slightly dimmer phosphor. Keep the cursor and text on
top (draw order: clear → text → scanlines → cursor). Out of
scope if it turns into more than a small addition.

### Step 4 — documentation

Update:

- **`README.md`** — Status section notes Phase 5 has landed and
  the binary now plays the full opening sequence. Two or three
  sentences; the existing Why / Building sections remain
  accurate.
- **`AGENTS.md`** — Current phase updates; Where to read first
  gains pointers to `src/scene.rs`, `src/event.rs`,
  `src/cursor.rs`.
- **`ARCHITECTURE.md`** — a new paragraph (after the Phase 4
  renderer paragraph) describing the scene state machine, the
  event ring buffer (capacity, event enum shape, purpose for
  Phase 6), the cursor glitch module, and the fact that the
  AWAITING handshake currently uses keypress-only. Note that
  real audio, mode-walk, and pointer handshake remain deferred.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. Four
phase-specific emphases:

- **Scope discipline.** This is the phase where DESIGN.md's
  rich vision starts to feel achievable. Resist adding any
  content not explicitly in scope. If you find yourself
  tempted, capture the idea under *Future work* here and move
  on.
- **Principle 6 still binds.** Cursor draws and scene
  re-draws must still be per-glyph BitBlts. Do not composite
  the AWAITING screen into an in-memory frame buffer and
  memcpy it — it will still feel subtly wrong on the SPICE
  wire and undermines the whole "we draw like an aged CRT
  naturally would" story.
- **Verbatim quotes from DESIGN.md are the right default for
  narrator leaks.** The owner iterated on those specific
  strings. Don't paraphrase.
- **Step 2 is the biggest sub-agent invocation so far.** Budget
  for one "it compiles but the scene feels wrong" cycle before
  Step 3 passes. Management session reviews the diff carefully
  before committing; Step 3 then drives iterations via
  operator feedback.

## Success criteria

Phase 5 is complete when:

- [ ] `make qemu` opens a QEMU window that shows the AWAITING
      OPERATOR screen with a blinking (and occasionally
      glitched) cursor.
- [ ] A keypress transitions into the boot sequence, which
      plays out with pacing that feels right and includes the
      DESIGN.md-specified narrator leaks.
- [ ] The sequence ends on a parking screen with its own
      blinking cursor; a final keypress ACPI-shuts-down QEMU
      cleanly.
- [ ] The event ring buffer is populated during the run (Phase 6
      will serialise it; this phase only requires it exists and
      is being fed).
- [ ] `make release` still produces working raw and qcow2
      artifacts.
- [ ] `pre-commit run --all-files` exits 0.
- [ ] `README.md`, `ARCHITECTURE.md`, `AGENTS.md` describe the
      scene, cursor, and event modules.

## Bugs fixed during this work

(None yet — populated during execution.)

## Future work

- Real audio chime (PC speaker or AC97/HDA driver).
- Actual mode-walk across OVMF's offered resolutions to
  exercise SPICE display-channel renegotiation.
- Pointer handshake for first-input (SimplePointer protocol).
- Scanline overlay (if Phase 5 stretch doesn't land) and, later,
  drifting horizontal tear + out-of-focus halo.
- Rendered variants for non-ASCII chars if the narrative ever
  wants them (em dash, ellipsis, etc.) — spleen has cp437
  glyphs but our font table only covers ASCII.
- Scene scripting as data rather than code: a `const SCRIPT:
  &[SceneStep]` that the renderer walks. Would make Phase 6's
  Ryll-driven variant (different scenes per test) natural.
  Premature for Phase 5.
- The Kerbside-as-distance-measurement scene from
  `docs/creator-notes/2026-04-concepts.md` — a later-milestone
  piece of content.
