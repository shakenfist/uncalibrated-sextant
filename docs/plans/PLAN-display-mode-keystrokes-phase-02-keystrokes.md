# Display-mode keystrokes — phase 2: dispatcher, cycle, toast

Parent plan:
[PLAN-display-mode-keystrokes.md](PLAN-display-mode-keystrokes.md).
Previous phase:
[PLAN-display-mode-keystrokes-phase-01-renderer.md](PLAN-display-mode-keystrokes-phase-01-renderer.md).

## Outcome

**Status: Not started.**

This section will be populated as Phase 2 lands, in the same
shape as Phase 1's *Outcome* section: a one-paragraph headline,
the list of what shipped, and the list of what was *not*
shipped and why.

## Prompt

Before working on this phase, re-read the master plan's
*Phase 2 sketch* and the *Open questions* entries it points to
(especially *cycle interruptibility*, *cycle dwell time*,
*Mode switching during `run_booting`*, *Mode switching during
the locked-bootloader scene*, *On-screen toast vs serial-only
feedback*, and *Toast row collision with scene content*). Skim:

- [`src/scene.rs`](../../src/scene.rs) — your primary working
  surface. Every change in this phase touches this file. Pay
  attention to the `Scene` struct (added `repaint_state` in
  Phase 1), `play_script` (currently calls `stall(POLL_MS)`
  per line; will need polling-aware variant), the runners
  (`run_awaiting`, `run_booting`, `run_parked`), and the
  free `poll_key` helper.
- [`src/bootloader.rs`](../../src/bootloader.rs) — read-only
  for this phase. The bootloader has its own `poll_key`
  sites at lines 277 and 519 and its own `Keypress` event
  pushes. The carve-out is mechanical: do not invoke the
  dispatcher from bootloader.rs. Mode keys arriving during
  the R/I/A or paste-prompt loops continue to be logged as
  Keypress events and otherwise ignored, exactly as today.
- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) — Phase 1
  added `set_mode`, `available_modes`, and `nearest_mode`.
  This phase adds one more accessor (`screen_rows()`) for
  toast positioning. No other renderer changes.
- [`src/event.rs`](../../src/event.rs) — Phase 1 added
  `ModeSwitch` and `ModeCycle` with `#[allow(dead_code)]`.
  This phase removes those attributes as the dispatcher
  becomes the emitter.
- The Phase 1 *Outcome* section, especially the deferred
  items: the qxl mode-list capture and host-side `cargo test`
  for `nearest_mode`. Phase 2's smoke-test step is where
  the qxl path gets exercised against ryll.

Where the UEFI spec or `uefi-rs` semantics matter, read first:

- The Phase 1 plan's notes on `set_mode` framebuffer
  invalidation. Every dispatcher invocation calls `set_mode`
  + `Scene::repaint`, in that order, to honour the contract.
- `uefi::system::with_stdin` for `read_key`. Existing
  `poll_key` already wraps it; reuse rather than re-derive.

This phase plans at **medium effort** overall, with one
**high-effort opus** step for the cycle-mode interruption
design (step 2c). The master plan flagged the `run_booting`
redraw policy as the high-effort candidate; further analysis
makes the cycle-interruption logic the more delicate piece —
the `run_booting` policy is satisfied by the
already-implemented `Scene::repaint`'s static replay.

## Goal

Land the keystroke-driven user-facing affordance: keys `1`–`6`
switch the GOP framebuffer to a chosen resolution from the
master plan's table, key `0` walks every available mode with a
1.0 s dwell per step, and an on-screen toast names the
*applied* resolution (with a `requested → using` form when the
firmware substitutes). Mode keys work in `run_awaiting`, mid-
`run_booting` (with the boot script paused but resumed), and
`run_parked`. The locked-bootloader scene's input loop is
untouched; mode keys arriving there continue to be
logged-and-ignored as today.

By the end of this phase:

- An operator running `make qemu` can press `1`–`6` and `0`
  from the awaiting / booting / parked screens and see the
  GTK window resize, the scene repaint cleanly, and a toast
  name the applied mode for ~1.5 s.
- An operator running `make spice-ryll` against ryll's
  `display-mode-ui` branch can do the same and see ryll's
  window track each new resolution. (Acceptance test deferred
  to step 2e; ryll-driven verification of both Obey-on and
  Obey-off branches lives there.)
- Ring-buffer events `ModeSwitch` and `ModeCycle` carry the
  applied (queried-back-from-GOP) dimensions and are visible
  in `dist/screenshot-serial.log` after a cycle.
- The locked-bootloader scene continues to behave exactly as
  it does today.

## Scope

**In scope:**

- `Scene::try_handle_mode_key(&mut self, &mut Renderer, ch:
  char) -> bool` — the dispatcher. Returns true iff the key
  was a mode key.
- Static binding table: `'1'`→640×480, `'2'`→800×600,
  `'3'`→1024×768, `'4'`→1280×720, `'5'`→1280×1024,
  `'6'`→1920×1080, `'0'`→cycle. Match on unicode `char`
  per the master plan; do not key off scancodes.
- `ToastState` + `Scene::draw_toast` + `Scene::tick_toast`
  for ~1.5 s ephemeral on-screen feedback at the bottom row.
- `Scene::cycle_modes(&mut self, &mut Renderer)` — walks
  `Renderer::available_modes()`, dwell `CYCLE_DWELL_MS = 1000`
  per step, interruptible by any keypress.
- Polling-aware stall in `play_script` so mid-boot mode keys
  are honoured. Boot pacing is *paused* during the dispatch
  + repaint + toast and resumes from where it was; total
  pacing for the line absorbs the dispatch wall time without
  artificially extending it.
- Hooks into `run_awaiting` and `run_parked` blink loops.
- `Renderer::screen_rows()` accessor (parallels the existing
  `screen_rows`-equivalent computation; needed for toast
  positioning).
- Removal of `#[allow(dead_code)]` from `Event::ModeSwitch`,
  `Event::ModeCycle`, `Scene::repaint`, `Scene::repaint_script_prefix`,
  and the `RepaintState` enum, all of which gain real
  callers in this phase.
- Smoke test: `make screenshot` continues to exercise the
  no-keystroke path identically to Phase 1 (no regression in
  `dist/screenshot-serial.log`); `make qemu` exercises the
  keystroke path end-to-end interactively;
  `make spice-ryll` against ryll's `display-mode-ui` branch
  is the acceptance test for both Obey-on and Obey-off
  branches of the ryll feature.

**Out of scope (Phase 3):**

- Documentation: `README.md`, `ARCHITECTURE.md`, `AGENTS.md`,
  `docs/spice-test-inventory.md`. Phase 2's commit messages
  carry the why; Phase 3 lifts that into the operator-
  visible docs.
- `make screenshot-bootloader` or any sequence-screenshot
  harness for capturing one frame per mode. *Future work*
  per the master plan.

**Out of scope (this phase, deferred to Future work):**

- Ryll-driven mode requests (gRPC-over-serial). This phase
  has the binary's side; ryll-driven assertions wait on the
  transport landing in another milestone.
- Bit-depth / multi-head simulation / scene-driven mode
  changes. All inventory rows distinct from the *Mode walk*
  row this phase claims.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 2a   | medium | sonnet | none      | `Renderer::screen_rows()` + `Scene::try_handle_mode_key` core dispatcher (keys `'1'`–`'6'` only). Pushes `ModeSwitch` event with applied dims; calls `set_mode` then `repaint`. Removes `#[allow(dead_code)]` from `Event::ModeSwitch`, `Scene::repaint`, `Scene::repaint_script_prefix`, and `RepaintState`. No callers yet. |
| 2b   | medium | sonnet | none      | On-screen toast: `ToastState` + `draw_toast` + `tick_toast` + integration into `try_handle_mode_key`. Toast text format (`mode WxH` for exact match, `requested WxH -> using AxB` for substitution; ASCII only because the renderer's font is ASCII). Position: bottom row of current screen. TTL constant `TOAST_MS = 1500`. |
| 2c   | high   | opus   | worktree  | Cycle mode (key `'0'`): `Scene::cycle_modes` walks `available_modes()` with `CYCLE_DWELL_MS = 1000` per step, interruptible by any keypress; the interrupting key is consumed by the dispatcher recursively if it is a mode key, otherwise discarded. Push `ModeCycle` event. Removes `#[allow(dead_code)]` from `Event::ModeCycle`. Worktree-isolated because the interrupt-handling design has subtle correctness questions and the first attempt may need revising. |
| 2d   | medium | sonnet | none      | Wire dispatcher + toast tick into all three runner sites: `run_awaiting`, `play_script` (via a new polling-aware stall), `run_parked`. Bootloader untouched. Document the small behaviour change for non-mode keys received during `play_script` (today they sit in the firmware buffer and arrive at `run_parked`; after this step they are read and discarded). |
| 2e   | medium | sonnet | none      | Smoke-test exit: `make screenshot` continues to produce the same 59-event transcript (no keystrokes injected); `make qemu` exercises mode keys interactively; `make spice-ryll` against ryll's `display-mode-ui` branch is the acceptance test for the four bullets in the master plan's *Mission*. Tick exit criteria. Step 2e is mostly about driving the operator-side smoke test — keep the diff tiny. |

Commits expected: one per step (2a–2d). 2e produces a closeout
commit that ticks exit criteria + may tune toast text /
timing if the operator iteration finds something worth
fixing.

## Detailed step briefs

### 2a — dispatcher core (keys `'1'`–`'6'`)

**Files:** `src/renderer/mod.rs`, `src/scene.rs`, `src/event.rs`.

**What to add to `src/renderer/mod.rs`:**

```rust
/// Number of whole 8x16 text cells that fit between the top
/// and bottom overscan margins on the current screen.
pub fn screen_rows(&self) -> usize {
    (self.height.saturating_sub(2 * MARGIN_Y)) / CELL_H
}
```

Parallel to the existing `screen_cols`. Used in 2b for
toast row selection. Adding it in 2a keeps the renderer
diff for 2a small and self-contained.

**What to add to `src/scene.rs`:**

```rust
/// Mapping from keystroke to (width, height) request.
/// Match the master plan's table verbatim.
const MODE_KEYS: &[(char, u32, u32)] = &[
    ('1', 640, 480),
    ('2', 800, 600),
    ('3', 1024, 768),
    ('4', 1280, 720),
    ('5', 1280, 1024),
    ('6', 1920, 1080),
];
```

```rust
impl Scene {
    /// Try to handle a keystroke as a mode-switch request.
    /// Returns `true` if the keystroke was a mode key and was
    /// consumed; `false` if the caller should handle it as
    /// usual.
    ///
    /// Mode-switch path: look up `ch` in `MODE_KEYS`, call
    /// `Renderer::set_mode` with the requested dimensions,
    /// push a `ModeSwitch` event with both the request and
    /// the queried-back applied resolution, and call
    /// `Scene::repaint` so the framebuffer (invalidated by
    /// `set_mode` per UEFI 2.10 §12.9) shows the current
    /// scene state at the new dimensions.
    ///
    /// `'0'` is the cycle key and is handled in step 2c;
    /// for now this dispatcher returns `false` for `'0'`.
    pub(crate) fn try_handle_mode_key(
        &mut self,
        renderer: &mut Renderer,
        ch: char,
    ) -> bool {
        let Some(&(_, req_w, req_h)) =
            MODE_KEYS.iter().find(|(c, _, _)| *c == ch)
        else {
            return false;
        };
        let (applied_w, applied_h) =
            renderer.set_mode(req_w as usize, req_h as usize);
        self.ring.push(Event::ModeSwitch {
            requested_w: req_w,
            requested_h: req_h,
            applied_w: applied_w as u32,
            applied_h: applied_h as u32,
            timestamp_ms: self.clock_ms,
        });
        self.repaint(renderer);
        true
    }
}
```

**Remove `#[allow(dead_code)]` from:**

- `Event::ModeSwitch` in `src/event.rs` (now has an emitter).
- `Scene::repaint` in `src/scene.rs` (now has a caller).
- `Scene::repaint_script_prefix` in `src/scene.rs` (called
  by `repaint`, transitively used by `try_handle_mode_key`).
- The `RepaintState` enum in `src/scene.rs` (consumed via
  `repaint`).

**Constraints:**

- `pub(crate)` visibility for `try_handle_mode_key`. It is
  called by other `scene.rs` code today (added in 2c, 2d);
  no public API surface is needed.
- The `MODE_KEYS` table uses `u32` for width / height (not
  `usize`) to match the `ModeSwitch` event's field types
  exactly — avoids casting noise at the emitter.
- No toast yet. Step 2b adds the toast call; 2a's `repaint`
  call cleans the screen and that's it.
- No callers in 2a. The dispatcher exists; nothing in any
  runner invokes it. Step 2d wires the call sites.
- The dispatcher is dead code in 2a after the
  `#[allow(dead_code)]` removals on the other items; but it
  is not dead code in 2d, and we want to land 2a as a
  reviewable atom. Mark the dispatcher itself with
  `#[allow(dead_code)] // Wired in step 2d.` for the 2a
  commit; remove in 2d.

**Smoke test:** `make screenshot` after 2a should produce
exactly the same 59-event transcript as Phase 1's
`a84df16` baseline (no behavioural change because no caller
exists).

### 2b — on-screen toast

**Files:** `src/scene.rs`.

**State to add:**

```rust
/// Constants near the top of scene.rs.
const TOAST_MS: u64 = 1500;
```

```rust
/// Per-instance toast tracking. Toast text is not stored
/// once drawn — only the remaining TTL matters for cleanup.
#[derive(Copy, Clone, Debug)]
struct ToastState {
    remaining_ms: u64,
}
```

```rust
pub struct Scene {
    // ... existing fields, plus:
    toast: Option<ToastState>,
}
```

Initialise to `None` in `Scene::new`.

**Helpers to add to `Scene`:**

```rust
/// Draw a toast on the bottom row naming the applied mode.
/// Format depends on whether the firmware substituted:
///   exact       → "mode 1024x768"
///   substitute  → "requested 1280x720 -> using 1024x768"
///
/// ASCII only — the renderer's font is ASCII (any non-ASCII
/// character renders as `?`). Stores `Some(ToastState { ... })`
/// on `self.toast` so `tick_toast` can clear it later.
fn draw_toast(
    &mut self,
    renderer: &mut Renderer,
    requested: (u32, u32),
    applied: (u32, u32),
) {
    let row = renderer.screen_rows().saturating_sub(1);
    // Compose into a small fixed buffer; UEFI no_std + alloc
    // is fine for write! into a heapless::String, but for
    // simplicity use core::fmt::Write into a Vec<u8> + draw
    // char-by-char via draw_text_at.
    // ... (see implementation note below)
    self.toast = Some(ToastState { remaining_ms: TOAST_MS });
}
```

**Implementation note for `draw_toast`'s buffer:** the
renderer doesn't take strings via a single `&str`-rendering
path beyond `draw_line` / `draw_text_at`, both of which
take `&str`. The cleanest approach in `no_std` + `alloc` is
either:

1. `alloc::string::String` formed via `format!`, then passed
   to `draw_text_at(&s, 0, row)`. Costs one heap allocation
   per toast; perfectly fine for an event that fires at
   human-typing rates.
2. A small fixed-size byte buffer with `core::fmt::Write`
   bridge. Avoids the allocation; more code.

Pick (1) unless clippy complains. The `alloc` feature is
already enabled in `Cargo.toml` and used by
`Renderer::available_modes`'s `Vec` return.

```rust
/// Tick the toast TTL by `dt_ms`. If the TTL elapses, clear
/// the toast by repainting the entire scene (cheap, and the
/// canonical way to undo any partial-row state).
fn tick_toast(&mut self, renderer: &mut Renderer, dt_ms: u64) {
    if let Some(state) = self.toast.as_mut() {
        if state.remaining_ms <= dt_ms {
            self.toast = None;
            self.repaint(renderer);
        } else {
            state.remaining_ms -= dt_ms;
        }
    }
}
```

**Modify `try_handle_mode_key`** (added in 2a) to draw the
toast after `repaint`:

```rust
self.repaint(renderer);
self.draw_toast(renderer, (req_w, req_h), (applied_w as u32, applied_h as u32));
```

Order: `repaint` first (so the screen is clean after the
mode switch), then `draw_toast` (so the toast lands on top
of the freshly-repainted background and is not wiped).

**Constraints:**

- Bottom-row position is reliably empty in awaiting / parked
  / mid-PRE / mid-POST: at the smallest mode (640×480) we
  have 28 rows total and the boot transcript uses ~24, so
  there is at least one empty row at the bottom. Larger
  modes have more. The master plan's open question on toast
  row collision is satisfied by this analysis.
- `tick_toast` calls `repaint` to clear, which calls
  `Renderer::clear` + chrome + script-prefix replay. That
  is more work than `clear_row(bottom_row)` would do, but
  it is the canonical "redraw from state" operation and
  avoids any partial-row reasoning. Cheap enough at human
  scale.
- `tick_toast` is a no-op when `self.toast` is `None`.
- Toast does not get its own ring-buffer event; the
  `ModeSwitch` event already records the semantic outcome.
- Per-glyph BltOp throughout (principle 6); reusing
  `draw_text_at` preserves this automatically.
- Wire `tick_toast` into the runner blink loops in step 2d,
  not 2b. Step 2b only adds the helpers and the dispatcher
  modification.

### 2c — cycle mode (key `'0'`)

**Files:** `src/scene.rs`, `src/event.rs`.

This is the high-effort step of Phase 2. The brief is
deliberately question-shaped where the design has subtle
choices to make.

**The problem.** Pressing `'0'` walks every mode in
`Renderer::available_modes()` with a `CYCLE_DWELL_MS = 1000`
dwell per step, drawing a toast on each. The walk must be
interruptible: any keypress mid-walk stops the cycle, the
binary settles at the most recent mode, and the cycle emits
a `ModeCycle` event with `interrupted: true` and the count
of mode switches actually performed before interruption.

**The design question.** The master plan's open question on
cycle interruption settles on: "any keypress during the
cycle stops the walk; if the interrupting key is itself a
mode key, it is honoured (stop + apply the new request)."
Implementing this cleanly is the subtle part:

1. The cycle's dwell loop calls `poll_key` periodically.
   If it returns `Some((ch, sc))`, we want to break out.
2. The interrupting `(ch, sc)` is "consumed" by `poll_key` —
   it's a one-shot read.
3. If `ch` is a mode key (`'1'`–`'6'` or `'0'`), the
   master-plan default is to honour it — i.e. dispatch it
   recursively after pushing the `ModeCycle` event for the
   cycle that's just ending.
4. If `ch` is anything else, we discard it (small acceptable
   loss of one keystroke versus the simpler implementation).

**Suggested shape:**

```rust
const CYCLE_DWELL_MS: u64 = 1000;
```

```rust
impl Scene {
    /// Walk every available GOP mode, dwelling
    /// `CYCLE_DWELL_MS` per step. Interruptible by any
    /// keypress: the cycle stops, the binary settles at the
    /// most recent mode, and a single `ModeCycle` event is
    /// pushed naming `count` (modes switched) and
    /// `interrupted` (whether the walk completed naturally).
    ///
    /// If the interrupting key is itself a mode key (`'1'`–
    /// `'6'` or `'0'`), it is honoured by recursing into
    /// `try_handle_mode_key`. Other keys are discarded.
    pub(crate) fn cycle_modes(
        &mut self,
        renderer: &mut Renderer,
    ) {
        let modes = renderer.available_modes();
        let mut count: u32 = 0;
        let mut interrupted_by: Option<char> = None;

        'outer: for (w, h) in modes {
            // Apply this step.
            let (applied_w, applied_h) =
                renderer.set_mode(w, h);
            self.ring.push(Event::ModeSwitch {
                requested_w: w as u32,
                requested_h: h as u32,
                applied_w: applied_w as u32,
                applied_h: applied_h as u32,
                timestamp_ms: self.clock_ms,
            });
            self.repaint(renderer);
            self.draw_toast(
                renderer,
                (w as u32, h as u32),
                (applied_w as u32, applied_h as u32),
            );
            count += 1;

            // Dwell with key polling.
            let mut elapsed: u64 = 0;
            while elapsed < CYCLE_DWELL_MS {
                if let Some((ch, sc)) = poll_key() {
                    self.ring.push(Event::Keypress {
                        unicode: ch,
                        scancode: sc,
                        timestamp_ms: self.clock_ms,
                    });
                    interrupted_by = Some(ch);
                    break 'outer;
                }
                self.tick_toast(renderer, POLL_MS);
                stall(&mut self.clock_ms, POLL_MS);
                elapsed += POLL_MS;
            }
        }

        self.ring.push(Event::ModeCycle {
            count,
            interrupted: interrupted_by.is_some(),
            timestamp_ms: self.clock_ms,
        });

        // Honour the interrupting key if it was a mode key.
        if let Some(ch) = interrupted_by {
            let _ = self.try_handle_mode_key(renderer, ch);
        }
    }
}
```

**Modify `try_handle_mode_key`** (from 2a/2b) to delegate
`'0'` to `cycle_modes`:

```rust
if ch == '0' {
    self.cycle_modes(renderer);
    return true;
}
```

Place this branch *before* the `MODE_KEYS` lookup so `'0'`
is handled without falling through.

**Open questions for 2c:**

- **`'0'` while a cycle is already running.** Cycle
  dispatches the interrupting key via
  `try_handle_mode_key`; `'0'` re-enters `cycle_modes`,
  which restarts the walk. Each entry pushes its own
  `ModeCycle` event so the ring buffer's event order
  remains parseable. Stack growth is one frame per
  interruption — bounded in practice; an operator pounding
  `'0'` will stop before the stack overflows. **Default:
  accept the recursion.** If a future operator finds the
  recursion problematic, convert to a loop with a `while`
  guard.
- **Pushing the `Keypress` event for the interrupting key.**
  The cycle's `poll_key` consumes the key off the firmware
  queue, so the runner that originally polled (e.g.
  `run_awaiting`) does not see it. Pushing `Keypress` here
  preserves the audit trail. The subsequent
  `try_handle_mode_key` call may push another event
  (`ModeSwitch` or another `ModeCycle`) — that's two
  events for one keystroke, which is correct: one raw,
  one semantic.
- **`tick_toast` inside the dwell.** Without it, a toast
  drawn at the start of step N is displayed for the
  full `CYCLE_DWELL_MS`, then immediately overwritten by
  step N+1's toast (and its repaint). The
  `repaint` at the start of N+1 wipes the previous
  toast pixels. The `tick_toast` call is included for
  uniformity but is functionally a no-op during normal
  cycle progression — the toast TTL is `TOAST_MS = 1500`,
  the dwell is `1000`, so toast clean-up is always pre-
  empted by the next step's repaint. **Default: leave the
  `tick_toast` call in for symmetry.**
- **Event order at cycle end.** The `ModeCycle` event is
  pushed *after* the per-step `ModeSwitch` events and
  *before* the recursive `try_handle_mode_key` (which
  pushes its own events). Ring-buffer parsers see:
  `ModeSwitch × N, ModeCycle, [ModeSwitch ...]` for an
  interrupted-by-mode-key cycle, or
  `ModeSwitch × N, ModeCycle` for completion or non-mode
  interruption. **Default: this order.**
- **Cycle modes with zero available modes.** Per UEFI spec,
  GOP guarantees at least one mode. If `available_modes`
  returns an empty `Vec`, the `for` loop has zero
  iterations, count stays 0, and a `ModeCycle { count: 0,
  interrupted: false }` event is pushed. Acceptable
  degenerate case.

**Why opus / why worktree:**

- Opus: the design has multiple subtle interactions —
  recursion semantics, event ordering, what happens when
  a cycle interrupts another cycle, what `tick_toast`
  inside the dwell loop does. Getting it right at the
  first commit avoids a redo across multiple call sites.
- Worktree: the first attempt may be wrong in a way that
  needs the management-session review to catch. Easy to
  discard if the shape is off.

### 2d — wire dispatcher into runner sites

**Files:** `src/scene.rs` only.

**Three call sites:**

1. **`run_awaiting`'s blink loop.** Today the runner reads
   a key, pushes Keypress, transitions to Booting. The new
   shape: read a key, push Keypress, then call
   `try_handle_mode_key`. If it returns true, *do not
   transition* — stay in awaiting. If it returns false,
   transition exactly as today. Also call `tick_toast` in
   the per-tick loop body.

2. **`play_script`'s per-step stall.** Replace the existing
   `stall(&mut self.clock_ms, PACE_LINE_MS)` with a polling
   variant — call it `stall_with_keys` or similar — that
   ticks down `PACE_LINE_MS` in `POLL_MS` chunks, checks
   `poll_key` each chunk, dispatches mode keys via
   `try_handle_mode_key` (and pushes Keypress events), and
   calls `tick_toast` each chunk. Non-mode keys are
   discarded. The pacing budget continues to count down
   *across* the dispatch (i.e. dispatching a mode key mid-
   stall does not reset the stall — the line's pacing
   continues from where it was). Document this in the
   helper's doc-comment.

3. **`run_parked`'s blink loop.** Same shape as
   `run_awaiting`: read key, push Keypress, dispatch via
   `try_handle_mode_key`. If consumed, stay in parked. If
   not, exit the loop (which leads to ACPI shutdown
   exactly as today). Also call `tick_toast` in the
   per-tick loop body.

**Suggested helper:**

```rust
/// Stall for `total_ms` while polling for keystrokes. Mode
/// keys are dispatched via `try_handle_mode_key`; non-mode
/// keys are discarded (a small behaviour change from the
/// pre-Phase-2 binary, where non-mode keys would sit in the
/// firmware queue and be drained by `run_parked`'s blink
/// loop). Toast TTL is ticked each chunk.
///
/// The total stall budget is fixed: dispatching a mode key
/// mid-stall does not reset the budget. Wall-clock time
/// spent in `try_handle_mode_key` is *not* deducted from
/// the budget either — the stall accounts only for `stall`
/// calls within its own loop.
fn stall_with_keys(
    &mut self,
    renderer: &mut Renderer,
    total_ms: u64,
) {
    let mut elapsed: u64 = 0;
    while elapsed < total_ms {
        let chunk = POLL_MS.min(total_ms - elapsed);
        if let Some((ch, sc)) = poll_key() {
            self.ring.push(Event::Keypress {
                unicode: ch,
                scancode: sc,
                timestamp_ms: self.clock_ms,
            });
            // Returns false for non-mode keys — discard.
            let _ = self.try_handle_mode_key(renderer, ch);
        }
        self.tick_toast(renderer, chunk);
        stall(&mut self.clock_ms, chunk);
        elapsed += chunk;
    }
}
```

`play_script` then becomes (only the stall lines change):

```rust
// Before:
//   stall(&mut self.clock_ms, PACE_LINE_MS);
// After:
self.stall_with_keys(renderer, PACE_LINE_MS);
```

**Modify the runner blink loops** to dispatch + tick toast:

```rust
// run_awaiting (similar in run_parked):
loop {
    let glyph = self.cursor.tick(POLL_MS);
    Self::draw_or_clear_cursor(renderer, glyph, CURSOR_COL, CURSOR_ROW);
    self.tick_toast(renderer, POLL_MS);
    stall(&mut self.clock_ms, POLL_MS);

    if let Some((ch, sc)) = poll_key() {
        self.ring.push(Event::Keypress {
            unicode: ch,
            scancode: sc,
            timestamp_ms: self.clock_ms,
        });

        if self.try_handle_mode_key(renderer, ch) {
            // Mode key consumed — stay in awaiting.
            continue;
        }

        // Non-mode key — fall through to the existing
        // transition logic.
        self.ring.push(Event::SceneTransition { /* ... */ });
        self.phase = Phase::Booting;
        return;
    }
}
```

**Remove the dispatcher's `#[allow(dead_code)]`** (added in
2a) since it now has callers.

**Constraints:**

- Bootloader untouched. Confirm at review time that
  `src/bootloader.rs` is unmodified by this commit.
- `play_script`'s polling stall must not change the *order*
  of operations: line render, ring-buffer LineRendered
  event, row increment, repaint_state update — all happen
  *before* the stall, exactly as today. Only the stall's
  internals change.
- The behaviour change for non-mode keys during boot
  (today's "buffered for run_parked" → after this step's
  "discarded") is documented in the commit message and in
  this phase plan's *Outcome* section after the commit
  lands.
- `tick_toast` calls happen alongside cursor ticks at
  `POLL_MS` resolution. With `TOAST_MS = 1500` and
  `POLL_MS = 50`, a toast persists for 30 ticks — visually
  smooth.

### 2e — smoke-test exit + closeout

**Files:** `docs/plans/PLAN-display-mode-keystrokes-phase-02-keystrokes.md`,
`docs/plans/PLAN-display-mode-keystrokes.md`.

**Run, in order:**

1. `make screenshot` — verifies the no-keystroke path is
   regression-free. Compare event count against Phase 1's
   59-event baseline; should still be 59 (no synthetic
   mode keys are injected by `screenshot.sh`).
2. `make qemu` — launch interactively, exercise mode keys
   `'1'`–`'6'` and `'0'` from the awaiting screen, the
   booting screen (mid-script), and the parked screen.
   Confirm: window resizes, scene repaints cleanly, toast
   appears for ~1.5 s naming the applied mode, ring-buffer
   events drained at shutdown include `ModeSwitch` and
   `ModeCycle` lines with the queried-back applied
   resolutions.
3. `make spice-ryll` — the acceptance test. Build ryll's
   `display-mode-ui` branch first (per the master plan's
   *Mission* section's setup), then:
   - With ryll's "Obey guest size hints" toggle **on** (the
     default), press each of `'1'`–`'6'` and confirm ryll's
     window re-fits to each new resolution. Press `'0'` and
     confirm ryll's window tracks every step of the cycle.
   - Toggle "Obey guest size hints" **off** in ryll's
     hamburger menu. Press a mode key. Confirm ryll's
     window stays put while the binary's scene rendering
     visibly resizes inside the pinned window.
   - Toggle "Obey guest size hints" back **on**. Press
     another mode key. Confirm ryll's window re-fits.
   - Walk the four edge cases from the master plan's
     *Mission*: maximised window, drag-then-mode-change,
     toggle-off / change / toggle-on, drag-then-different-
     mode-than-requested.

   Capture observations inline in this phase plan's
   *Outcome* section. Anything surprising in qxl mode
   support (the open question carried over from Phase 1)
   gets recorded back in the master plan's *Open
   questions*.

4. `make release-verify` — should still pass; no changes
   to the startup-banner path.

5. `pre-commit run --all-files` — clean.

**Closeout edits:**

- Mark this phase plan's *Outcome* section Complete with
  the commit range from steps 2a–2d (and 2e's closeout
  commit if any tuning lands).
- Tick the *Exit criteria* checklist below with per-item
  verification notes pointing at the relevant commits or
  smoke-test observations.
- Mark Phase 2 Complete in the master plan's *Execution*
  table.
- If qxl mode-list capture happened, record the list in
  the master plan's *Open questions*. If there is
  divergence from the std list, decide whether any of the
  six binding keys need fallback notes for qxl.

**Constraints:**

- Step 2e is operator-driven for the ryll acceptance test.
  Sub-agent invocation should be minimal here — possibly
  none if the management session takes 2e directly. The
  `make screenshot` and `pre-commit` checks can be run
  by a sub-agent; the interactive ryll test is operator
  work.

## Exit criteria

- [ ] `Renderer::screen_rows()` exists and returns the
      number of whole text cells fitting between the top
      and bottom margins.
- [ ] `Scene::try_handle_mode_key(&mut Renderer, char) ->
      bool` exists; maps `'1'`–`'6'` to the master plan's
      resolution table; delegates `'0'` to `cycle_modes`;
      returns false for any other character; calls
      `set_mode` then `repaint` and pushes a `ModeSwitch`
      event with the queried-back applied dimensions.
- [ ] `Scene::cycle_modes(&mut Renderer)` walks
      `Renderer::available_modes()` with `CYCLE_DWELL_MS`
      dwell, pushes per-step `ModeSwitch` events, and a
      single `ModeCycle` event on exit. Interruptible by
      any keypress; mode-key interrupts are honoured by
      recursive dispatch.
- [ ] `ToastState` + `Scene::draw_toast` + `Scene::tick_toast`
      exist; toast appears for `TOAST_MS` (1500 ms) on the
      bottom row, with `mode WxH` for exact-match and
      `requested WxH -> using AxB` for substitutions.
- [ ] `play_script`'s pacing stall is replaced with a
      polling-aware stall that dispatches mode keys and
      ticks the toast TTL. Boot-script line ordering and
      `LineRendered` event ordering are unchanged.
- [ ] `run_awaiting` and `run_parked` blink loops dispatch
      mode keys and tick the toast TTL.
- [ ] `src/bootloader.rs` is unmodified. The bootloader's
      input loop continues to handle its own R/I/A and
      paste-capture keys; mode keys received there are
      logged-and-ignored as today.
- [ ] `#[allow(dead_code)]` is removed from
      `Event::ModeSwitch`, `Event::ModeCycle`,
      `Scene::repaint`, `Scene::repaint_script_prefix`,
      and `RepaintState`.
- [ ] `make screenshot` produces the same 59-event
      transcript as Phase 1's baseline (no regression in
      the no-keystroke path).
- [ ] `make qemu` exercises mode keys end-to-end: pressing
      `'1'`–`'6'` resizes the GTK window and repaints the
      scene cleanly; pressing `'0'` walks every available
      mode with the toast naming each step; an
      interrupting keypress mid-cycle stops the walk and,
      if a mode key, applies the new request.
- [ ] `make spice-ryll` against ryll's `display-mode-ui`
      branch confirms ryll's window tracks every mode
      change with Obey-on; stays pinned with Obey-off;
      re-fits when Obey is toggled back on. The four edge
      cases from the master plan's *Mission* are walked
      and behaviour matches the documented expectations.
- [ ] `make release-verify` continues to pass (raw + qcow2).
- [ ] `pre-commit run --all-files` exits 0 at every commit
      across the phase.
- [ ] Commit messages follow the project's template.
- [ ] Master plan's *Execution* table marked Phase 2
      Complete with the commit range.

## Risks and open questions

- **`Renderer::set_mode` wall-cost during a fast cycle.**
  Each cycle step calls `set_mode` (which calls
  `gop.set_mode`, then `gop.modes()` again to iterate).
  At 30 modes × 1 s dwell, the cycle takes ~30 s. The
  wall cost of each `set_mode` is small (firmware
  micro-seconds), but if it turns out to add visible
  delay, consider caching the modes list at the start of
  the cycle and not re-querying inside the inner loop.
  Step 2c's review should check this.

- **Repaint flicker during cycle.** Each step does
  `clear()` → `draw_chrome()` → script replay → toast.
  At 30 steps that is a lot of clears. If the operator
  reports objectionable flicker, consider implementing a
  "draw-toast-only" path that updates the bottom row
  without a full clear. Defer until iterated against in
  step 2e.

- **Toast text wraps the screen at 640×480.** The toast
  format `requested 1280x1024 -> using 1024x768` is ~40
  characters. At 640×480 we have 76 cells (`(640 - 32) /
  8`), so it fits. At any larger mode it definitely fits.
  Confirmed safe.

- **`run_booting` mode-switch visual acceptability.** The
  master plan flagged this as a high-effort decision; the
  static-replay shape from Phase 1's `Scene::repaint`
  satisfies it. If operator iteration in step 2e finds
  the static replay jarring, fall back to "buffer the
  keystroke and apply at the next scene boundary" by
  storing the pending request on `Scene` and dispatching
  it at the run_parked entry. Capture the decision in
  step 2e's notes.

- **Behaviour change for non-mode keys during run_booting.**
  Today, non-mode keys pressed during run_booting sit in
  the firmware key queue and are drained by run_parked.
  After step 2d, they are read and discarded. Acceptable
  per the master plan's *Out of scope* note that this is
  a test guest not a key-buffering oracle, but worth
  flagging in step 2d's commit message.

- **Interrupt-key event order.** The cycle's interrupting
  key generates two ring-buffer events: a `Keypress` (raw)
  pushed by `cycle_modes` itself, and (if it is a mode key)
  the resulting `ModeSwitch` or `ModeCycle` from the
  recursive dispatch. Parsers reading the ring buffer
  must accept this two-events-per-keystroke pattern. The
  pre-Phase-2 binary already exhibits this for some paths
  (Keypress + SceneTransition) so it is not a new
  invariant.

- **`set_mode` failing under qxl.** The qxl mode list may
  differ from the std list (open question carried over
  from Phase 1). If a key-table entry has no near-enough
  qxl mode, the toast will name a substitution and the
  `ModeSwitch` event will record the substitution
  faithfully. No code change needed — just operator
  awareness during step 2e.

- **`heapless::String` vs `alloc::String` for toast text.**
  Picked `alloc::String` for simplicity. If clippy or
  binary-size review flags the heap allocation as
  objectionable, swap to a `[u8; N]` + `core::fmt::Write`
  bridge. The cost is one allocation per mode key (human
  rate) — almost certainly fine.

## Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with it. In particular:
confirm the locked-bootloader carve-out (no edits to
`src/bootloader.rs`), confirm the recursion semantics for
cycle-during-cycle and mode-key interrupts, confirm the
toast-cleanup-via-repaint approach, and confirm the
behaviour change for non-mode keys during `play_script`.
