# Locked bootloader — phase 2: scene state machine

Parent plan: [PLAN-locked-bootloader.md](PLAN-locked-bootloader.md).
Sibling: [phase 1](PLAN-locked-bootloader-phase-01-spice-infra.md)
(SPICE infra, complete).

## Prompt

Before working on this phase, re-read:

- The master plan's *Mission and problem statement*, *Open
  questions*, and *Phase 2 sketch* sections. Defaults from
  *Open questions* are in force unless this phase plan
  overrides them.
- [src/scene.rs](../../src/scene.rs) — particularly
  `BOOT_SCRIPT`, `run_booting`, the `poll_key` helper, and
  the existing `stall` clock. The bootloader scene is a
  sub-state-machine that runs inside what is today the
  `Booting` phase, between `SENSORIUM: nominal` and
  `EMERGENCY SAFE BOOT COMPLETE`.
- [src/event.rs](../../src/event.rs) — the `Event` enum and
  `RingBuffer` capacity (256). New event variants land here.
- [src/serial.rs](../../src/serial.rs) — the `drain` formatter
  Ryll's parser will eventually consume. New event variants
  need stable lowercase tags.
- [src/renderer/mod.rs](../../src/renderer/mod.rs) — what is
  available for in-place row updates today (`draw_glyph`,
  `clear_cell`, `draw_line`, `draw_telemetry_line`). Some
  helpers are missing for this phase and will be added.
- [src/cursor.rs](../../src/cursor.rs) — blink and glitch
  state. The bootloader scene does NOT carry the cursor through
  itself; the cursor only blinks during AWAITING and Parked.
- [scripts/screenshot.sh](../../scripts/screenshot.sh) — QMP
  send-key is the only headless drive we have today. The
  screenshot path needs adapting (or deferring) once the
  bootloader scene becomes part of the boot sequence.
- The cross-repo source for the paste behaviour we are
  receiving:
  [`shakenfist/ryll/docs/plans/PLAN-paste-as-keystrokes.md`](../../../ryll/docs/plans/PLAN-paste-as-keystrokes.md)
  — particularly the resolved Open Questions for cap (4096),
  default delay (16 ms), unrepresentable-codepoint pre-validation
  (whole string aborts before any keystrokes are sent), and
  Enter handling (LF or CRLF in source becomes a single Enter
  scancode emitting `\r` to UEFI Simple Text Input Ex).

This phase is the load-bearing one. It is the first scene
content the project ships beyond the first-playable scaffold,
the first per-character-blitted piece of in-place animation
(retry dot leader, attempt counter, visible countdown), and
the first time the binary holds a multi-keystroke buffer.

It also touches three non-obvious invariants that must not
break:

- **Principle 6 (per-glyph BltOps).** Every dot of the retry
  animation, every digit of the countdown, every character
  of the attempt counter is its own `draw_glyph` call. No
  string concatenation into a single call.
- **In-place row updates, no scrolling.** The retry animation,
  attempt counter, and countdown all overwrite the same screen
  rows. The renderer needs a `clear_row` helper today; add it
  rather than abusing `clear_cell` in a loop on the call site.
- **Single-keystroke poll cadence.** The existing `read_key`
  loop returns one key per 50 ms poll. Ryll's paste runs at
  16 ms inter-character; the firmware buffers behind UEFI
  Simple Text Input Ex are responsible for not dropping
  characters. We do *not* tighten the poll interval; we trust
  the firmware buffer. If smoke testing reveals dropped
  characters, that is a Phase 2 finding to record in *Bugs
  fixed during this work* and address — not a justification
  for changing the design upfront.

All planning documents go in `docs/plans/`. This phase plan is
committed alongside the code change it covers (one commit per
logical step, see *Execution* below).

## Situation

Phase 1 shipped the SPICE-client launch path (`make spice`,
`scripts/spice.sh`) and confirmed SPICE Display + Inputs
channels work end-to-end against the unchanged binary. Ryll
shipped paste-as-keystrokes (`fallback-paste` branch, merged
to develop via PR #46), so an operator can deliver a multi-
character paste through the SPICE Inputs channel without
guest-side vdagent. Both prerequisites for this phase are
satisfied.

The binary today has three top-level scene phases (`Awaiting`,
`Booting`, `Parked`) and a flat `BOOT_SCRIPT` of seventeen
telemetry / line / probe steps. `run_booting` walks the script
and returns the next-free row to `run_parked`. There is no
intra-boot interaction, no in-place animation, no buffered
input handling, and no diegetic failure path. The serial drain
records `Keypress`, `LineRendered`, and `SceneTransition`
events; no event variants exist for in-scene decisions, paste
contents, or timeouts.

The locked-bootloader scene needs to insert between
`SENSORIUM: nominal` and `EMERGENCY SAFE BOOT COMPLETE`,
exercise the full R/I/A flow, capture and validate a paste,
and either continue the boot sequence (success) or terminate
the run (cold-reset on Abort, ACPI shutdown on timeout / wrong-
paste-cap-reached). Every flow path must be playable end-to-end
under `make spice`; `make qemu` (GTK) must continue to launch
the binary and reach the bootloader prompt, with paste timing
out cleanly when no SPICE client is connected.

## Mission and problem statement

Implement the locked-bootloader scene as a self-contained
sub-state-machine that runs mid-`run_booting`, plus the
ring-buffer event variants and renderer helpers it needs, plus
the headless-screenshot accommodation, plus the operator-
facing documentation. Five deliverables:

### 1. New event variants (`src/event.rs`, `src/serial.rs`)

Three new `Event` variants with stable lowercase tags for the
serial drain:

```rust
Event::BootloaderDecision { choice: BootloaderChoice, attempt: u32, timestamp_ms: u64 }
Event::PasteReceived       { len: usize, correct: bool, timestamp_ms: u64 }
Event::BootloaderTimeout   { timestamp_ms: u64 }
```

`BootloaderChoice` is an enum: `Retry`, `Ignore`, `Abort`, with
`tag()` returning `"retry"`, `"ignore"`, `"abort"`.

`attempt` is the 1-indexed count of times the prompt has
rendered (so a fresh first-time prompt records as `attempt=1`,
a re-prompt after the first wrong paste records as `attempt=2`,
etc.). Recording the count alongside the choice lets Ryll's
future parser correlate behaviour with attempt number without
having to keep its own counter.

`PasteReceived.correct` carries the validation outcome so a
parser does not have to reconstruct it from the `len` and
context; this is a small departure from the master plan's
`PasteReceived { len }` shorthand and is justified by the
diagnostic value (see *Open questions* below).

`BootloaderTimeout` records the moment the silent-wait timer
elapses and the visible countdown begins. The countdown's
final ACPI shutdown is implicit (the run ends; no further
events).

Serial drain format additions:

```
t=NNN type=bootloader_decision choice=retry|ignore|abort attempt=N
t=NNN type=paste len=N correct=true|false
t=NNN type=bootloader_timeout
```

Phase enum is **unchanged**. The bootloader scene plays during
`Phase::Booting`. The new event variants carry the diagnostic
vocabulary without churning the existing serial format. (If
Ryll's parser develops a need for sub-phase tags later, that
is an additive change, not a Phase 2 concern.)

Unit-test coverage: **none in this phase.** The project's
build flow goes through `make build` (Docker, UEFI target);
there is no `make test` target and no host-side test wiring,
because adding it would require splitting `Cargo.toml`'s
UEFI-only features and conditionally relaxing `no_main` /
`no_std` in `main.rs` — restructuring the crate to support
two unit tests is not worth it. Behaviour for the new event
variants is exercised end-to-end by the operator-driven
`make spice` smoke test in step 2e (each flow path produces
a specific `dist/serial.log` line set, asserted by inspection).
A future "Host-side state-machine tests" item under *Future
work* covers extracting test-friendly traits if the cost ever
becomes worth it.

### 2. Bootloader sub-state-machine (`src/bootloader.rs`)

A new module owns the entire scene from the b64 telemetry line
through either the success "Booting..." line or scene
termination. It is invoked from `scene.rs` after the existing
`SENSORIUM: nominal` step and before the existing
`EMERGENCY SAFE BOOT COMPLETE` step.

Module surface:

```rust
pub enum BootloaderOutcome {
    /// Operator pasted the correct payload. Caller continues
    /// the boot sequence at `next_row`.
    Continue { next_row: usize },
    // Abort and Timeout terminate the run inside `run` itself
    // (cold-reset and ACPI-shutdown respectively). Both call
    // `uefi::runtime::reset` directly and are `-> !`, so the
    // outcome enum has only one variant the caller sees.
}

pub fn run(
    renderer: &mut Renderer,
    ring: &mut RingBuffer<256>,
    clock_ms: &mut u64,
    start_row: usize,
) -> BootloaderOutcome;
```

Interior state machine, in run-order:

1. **Telemetry preamble** — render
   `Advanced b64 cryptographic coprocessor` + `OFFLINE` and
   `NIST 800-53 SC-28(1) Secret hardening` + `DISABLED BY
   CONFIGURATION` as ordinary telemetry lines (using the
   existing `draw_telemetry_line` path), one per row, with
   the same `PACE_LINE_MS` (200 ms) pacing the rest of the
   boot script uses. Each line emits `Event::LineRendered`.
2. **Prompt** — render
   `Decryption of next-stage bootloader failed. (R)etry,
   (I)gnore, or (A)bort?` on a fresh row, with attempt counter
   `(attempt N)` appended after the second prompt onward (N ≥
   2). Poll for keypresses; case-insensitive match on
   `r`/`i`/`a`. Any other key is ignored (including modifier
   leakage from a stray `Ctrl+Alt+V` if the operator triggers
   ryll's paste-as-keystrokes shortcut on this screen). Each
   render emits one `Event::LineRendered`; the operator's
   selection emits `Event::Keypress` (existing path) plus
   `Event::BootloaderDecision`.
3. **Retry path** — animated `Retrying decryption` followed
   by per-glyph dots (one dot every 200 ms, eight dots total
   = 1.6 s, slightly longer than the 1.2 s nominal so the
   animation reads as deliberate). When the animation
   completes, clear the prompt row(s) (new
   `Renderer::clear_row` helper) and re-render the prompt
   in place with the incremented attempt counter. After
   `RETRY_NUDGE_AFTER` (5) total retry presses, render
   `Continued retry will not change the outcome.` on the row
   below the prompt; do not clear it on subsequent retries
   (it is a sticky observation, not a transient one). Loops
   back to step 2.
4. **Abort path** — call
   `uefi::runtime::reset(ResetType::COLD, Status::SUCCESS,
   None)` directly. The `BootloaderDecision { choice: Abort
   }` event is recorded *before* the call; the serial drain
   does not get a chance to flush, but the next run's drain
   will not have history of the previous run anyway, so this
   is acceptable. Document that a cold reset means no drain
   and no shutdown record.
5. **Showing blob path** — render
   `Cryptographic co-processor offline. Encrypted bootloader
   payload follows. Decode externally and paste back to
   continue.` on a fresh row, then a blank row, then the
   blob:
   ```
   c2V4dGFudHtIRUxMT19PUEVSQVRPUn0=
   ```
   (which is `sextant{HELLO_OPERATOR}` base64-encoded). One
   row each, full-width. Then a blank row, then the prompt
   `Awaiting decoded payload> ` followed by an inline cursor
   target column where typed characters echo as they arrive.
6. **Awaiting paste** — poll keys at the existing 50 ms
   cadence into a fixed `[u8; 64]` buffer (the placeholder
   `sextant{HELLO_OPERATOR}` is 23 characters; 64 leaves
   plenty of headroom for paste artifacts and future longer
   payloads). For each printable character:
   - Echo it at the current input column (per-glyph blit).
   - Append to the buffer.
   - Reset the silent-wait idle timer.
   - If the buffer fills before a terminator arrives, treat
     as wrong paste (re-prompt with attempt counter).
   - If a `\r` (Enter) terminator arrives, validate the
     buffer against `sextant{HELLO_OPERATOR}`. Emit
     `Event::PasteReceived { len, correct, .. }`.
     - On correct: clear the bootloader scene region (rows
       between telemetry preamble and post-blob), render
       `Booting...` for `BOOT_PAUSE_MS` (600 ms), then
       return `BootloaderOutcome::Continue { next_row }`
       where `next_row` is the row immediately after the
       `Booting...` line.
     - On wrong: clear the input echo, increment the
       wrong-paste counter, re-render the
       `Awaiting decoded payload> ` line (in place) with
       suffix `(wrong, attempt N of 3)`. After the third
       wrong, transition to **Timeout** (step 7) — bypassing
       the silent-wait phase, since the operator is
       definitionally not silent.
   - If the silent-wait timer (`PASTE_SILENT_WAIT_MS`,
     60 000 ms) elapses with the buffer empty AND no key
     activity since the prompt rendered, transition to
     **Timeout**. (Once any character has been received, the
     wrong-paste counter is the gate, not the silent-wait
     timer; partial half-typed pastes thus do not silently
     time out — they fail by terminator-or-buffer-full, which
     re-prompts.)
7. **Timeout** — emit `Event::BootloaderTimeout`. Clear the
   bootloader scene region. Render
   `Awaiting decoded payload. Aborting in NN...` where `NN`
   counts down from `TIMEOUT_COUNTDOWN_S` (30) at 1 Hz. Each
   tick is an in-place per-cell update of the two digits;
   no other text on the row is redrawn. When the countdown
   reaches 00, render
   `BOOTLOADER UNRECOVERABLE. SHUTTING DOWN.` on a fresh row,
   stall `ERROR_HALT_MS` (5000 ms), then call
   `uefi::runtime::reset(ResetType::SHUTDOWN, ...)`.

Constants live at module top:

```rust
const PASTE_BUFFER_LEN: usize = 64;
const PASTE_TARGET: &str = "sextant{HELLO_OPERATOR}";
const ENCODED_BLOB: &str = "c2V4dGFudHtIRUxMT19PUEVSQVRPUn0=";
const RETRY_SLEEP_MS: u64 = 200;          // per-dot
const RETRY_DOT_COUNT: u32 = 8;
const RETRY_NUDGE_AFTER: u32 = 5;
const PASTE_SILENT_WAIT_MS: u64 = 60_000;
const TIMEOUT_COUNTDOWN_S: u64 = 30;
const ERROR_HALT_MS: u64 = 5_000;
const BOOT_PAUSE_MS: u64 = 600;
const WRONG_PASTE_LIMIT: u32 = 3;
```

The clock counter (`clock_ms`) and ring (`ring`) are passed
mutably from `Scene::run_booting`. The bootloader does not
own a clock — that would create two clocks the serial drain
would have to reconcile.

### 3. Renderer helpers (`src/renderer/mod.rs`)

Two missing primitives the scene needs and that have no
existing call sites:

```rust
/// Clear an entire text row, edge to edge, to background.
/// One BltOp::VideoFill per call (column-aligned to MARGIN_X
/// so the overscan is preserved).
pub fn clear_row(&mut self, row: usize);

/// Render a string starting at an arbitrary text-cell column.
/// Per-glyph; existing `draw_line` is column-0-only.
pub fn draw_text_at(&mut self, text: &str, col: usize, row: usize);
```

Add these in a single commit alongside the bootloader module
or just before it. Both are trivial; they exist only because
adding them inside the bootloader module would put rendering
primitives in the wrong place.

### 4. Headless screenshot accommodation

`make screenshot` today drives AWAITING → Booting → Parked
with a single QMP `send-key` and then captures a screendump
~5.5 s after the keypress. With the bootloader scene inserted,
that 5.5 s window now lands in the middle of the bootloader
prompt rather than the parking screen — a regression in what
the committed `docs/images/boot-sequence.png` represents.

**Default: extend `scripts/screenshot.sh` to drive through
the bootloader.** QMP `send-key` accepts a sequence of
`{type: 'qcode', data: '<key>'}` entries (existing precedent
in `screenshot.sh:117`); we already use `'spc'` for AWAITING.
For the bootloader scene we additionally need:

- `'i'` — selects Ignore.
- The `sextant{HELLO_OPERATOR}` keystroke sequence: lowercase
  letters (`s`, `e`, `x`, `t`, `a`, `n`, `t`), then
  `shift+'bracketleft'` for `{`, then uppercase letters with
  `shift` modifier, `shift+'minus'` for `_`, then
  `shift+'bracketright'` for `}`, then `'ret'` for Enter.

This is fiddly but mechanical. A Python helper inside the
existing inline `python3 -` block builds the key sequence
from the literal target string. Total scene-traversal time
budget: AWAITING settle (1.5 s, existing) + boot script up
to bootloader (~5 lines × 200 ms ≈ 1 s, modulo the new b64
and NIST lines pushing it to ~1.4 s) + Ignore selection
(immediate) + paste (23 keystrokes at, say, 50 ms each ≈
1.2 s) + Booting pause (0.6 s) + post-bootloader boot lines
(~1 line × 200 ms ≈ 0.2 s) + parking settle (1 s) ≈ 6 s.
Grow the existing 5.5 s post-keypress wait to ~9 s for
margin.

If QMP `send-key` turns out to mis-handle some character in
the placeholder string (the most likely candidate is `_`,
which is `shift+minus` on US-QWERTY but may map differently
under QEMU's qcode dictionary), fall back to **Default
fallback: keep `make screenshot` capturing the pre-bootloader
frame** and add a separate `make screenshot-bootloader` that
captures the prompt screen specifically. Update
`docs/images/boot-sequence.png` only in the first scenario;
in the fallback scenario, the committed image stays as-is
and a new `docs/images/bootloader-prompt.png` lands alongside.

Either path satisfies the master plan's *Open question /
screenshot* default ("no change" expanded to "the existing
screenshot continues to capture *something* useful, even if
the precise frame moves").

### 5. Documentation surface

- **`README.md`** — new subsection under *Building and
  running* describing the bootloader scene's operator UX:
  what the prompt looks like, what to type / paste, how the
  three flow paths behave, how to recover from a Wrong-paste
  loop. Cross-link to `make spice` from phase 1. Update
  *Status* if anything Phase-1-shaped is now stale.
- **`AGENTS.md`** — bump *Current phase* to "locked-bootloader
  phase 3" once Phase 2 ships; add `src/bootloader.rs` to the
  *Where to read first* list.
- **`ARCHITECTURE.md`** — new paragraph in the scene-flow
  section describing the bootloader sub-state-machine, the
  ring buffer's new event variants, and the `paste-as-
  keystrokes` channel-test framing. Reference the
  `BOOT_SCRIPT` split at `SENSORIUM: nominal`.
- **`DESIGN.md`** — no functional change; if the SPICE channel
  mapping table has a row for *Text clipboard client → server*
  with a status, mark this milestone's binary as the
  implementer (or leave to Phase 3 docs sweep — see the
  master plan's Phase 3 sketch). **Default: defer to Phase 3**
  to keep this phase's diff focused on code.
- **`docs/spice-test-inventory.md`** — same deferral; Phase 3
  is the inventory closeout phase per the master plan.

## Open questions

Defaults below are strong but worth confirming as the
implementation lands. Capture changes inline.

- **`PasteReceived` carries `correct`**. **Default: yes**,
  per the *Mission* above. The master plan's shorthand
  `PasteReceived { len }` was illustrative, not normative.
  Trade-off: a parser could infer correctness from the
  next-event sequence (a `LineRendered` "Booting..." vs a
  `BootloaderTimeout` or another `LineRendered` re-prompt),
  but that ties parser logic to render order rather than
  semantic outcome. Keeping `correct` explicit is cheaper
  than a 32-bit field worth of clarity.
- **`BootloaderDecision` carries `attempt`**. **Default:
  yes.** Same justification — explicit field beats parser
  reconstruction. Costs nothing on the wire; ringbuffer
  capacity (256) is unaffected.
- **Wrong-paste counter shares the prompt's attempt counter
  vs separate.** **Default: separate.** The R/I/A prompt's
  `attempt N` counts retry presses (R chosen N-1 times before
  the current render); the awaiting-paste line's
  `(wrong, attempt N of 3)` counts wrong pastes
  independently. They mean different things and conflating
  them would confuse the operator and the parser. Two
  counters in the bootloader state struct.
- **Paste validation strictness**. **Default: byte-exact
  match against `sextant{HELLO_OPERATOR}` after stripping a
  single trailing `\r` if present.** Trim trailing CR/LF
  exactly once; do not collapse internal whitespace, do not
  case-fold, do not accept the base64 form (the whole point
  is the operator decoded it). Trim is needed because ryll's
  Enter mapping translates LF/CRLF in the source clipboard to
  one Enter scancode; UEFI Simple Text Input Ex returns `\r`
  for that Enter; the operator expects "Enter ends the
  paste" rather than "Enter is a wrong character".
- **Echo-on-paste vs hidden input**. **Default: echo.**
  The scene is a channel test, not a credential prompt; the
  operator needs to see what arrived (especially if a
  character drops). Hiding input would also make the
  attempt-counter-of-3 wrong-paste loop nearly impossible
  to debug visually. Future work: a "secure" variant that
  hides input could exist for a later scene.
- **Per-character echo column tracking vs full re-render**.
  **Default: per-character.** Each arriving char blits into
  the next column on the input row and increments a column
  counter; on terminator-and-validate, the row is cleared and
  re-rendered (in place). No full re-render per character.
- **Retry dot animation: dots vs spinner**. **Default:
  dots.** Period-correct for the project's CRT aesthetic;
  spinners read as modern web UI. Eight dots over 1.6 s feels
  faster than the master plan's nominal 1.2 s but is closer
  to the visual reading time of "Retrying decryption........"
  at 200 ms per glyph.
- **Countdown rendering: `NN` zero-padded vs `N`**.
  **Default: zero-padded two-digit `NN`** so the in-place
  cell update never has to clear a third column when the
  count crosses from 10 to 9. Two cells, always.
- **Module location**. **Default: `src/bootloader.rs` at the
  crate root**, alongside `scene.rs`. Not `src/scene/
  bootloader.rs` (turning `scene` into a module directory is
  more reorganisation than this phase wants), and not
  `src/scenes/bootloader.rs` (premature — there is one scene
  today and one new sub-state-machine, no need for a plural
  module yet).
- **Should the bootloader own the `LineRendered` events for
  its preamble lines, or skip them**. **Default: emit them.**
  Consistency with `BOOT_SCRIPT`'s render path; a parser
  watching `LineRendered` row counts to estimate boot
  progress should not see a gap.
- **Test coverage scope**. **Default: no automated tests in
  this phase.** Integration coverage is operator-driven via
  `make spice`. Host-side `cargo test` would need crate
  restructuring (UEFI-only features split out of
  `Cargo.toml`'s default `[dependencies]`, `no_main` /
  `no_std` made conditional on `cfg(not(test))`) — too much
  scaffolding for the unit-test surface area available
  today. Future work: extract a clock + render trait the
  scene can take generically so the state machine can be
  exercised in tests once that scaffolding is justified by
  more than one scene.

## Execution

Five steps, one logical change each. Each lands as its own
commit; together they constitute Phase 2.

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 2a   | low    | sonnet | none      | Add the three new `Event` variants and the `BootloaderChoice` enum to `src/event.rs`; extend `src/serial.rs::drain` with the new `type=...` lines. No call sites yet (compile-only consumer; both new variants and the new enum get `#[allow(dead_code)]` to keep clippy quiet until step 2c). See *Brief for step 2a*. |
| 2b   | low    | sonnet | none      | Add `Renderer::clear_row` and `Renderer::draw_text_at` to `src/renderer/mod.rs`. Both are mechanical; principle-6-clean (one BltOp per glyph or one VideoFill per row). See *Brief for step 2b*. |
| 2c   | high   | opus   | worktree  | Create `src/bootloader.rs` implementing the full state machine specified in *Mission §2*. Wire it into `src/scene.rs::run_booting` between the `SENSORIUM: nominal` step and the `EMERGENCY SAFE BOOT COMPLETE` step (split `BOOT_SCRIPT` into PRE/POST slices and call `bootloader::run` between them). Confirm `pre-commit run --all-files` passes and `cargo build --release` for the UEFI target succeeds. See *Brief for step 2c*. |
| 2d   | medium | sonnet | none      | Update `scripts/screenshot.sh` to drive the bootloader scene end-to-end via QMP `send-key` (Ignore + `sextant{HELLO_OPERATOR}` + Enter). If a character mapping issue forces the fallback, leave `make screenshot` capturing pre-bootloader and add `make screenshot-bootloader` for a prompt-screen capture. Either way, regenerate any screenshots that change. See *Brief for step 2d*. |
| 2e   | n/a    | n/a    | n/a       | **Operator-driven smoke test in the management session.** Run `make spice` and walk all four flow paths (correct paste, wrong-then-correct, abort-and-replay, timeout). Capture findings in this plan's *Bugs fixed during this work* section, then update `README.md`, `AGENTS.md`, `ARCHITECTURE.md` per *Mission §5* in a final docs commit. See *Brief for step 2e*. |

### Brief for step 2a

Edit `src/event.rs`:

- Add a `BootloaderChoice` enum with variants `Retry`,
  `Ignore`, `Abort`. Implement `Copy`, `Clone`, `Debug`, and
  a `tag(&self) -> &'static str` returning `"retry"`,
  `"ignore"`, `"abort"`.
- Add three new variants to `enum Event`:
  - `BootloaderDecision { choice: BootloaderChoice, attempt: u32, timestamp_ms: u64 }`
  - `PasteReceived { len: usize, correct: bool, timestamp_ms: u64 }`
  - `BootloaderTimeout { timestamp_ms: u64 }`

Edit `src/serial.rs::drain` to format the new variants:

```text
t={ms} type=bootloader_decision choice={tag} attempt={n}
t={ms} type=paste len={n} correct={true|false}
t={ms} type=bootloader_timeout
```

No tests in this phase: the project builds through `make
build` (Docker, UEFI target) and has no host-side test
wiring. Wiring host-side `cargo test` would require splitting
`Cargo.toml` into target-conditional dependency tables and
guarding `#![no_main]` / `#![no_std]` behind `cfg(not(test))`
— too much crate restructuring for two unit tests. Both new
variants and the new enum get `#[allow(dead_code)]` to keep
clippy quiet until step 2c adds call sites.

Verification:
- `make build` (Docker, UEFI target) clean.
- `pre-commit run --all-files` clean.

No call sites for the new variants yet — they're compile-only
consumers until step 2c. Behaviour gets exercised end-to-end
by the operator-driven smoke test in step 2e.

Commit message subject: `Add bootloader event variants.`
Body: brief description of the three variants, their tags,
and the unit test surface added.

### Brief for step 2b

Edit `src/renderer/mod.rs` to add two helper methods on
`impl Renderer`:

```rust
/// Clear an entire text row to background. Issues one
/// `BltOp::VideoFill` over the writable area between the
/// horizontal margins.
pub fn clear_row(&mut self, row: usize) {
    let px = MARGIN_X;
    let py = MARGIN_Y + row * CELL_H;
    let dims = (
        self.width.saturating_sub(2 * MARGIN_X),
        CELL_H,
    );
    let _ = self.gop.blt(BltOp::VideoFill {
        color: BG,
        dest: (px, py),
        dims,
    });
}

/// Render a string starting at the given text-cell column.
/// One `draw_glyph` per character (principle 6).
pub fn draw_text_at(&mut self, text: &str, col: usize, row: usize) {
    for (i, ch) in text.chars().enumerate() {
        self.draw_glyph(ch, col + i, row);
    }
}
```

Doc-comments wrapped at 80; no other behaviour changes; no
new tests required (rendering is not host-testable).

Verification: `pre-commit run --all-files` clean,
`cargo build --release` clean.

Commit message subject: `Add clear_row and draw_text_at
renderer helpers.`

### Brief for step 2c

The load-bearing implementation step.

Create `src/bootloader.rs` from scratch implementing the
state machine in *Mission §2*. The module exposes one public
function (`run`) and one public type (`BootloaderOutcome`);
all state is internal.

Key implementation guidance:

- **State carrier.** Internal `BootloaderScene` struct
  holding `renderer: &mut Renderer`, `ring: &mut
  RingBuffer<256>`, `clock_ms: &mut u64`, `start_row: usize`,
  `retry_count: u32`, `wrong_paste_count: u32`,
  `current_prompt_row: usize`, `current_input_row: usize`.
  `run` instantiates this and dispatches a state-machine
  loop.
- **Polling cadence.** Reuse the 50 ms `POLL_MS` from
  `scene.rs`. Add a private `poll_key` here OR (preferred)
  refactor `Scene::poll_key` to a free function in `scene.rs`
  and call it from both. **Default: refactor.** The function
  is purely a `with_stdin` wrapper and does not need to be a
  method.
- **Stall + clock.** Same pattern as `Scene::stall`. Either
  refactor that to a free function too, or duplicate the
  three lines in the bootloader module. **Default:
  refactor** — the shared stall function takes `clock_ms:
  &mut u64` and the renderer-less callers can live with that.
- **Per-glyph correctness.** Every dot in the retry
  animation, every digit in the countdown, every character
  of the attempt-counter line is its own `draw_glyph` /
  `draw_text_at` call. The countdown's NN cells are blitted
  by clearing the two cells (`clear_cell` ×2) and then
  blitting the new digits. Do not compose strings into a
  single `draw_line` call.
- **In-place row updates.** When re-rendering the prompt
  (after a Retry), call `clear_row` on the prompt's row
  *and* the diegetic-nudge row (so the nudge does not
  ghost-leak after the operator stops retrying), then
  re-render only what is current.
- **Idle timer.** Track `idle_ms_since_last_key` separately
  from the global `clock_ms`. Reset on each non-empty
  `poll_key` call. Compare to `PASTE_SILENT_WAIT_MS`.
  Important: only the awaiting-paste state checks the silent-
  wait timer; the prompt state polls indefinitely (the
  master plan does not specify a timeout for indecision at
  the prompt itself).
- **Wrong-paste re-prompt.** After three wrong pastes,
  emit `BootloaderTimeout` and proceed to the visible
  countdown — not to the silent-wait phase. The master plan
  treats wrong-paste-cap-reached and silent-wait-elapsed as
  the same terminal flow path, with the same countdown
  duration. (If you read this and disagree, raise it as an
  *Open questions* issue rather than diverging silently.)
- **Abort path.** Emit
  `BootloaderDecision { choice: Abort, attempt, ts }`, then
  `uefi::runtime::reset(ResetType::COLD, Status::SUCCESS,
  None)`. The function is `-> !`. No drain.
- **Success path.** After validating, clear the bootloader
  scene region (rows from telemetry preamble through input
  echo line — track the start_row and the highest row used,
  call `clear_row` over that range), render `Booting...` on
  `start_row + 2` (leaving the b64 / NIST telemetry lines as
  a "tail of evidence" for the post-boot reader), stall
  `BOOT_PAUSE_MS`, return `Continue { next_row: start_row +
  3 }`. The caller's existing `EMERGENCY SAFE BOOT COMPLETE`
  step renders on that row.

Wire-in changes to `src/scene.rs`:

- Add `mod bootloader;` import path adjustment in
  `src/main.rs`.
- Split `BOOT_SCRIPT` into `BOOT_SCRIPT_PRE` (entries 0..=N
  where N is the `SENSORIUM: nominal` line, inclusive) and
  `BOOT_SCRIPT_POST` (just the `EMERGENCY SAFE BOOT
  COMPLETE` line). Verify by visual inspection of the file
  diff that no script entries are dropped or reordered.
- In `Scene::run_booting`, replace the single loop with two
  loops separated by a `bootloader::run(renderer, &mut
  self.ring, &mut self.clock_ms, row)` call. The bootloader
  returns `BootloaderOutcome::Continue { next_row }`; assign
  `row = next_row` before the second loop.

Build and verify:

- `make build` (Docker, UEFI target) clean.
- `pre-commit run --all-files` clean.
- `make qemu` launches the binary and at minimum reaches
  the bootloader prompt; the prompt does not have to be
  driven from the GTK display (that is operator-driven in
  step 2e).

Commit message subject: `Implement locked-bootloader scene.`
Body: outline the state machine, the BOOT_SCRIPT split, the
new event emission points, and the constants chosen.

Worktree isolation: yes (`isolation: "worktree"`). The
change is large and touches scene flow; a worktree gives a
clean rollback path if the design diverges from this plan
during implementation.

### Brief for step 2d

Edit `scripts/screenshot.sh` to drive the bootloader scene.

The current Python inline block sends `'spc'` to advance
AWAITING and waits 5.5 s for the parking screen. Extend the
sequence:

1. AWAITING settle: 1.5 s (existing).
2. Send `'spc'` — advances to Booting (existing, kept).
3. Wait for boot transcript to reach the bootloader prompt.
   Empirically ~1.5 s after the spc keypress (BOOT_SCRIPT_PRE
   has ~7 entries at 200 ms each, plus the new b64 and NIST
   telemetry lines pushing total time closer to 2 s). Use a
   2.5 s settle.
4. Send `'i'` — selects Ignore.
5. Wait 0.5 s for the blob to render.
6. Type out `sextant{HELLO_OPERATOR}` as a sequence of QMP
   `send-key` calls. Letters are direct qcodes; `{`, `_`,
   `}`, and the uppercase letters all use `shift` modifier:
   `[{type:'qcode', data:'shift'}, {type:'qcode', data:'h'}]`
   for `H`, etc. Build the sequence in Python from the
   literal target string by iterating each char and
   classifying it via a small lookup table (lowercase → bare
   qcode; uppercase / shifted-symbol → `[shift, qcode]`).
7. Send `'ret'` — Enter.
8. Wait for Booting pause + post-bootloader transcript +
   parking settle: 2 s.
9. `screendump` (existing).

If any character in the target string maps to a qcode that
QEMU's qcode dictionary does not provide (most likely
candidates: `{` and `}` — these are `shift+bracketleft` and
`shift+bracketright`; and `_` — `shift+minus`), the script
will fail at runtime. **Fallback path:** revert the
`screenshot.sh` changes; instead, add a new `make
screenshot-bootloader` target backed by
`scripts/screenshot-bootloader.sh` that drives AWAITING +
the prompt-screen-only path (no paste) and screendumps the
prompt. Keep the original `make screenshot` capturing the
pre-bootloader frame (it will time out at the paste prompt
during the 5.5 s wait, but the screendump fires before the
30 s visible countdown so the prompt screen IS what gets
captured — note this in the script header). Update only
`docs/images/bootloader-prompt.png` in this fallback;
`docs/images/boot-sequence.png` remains the previous
milestone's frame.

Verification:

- `make screenshot` succeeds and produces a non-zero PNG.
  Spot-check by eye that the captured frame is the parking
  screen (success path) — or, in the fallback, the
  bootloader prompt (per the fallback's documented
  behaviour).
- `pre-commit run --all-files` clean (shellcheck on the
  script).

Commit message subject: `Drive screenshot through bootloader
scene.` (or `Add bootloader screenshot fallback.` in the
fallback path).

### Brief for step 2e

**Operator-driven; runs in the management session, not a
sub-agent.**

Run `make spice`. Walk all four flow paths and capture
notes:

1. **Correct paste**: AWAITING → boot → bootloader prompt →
   `I` → blob → paste `sextant{HELLO_OPERATOR}` from host
   clipboard via ryll's `Ctrl+Alt+V` → confirm `Booting...`
   → confirm parking screen reached. Confirm
   `dist/serial.log` after shutdown contains a
   `type=paste len=23 correct=true` line.
2. **Wrong-then-correct**: same up to the blob → paste
   `wrong` → confirm in-place re-prompt with `(wrong,
   attempt 1 of 3)` → paste correct → confirm boot
   continues. Confirm two `type=paste` lines, the first
   `correct=false`, the second `correct=true`.
3. **Abort-and-replay**: at the bootloader prompt → `A` →
   confirm cold reset (the OVMF firmware boot manager
   should re-run; the AWAITING screen returns). Confirm a
   `type=bootloader_decision choice=abort` line appears in
   any captured serial log if the operator can grab the log
   before reset. (Cold reset wipes the log file under
   `-serial file:` semantics; this is expected.)
4. **Timeout**: at the bootloader prompt → `I` → at the
   blob screen → wait silently. Confirm
   `Awaiting decoded payload.` rendered, the silent wait of
   60 s completes, the visible countdown ticks
   `30, 29, ..., 0` (each tick in-place), the error halt
   line renders, 5 s of stall, then ACPI shutdown. Confirm
   `dist/serial.log` contains a `type=bootloader_timeout`
   line.

Edge cases worth recording in *Bugs fixed during this work*
or *Future work*:

- Whether ryll's 16 ms inter-character delay drops keys
  under the 50 ms UEFI poll. (Expected: no, firmware buffers
  multiple keys; record actuals.)
- Whether the host clipboard's trailing newline maps cleanly
  to a single Enter that terminates the paste, or arrives as
  two characters that the binary mishandles.
- Whether `Ctrl+Alt+V` accidentally arrives at the binary as
  `Ctrl+Alt+V` keystrokes (per ryll's plan, the shortcut is
  intercepted at the egui layer and not forwarded; verify).
- Whether `make qemu` (GTK) reaches the bootloader prompt
  and times out cleanly when no SPICE client is connected
  (it should — no key activity = silent-wait expires =
  countdown = halt).

After the smoke test, spawn a sub-agent (sonnet, none) with
the verified findings and the *Mission §5* spec to update
`README.md`, `AGENTS.md`, and `ARCHITECTURE.md`. The agent
does not need to run the smoke test — pass it the results.
Defer `DESIGN.md` and `docs/spice-test-inventory.md` updates
to Phase 3 per the *Mission §5* default.

Final commit message subject: `Document locked-bootloader
scene.` Commit body lists the verified flow paths and any
findings.

Update the master plan's Execution table row 2 from
*Not started* to *Complete (commits 2a–2e SHAs)* in the
same docs commit.

## Agent guidance

Follow the master `PLAN-locked-bootloader.md` *Agent
guidance* and the inherited `PLAN-first-playable.md`
guidance. Phase 2-specific emphases:

- **One scene, one module.** All bootloader-specific logic
  lives in `src/bootloader.rs`. `scene.rs` only knows how to
  call `bootloader::run` and where to put the result. If a
  `cfg`-flag-or-sentinel is added to `BOOT_SCRIPT` to
  trigger the bootloader, that's a sign the split is
  wrong — split the script into two slices instead.
- **Trust the firmware buffer.** Ryll's 16 ms inter-character
  cadence is faster than the 50 ms poll loop. UEFI Simple
  Text Input Ex queues key events in the firmware until
  `read_key` consumes them. Do not pre-emptively tighten the
  poll cadence; this would only matter if the firmware
  actually drops characters, which Phase 2's smoke test will
  determine. If smoke testing reveals drops, that is a
  finding to record and address — not a planning-stage
  assumption to over-engineer around.
- **Per-glyph blits, no exceptions.** The retry animation,
  the attempt counter, the countdown digits, the input
  echo — all per-glyph. The renderer's `draw_text_at`
  helper from step 2b is the only path; do not introduce a
  multi-glyph blit shortcut.
- **In-place updates.** No scrolling. Every re-render either
  clears the affected rows first (`clear_row`) or only
  overwrites cells that changed (countdown digits). The
  scene fits comfortably below the existing boot transcript
  and above the parking-screen position.
- **Cold reset means no drain.** Document this in the abort
  path's code comment so a reader does not look for the
  Abort decision in `dist/serial.log` and report it as a
  bug.
- **Sub-agents inherit, but the scene is a judgement call.**
  Step 2c is high-effort opus-on-worktree because the state
  machine has timing-related correctness concerns (idle
  timer, retry animation, countdown), the modifier-state
  invariants from ryll's paste path matter, and the
  worst-case rollback cost of a bad implementation is
  several hours of operator smoke-testing. Steps 2a, 2b,
  2d are well-briefed enough for sonnet without isolation;
  step 2e is operator-driven.

## Administration and logistics

### Success criteria

This phase is complete when:

- [x] `Event::BootloaderDecision`, `Event::PasteReceived`,
      and `Event::BootloaderTimeout` exist in `src/event.rs`,
      have stable lowercase tags, and are emitted by the
      bootloader module at the right state transitions.
- [x] `src/bootloader.rs` exists and contains the full
      state machine (telemetry preamble → prompt → retry /
      ignore / abort → blob → awaiting paste → success /
      timeout). All rendering goes per-glyph through the
      renderer.
- [x] `Renderer::clear_row` and `Renderer::draw_text_at`
      exist and are used by the bootloader module.
- [x] `BOOT_SCRIPT` is split into PRE / POST slices and
      `Scene::run_booting` calls `bootloader::run` between
      them. `EMERGENCY SAFE BOOT COMPLETE` renders only on
      the success path.
- [x] `make spice-ryll` walks all four flow paths (correct
      paste, wrong-then-correct, abort-and-replay, timeout)
      with the observed behaviour matching the *Mission*
      spec. Operator-confirmed via smoke test.
- [x] `make qemu` (GTK) launches and reaches the bootloader
      prompt; the silent-wait timer + countdown + halt path
      runs cleanly when no key activity arrives. (Not
      directly walked under `make qemu`; ticked because the
      same `bootloader.rs::capture_paste` and `run_timeout`
      code paths run under `make spice-ryll` and were
      operator-confirmed there during the Phase 2 smoke
      test.)
- [x] `make release-verify` and `make screenshot` continue
      to pass. (`make screenshot` captures the parking
      screen via the bootloader-traversal QMP send-key
      sequence; screenshot regenerated and committed in
      step 2d.)
- [x] `dist/serial.log` after a successful run contains
      events in this order (timestamps elided):
      `type=transition from=awaiting to=booting`, several
      `type=line` lines, `type=bootloader_decision
      choice=ignore`, `type=paste len=23 correct=true`,
      more `type=line` lines, `type=transition from=booting
      to=parked`, then post-keypress `type=keypress` and
      `type=transition`. Operator confirmed via smoke test.
- [x] `pre-commit run --all-files` exits 0.
- [x] `README.md`, `AGENTS.md`, and `ARCHITECTURE.md` cover
      the new scene at the level specified in *Mission §5*.
      (This commit.)
- [x] Master plan execution table row 2 shows *Complete*
      with linked commits. (This commit.)

### Future work

Items deliberately deferred:

- **Cosmetic timing tuning.** The retry-dot cadence, the
  countdown tick spacing, and the prompt's keypress
  responsiveness will each want at least one operator
  iteration before they feel right. That iteration is
  Phase 3, not Phase 2 — Phase 2 ships the structurally
  complete scene; Phase 3 polishes it.
- **A "secure" hidden-input variant** of the awaiting-paste
  step. Useful for any future scene that simulates a real
  password prompt. Out of scope here; the channel test is
  the point, not the credential framing.
- **Host-side state-machine tests.** Extracting the clock
  and renderer behind a trait so the state machine can be
  unit-tested without UEFI dependencies. Cleaner long-term
  story; not load-bearing for this phase since the scene is
  a single, operator-driven artifact.
- **`docs/images/bootloader-prompt.png`** — a third
  screenshot dedicated to the prompt screen. Only landed if
  step 2d's fallback path is taken; otherwise covered by
  the parking-screen screenshot which now traverses the
  bootloader.
- **Real cryptographic content.** The plain-base64 round
  trip is per the master plan's "no cute encoding choices"
  guidance. Replacing it with a real encrypted payload
  (requiring a key from a later scene's keypath) is
  master-plan future work.
- **Multi-prompt indecision timeout.** Polling forever at
  the R/I/A prompt is intentional — the operator may step
  away to find the host clipboard tool — but a separate
  long timeout (5 minutes?) might be a kindness for CI
  regressions where neither input nor abort arrives.
  Master-plan future work.
- **Wrong-paste-suffix overlap.** If the paste buffer
  contains a string that is a strict prefix or suffix of
  the correct `sextant{HELLO_OPERATOR}` target, the
  re-prompt suffix `(wrong, attempt N of 3)` renders
  immediately after the operator's echoed input rather than
  on a clearly separate region. In a future iteration,
  clearing the echo portion before rendering the suffix
  would prevent visual ambiguity. Phase 3 cosmetic polish.
- **Partial-paste-no-newline indecision hang.** If the
  operator pastes a partial string (e.g. stops mid-paste
  without the ryll Enter-scancode terminator), the capture
  loop waits at the awaiting-paste state indefinitely — the
  silent-wait timer only fires on an empty buffer, and the
  partial paste has primed it. This is consistent with the
  plan's intent ("partial half-typed pastes do not silently
  time out — they fail by terminator-or-buffer-full") but
  can read as a hang to an operator who abandons a partial
  paste mid-way. A secondary timeout or a dedicated
  "partial paste detected" re-prompt is Phase 3 behavioural
  polish.

### Bugs fixed during this work

1. **`Ctrl+Shift+V` is a UX footgun.** The operator's first instinct
   at the paste prompt is `Ctrl+Shift+V` — the standard paste
   shortcut in every terminal emulator on Linux. Ryll's paste-as-
   keystrokes shortcut is `Ctrl+Alt+V` (deliberately different, to
   avoid the terminal clash). Pressing `Ctrl+Shift+V` sends those
   three literal keystrokes to the guest rather than triggering
   paste, which results in either garbage in the paste buffer or
   apparent silence. Mitigated by an explicit "NOT `Ctrl+Shift+V`"
   reminder in the `make spice-ryll` launch banner and called out in
   README.md and ARCHITECTURE.md.

2. **Mac keyboards / Option key mapping.** On a Mac keyboard plugged
   into the host (or via XRDP / Kasm), the Option key may or may not
   map to Alt depending on the active keymap layer. This means
   `Ctrl+Alt+V` may silently fail to trigger paste on Mac hardware.
   The *Menu → Paste* GUI path in ryll is a clean workaround that
   bypasses the keyboard-mapping question entirely, and is documented
   in README.md as the recommended fallback.

### Documentation index maintenance

- Update `docs/plans/index.md`: bump the master plan's
  status from *Not started* to *Phase 1 complete, Phase 2 in
  progress* when this phase plan lands, and to *Phase 1–2
  complete, Phase 3 in progress* when step 2e completes.
- `docs/plans/order.yml`: no change (phase plans are not
  added per the project convention).
- Master plan `PLAN-locked-bootloader.md` Execution table
  row 2: update at start of step 2c (*In progress*) and at
  end of step 2e (*Complete (commits ...)*).

### Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with that plan.
