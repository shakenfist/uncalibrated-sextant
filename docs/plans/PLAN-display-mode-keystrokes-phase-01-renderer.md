# Display-mode keystrokes — phase 1: renderer foundation

Parent plan:
[PLAN-display-mode-keystrokes.md](PLAN-display-mode-keystrokes.md).

## Outcome

**Status: Not started.**

This section will be populated as Phase 1 lands, in the same
shape as `PLAN-locked-bootloader-phase-01-spice-infra.md`'s
*Outcome* section: a one-paragraph headline, the list of what
shipped, and the list of what was *not* shipped and why.

## Prompt

Before working on this phase, re-read the master plan's
*Phase 1 sketch* and the *Open questions* entries it points to
(especially the GOP-mode-availability question, the
`run_booting` redraw policy, and the "query back from GOP, do
not trust the request" rule). Skim:

- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) — current
  one-shot `(1024, 768)` selection in `Renderer::new`. Phase 1
  generalises that into a reusable `set_mode` plus a pure
  nearest-mode helper that the boot-time path itself starts
  using.
- [`src/event.rs`](../../src/event.rs) — existing `Event`
  variants and the `BootloaderChoice` precedent for "tag()
  returns a stable lowercase string for the serial drain."
- [`src/serial.rs`](../../src/serial.rs) —
  `with_serial`, `write_startup_banner`, and the per-variant
  formatting in `drain`. Phase 1 adds one more emitter and one
  more variant to the formatter.
- [`src/scene.rs`](../../src/scene.rs) — the `Scene` struct,
  `play_script`, `run_awaiting`, `run_booting`, `run_parked`.
  Phase 1 adds a `RepaintState` field that scene runners
  update as they play, and a `Scene::repaint` method that
  reads it. The method has no callers in this phase; Phase 2
  wires the dispatcher.
- [`scripts/qemu.sh`](../../scripts/qemu.sh) and
  [`scripts/spice-ryll.sh`](../../scripts/spice-ryll.sh) for
  the smoke-test paths used at the end of the phase.

Where the UEFI spec or `uefi-rs` semantics matter, read first:

- `uefi::proto::console::gop::GraphicsOutput` —
  `modes()`, `set_mode(&Mode)`, `current_mode_info()`,
  `Mode::info().resolution()`. The existing `Renderer::new`
  uses all three already; Phase 1 just lifts them into a
  reusable shape.
- UEFI 2.10 §12.9 — the spec guarantees that `set_mode()`
  invalidates the framebuffer (it is the caller's
  responsibility to redraw). The repaint hook is the place
  where this contract gets honoured.

This phase plans at **medium effort** overall, with one
**high-effort opus** step for the `Scene::repaint` design.

## Goal

Land all the plumbing the keystroke dispatcher (Phase 2) will
need, *without* yet exposing any user-facing keystroke or
behavioural change. By the end of this phase, the binary still
boots, plays the same scene, and parks the same way; but
underneath:

- A reusable `Renderer::set_mode(width, height) -> (usize,
  usize)` exists, tested by the existing boot-time
  `(1024, 768)` selection now routing through it.
- The full list of GOP modes available under our default QEMU
  + OVMF configuration is dumped to serial once at boot, and
  the observed list has been recorded back into the master
  plan's *Open questions* section so Phase 2 can choose key
  bindings against ground truth rather than priors.
- `Event::ModeSwitch` and `Event::ModeCycle` are defined and
  rendered by the serial drain. Nothing emits them yet.
- `Scene::repaint(&mut self, &mut Renderer)` exists and can
  redraw the current scene state at the current renderer
  dimensions. It is called nowhere in Phase 1; it exists so
  Phase 2's dispatcher has something to call.

## Scope

**In scope:**

- Renderer changes: nearest-mode helper, `set_mode`,
  `available_modes()` (or equivalent), and refactoring
  `Renderer::new` to use them.
- One new serial emitter: `serial::write_available_modes`.
- Two new ring-buffer events and their formatter rows.
- `Scene::repaint` mechanism: per-phase repaint state, a
  field on `Scene`, and the method itself.
- Smoke test: `make qemu` boots, plays the scene, the new
  serial line appears, the rest of the run is byte-identical
  to the previous behaviour (modulo the new line and the
  refactored `Renderer::new` taking the same path it used to
  take by hand).

**Out of scope (Phase 2):**

- Any keystroke handling for mode switching.
- Cycle-mode walk.
- On-screen toast.
- Bootloader carve-out (its existing input loop continues to
  do exactly what it does today; nothing in Phase 1 touches
  `src/bootloader.rs`).

**Out of scope (Phase 3):**

- Documentation updates beyond what is strictly required to
  describe Phase 1's serial output.
- Any inventory-row status changes.

**Deferred from the master plan:**

- **Host-side `cargo test` for `nearest_mode`.** The master
  plan's success criterion list includes a `cargo test` run.
  Investigating the codebase before drafting this phase
  revealed there is *no* host-test infrastructure today: the
  crate is `no_std` and only compiles against
  `x86_64-unknown-uefi`, all dependencies are UEFI types, and
  `cargo test` against the host triple would currently fail
  to build the crate at all. Adding a host-test target
  requires either splitting the project into a workspace or
  conditionally-compiling around the `uefi` crate — both
  larger than the function being tested. Phase 1 instead
  validates `nearest_mode` by:
  1. Construction: it is a pure function over slices of
     `(usize, usize)`, written for inspection.
  2. Indirect: `Renderer::new`'s existing `(1024, 768)` path
     now routes through it; if the helper picks the wrong
     mode the boot screen renders at the wrong resolution
     and the regression is immediately obvious.
  3. Discovery: the boot-time mode dump confirms exactly
     which `(width, height)` candidates the helper sees on
     real hardware, and Phase 2's smoke tests exercise it
     against every binding.

  The master plan's *Success criteria* checklist will be
  updated in Phase 3's closeout to reflect this — the
  `cargo test` line gets struck through with a note pointing
  here.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | Add `nearest_mode` pure helper + `Renderer::set_mode` + `Renderer::available_modes`; refactor `Renderer::new` to use them. No new public side-effects beyond what `Renderer::new` already does today. |
| 1b   | medium | sonnet | none      | Add `serial::write_available_modes` and call it once at boot. Run `make qemu` (or `make release-verify`'s serial path), capture the line, paste the observed mode list into the master plan's *Open questions* section. |
| 1c   | low    | sonnet | none      | Add `Event::ModeSwitch` and `Event::ModeCycle` variants + formatter rows in `serial::drain`. No emitters yet. |
| 1d   | high   | opus   | worktree  | Design and implement `Scene::repaint`: per-phase `RepaintState`, scene-runner update points, and the method that consults the state and redraws current content at current dimensions. No callers in this phase. Worktree-isolated because the design touches every scene runner and the first attempt may be wrong. |
| 1e   | low    | sonnet | none      | Wire-up smoke test: `make qemu` clean run, capture serial log, confirm the `available GOP modes:` line + the existing scene transcript. Tick step exit criteria below. |

Commits expected: one per step (1a–1d), with 1e ticking exit
criteria as part of the 1d commit's verification rather than
its own commit. Five commits total is fine if 1e produces a
notable finding worth its own message; expect four.

## Detailed step briefs

### 1a — `nearest_mode`, `Renderer::set_mode`, `Renderer::available_modes`

**Files:** `src/renderer/mod.rs`.

**What to add:**

```rust
/// Pick the available mode whose resolution is nearest to
/// `(req_w, req_h)`, scored as `|dw| + |dh|`. Ties broken by
/// preferring the smaller width, then the smaller height.
///
/// Returns the index into `available` of the chosen mode.
/// Returns `None` only when `available` is empty (which would
/// be a firmware bug — GOP guarantees at least one mode).
fn nearest_mode(req: (usize, usize), available: &[(usize, usize)]) -> Option<usize> {
    // ...
}
```

```rust
impl Renderer {
    /// Switch GOP to the mode nearest to the requested size,
    /// re-query GOP for the actually-applied resolution, update
    /// cached width/height, and return the applied dimensions.
    ///
    /// The applied resolution may differ from the request when
    /// the requested mode is not exposed by the firmware; the
    /// caller is responsible for surfacing that difference.
    ///
    /// Per UEFI 2.10 §12.9, `set_mode` invalidates the
    /// framebuffer. The caller must redraw before this method
    /// returns control to a scene that expects pixels intact.
    pub fn set_mode(&mut self, req_w: usize, req_h: usize) -> (usize, usize) {
        // ...
    }

    /// All `(width, height)` modes the current GOP exposes,
    /// in the order GOP returns them.
    pub fn available_modes(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        // ...
    }
}
```

**Refactor `Renderer::new`** to use `set_mode(1024, 768)`
instead of the current ad-hoc `gop.modes().find(...)`. The
existing fallback semantics ("if 1024×768 is not available,
keep current mode") become "use whatever `nearest_mode`
returns, which on real hardware is essentially always 1024×768
because OVMF exposes it." That is a behavioural *equivalent*
in practice but a behavioural *change* in principle (the old
code kept whatever GOP came up in if 1024×768 was missing; the
new code switches to the nearest available). Document the
change in the commit message.

**Constraints:**

- `no_std` + `alloc` + `global_allocator` are enabled in
  `Cargo.toml`. `Vec` is therefore available, but
  `available_modes` returning `impl Iterator` over a
  GOP-borrowed lifetime is the cleaner shape — avoids
  allocations and matches `gop.modes()`'s native return.
  `Vec<(usize, usize)>` is acceptable if the lifetime gymnastics
  prove ugly; flag the trade-off in the commit message.
- Width and height stay `usize` (matching existing fields and
  `info.resolution()`'s return).
- Tie-break logic must be deterministic. Two different runs
  must pick the same fallback mode for the same request.
- `gop.set_mode` returns `Result`. On error, log nothing (the
  existing code uses `let _ =` for the same reason: serial may
  not be available, panicking on a firmware refusal would be
  worse than continuing). Re-query `gop.current_mode_info()`
  unconditionally so cached width/height match reality even
  if the requested switch was refused.
- Per the master plan's "query back from GOP" rule, the
  return value of `set_mode` comes from
  `gop.current_mode_info().resolution()`, never from
  `(req_w, req_h)`.

**Commit message style:** the locked-bootloader commits in
`git log` are good models — short subject in imperative,
2-3 sentence body explaining the why, the `Prompt:` paragraph,
`Signed-off-by`, `Co-Authored-By` with model + context window
+ effort level. See `~/.claude/CLAUDE.md` for the canonical
template.

### 1b — boot-time GOP mode dump

**Files:** `src/serial.rs`, `src/main.rs` (or whatever
construction site `Renderer::new` is called from — `main.rs`
today).

**What to add:**

```rust
// in src/serial.rs
pub fn write_available_modes<I>(modes: I)
where
    I: IntoIterator<Item = (usize, usize)>,
{
    with_serial(|serial| {
        let _ = write!(serial, "available GOP modes:");
        for (w, h) in modes {
            let _ = write!(serial, " {w}x{h}");
        }
        let _ = writeln!(serial, "\r");
    });
}
```

**Call site:** in `main.rs`, between `serial::write_startup_banner()`
and `Scene::new`, after the `Renderer` is constructed:

```rust
let mut r = Renderer::new().expect("renderer init failed");
serial::write_available_modes(r.available_modes());
```

**Smoke test the line:**

```sh
make qemu
# wait until the awaiting screen, press a key, let it run to ACPI shutdown.
grep "available GOP modes:" dist/serial.log
```

(`scripts/qemu.sh` writes to `dist/serial.log` per its mirror
in `scripts/spice.sh`; verify the path before running.)

**Update the master plan:** copy the observed line into the
*Open questions* section under
*"Which GOP modes does OVMF actually expose under QEMU?"*,
replacing the "to be confirmed in Phase 1" placeholder with
the actual observed list. Use a short fenced block:

```text
Observed under default `make qemu` (OVMF + qemu-system-x86_64
with `-vga std`), commit <sha>:

  available GOP modes: 640x480 800x600 1024x768 ...
```

If `-vga qxl` (used by `scripts/spice-ryll.sh`) yields a
different list, capture both. The Phase 2 plan will pick key
bindings against this ground truth.

### 1c — ring-buffer events

**Files:** `src/event.rs`, `src/serial.rs`.

**Add to `Event`:**

```rust
/// GOP mode switched (or attempted to switch) at the
/// operator's request.
ModeSwitch {
    requested_w: u32,
    requested_h: u32,
    applied_w: u32,
    applied_h: u32,
    timestamp_ms: u64,
},
/// Cycle-through-all-modes walk completed (or was
/// interrupted). `count` is the number of mode switches
/// performed during the cycle.
ModeCycle {
    count: u32,
    interrupted: bool,
    timestamp_ms: u64,
},
```

`u32` rather than `usize` for `requested_*` and `applied_*`
because the serial-drain format treats them as decimal
integers and the on-the-wire shape stays the same regardless
of host pointer width. Keep `Copy + Clone + Debug` to match
existing variants.

**Extend `serial::drain` formatter:**

```rust
Event::ModeSwitch {
    requested_w,
    requested_h,
    applied_w,
    applied_h,
    timestamp_ms,
} => {
    let _ = writeln!(
        serial,
        "t={timestamp_ms} type=mode_switch requested={requested_w}x{requested_h} \
         applied={applied_w}x{applied_h}\r",
    );
}
Event::ModeCycle {
    count,
    interrupted,
    timestamp_ms,
} => {
    let _ = writeln!(
        serial,
        "t={timestamp_ms} type=mode_cycle count={count} interrupted={interrupted}\r",
    );
}
```

The `type=` tags (`mode_switch`, `mode_cycle`) are the new
parser surface for ryll. Keep them lowercase + underscore-
separated for symmetry with `bootloader_decision` and
`bootloader_timeout`.

No emitters in this phase. The variants exist; nothing pushes
them. Phase 2's dispatcher wires the pushes.

### 1d — `Scene::repaint`

**Files:** `src/scene.rs` primarily; possibly `src/event.rs`
or a new `src/scene/` submodule if the design grows. Keep it
in `scene.rs` if it fits.

This is the design-judgement step of Phase 1. The brief is
deliberately question-shaped, not prescriptive — the opus
sub-agent should propose, then the management session
reviews.

**The problem.** `Renderer::set_mode` invalidates the
framebuffer. To call it usefully we need a `Scene` method
that, given the current scene state, redraws everything
visible at the current `(width, height)`. Today, scene
runners are functions that paint-and-exit; nothing about the
scene's *current visible state* is recoverable after the
runner returns or while a runner is mid-poll-loop.

**Concrete things that need to be redrawable:**

| Phase | What's on screen | What needs to be remembered to redraw it |
|-------|------------------|------------------------------------------|
| Awaiting | Chrome (logo) + cursor at (0,0). | Just "we are in awaiting." Cursor position is fixed. |
| Booting (mid-PRE script) | Chrome + every PRE step rendered so far. | Index into `BOOT_SCRIPT_PRE` of the next step (i.e. count of completed steps). |
| Booting (in bootloader) | Chrome + all of PRE + whatever the bootloader sub-state-machine is currently showing. | The locked-bootloader scene is **carved out** from receiving mode keys per the master plan. So mid-bootloader repaint is **not required** in this phase. Decide whether to redraw "PRE only, leaving bootloader rows blank" or to suppress repaints while the bootloader is active. The master plan's carve-out makes the latter the simpler answer; favour it unless there's a reason not to. |
| Booting (post-bootloader) | Chrome + PRE + bootloader's final post-Continue rows (from `next_row`) + whatever of POST has played. | PRE step count (full), `next_row` returned from the bootloader, POST step count. |
| Parked | Chrome + entire boot transcript + SYSTEM ONLINE row + cursor. | Same as booting-finished + the SYSTEM ONLINE start row. |

**Suggested shape (for the opus sub-agent to confirm or
revise):**

```rust
/// Snapshot of what is currently visible, sufficient to
/// repaint the screen from scratch. Updated by the scene
/// runners as they play.
#[derive(Copy, Clone, Debug)]
enum RepaintState {
    Chrome,
    Awaiting,
    BootingPre {
        played: usize, // count of BOOT_SCRIPT_PRE steps already drawn
    },
    BootingBootloader {
        pre_played: usize,
        // mode keys are ignored during the bootloader; if
        // a repaint somehow happens, redraw PRE only.
    },
    BootingPost {
        pre_played: usize,
        bootloader_next_row: usize,
        post_played: usize,
    },
    Parked {
        pre_played: usize,
        bootloader_next_row: usize,
        post_played: usize,
        system_online_row: usize,
    },
}
```

**Update points** in `Scene`:

- `run_awaiting` enters: set `repaint_state = Awaiting`.
- `run_booting` enters: clear; set `BootingPre { played: 0 }`.
- `play_script` for PRE: increment `played` after each step.
- Before calling `bootloader::run`: set `BootingBootloader`.
- After bootloader returns: set `BootingPost { ..., post_played: 0 }`.
- `play_script` for POST: increment `post_played`.
- `run_parked` enters: set `Parked { ..., system_online_row }`.

`Scene::repaint(&mut self, renderer: &mut Renderer)` reads
this enum, calls `renderer.clear()`, paints chrome, and then
replays the appropriate prefix of the script(s). It does
**not** re-emit `LineRendered` events to the ring buffer —
those describe the *original* rendering at the *original*
timestamp; replays during a mode switch are not new lines.

**Open questions for 1d:**

- Should `Scene::repaint` call `Renderer::clear` itself, or
  should the dispatcher (Phase 2) clear before calling? Lean
  toward clearing inside `repaint`: that's where the
  framebuffer-invalidation contract is satisfied, and it
  keeps the per-mode-switch sequence in one place.
- Does the chrome (logo) need to repaint? Yes — `clear()`
  wipes it, and the existing `draw_chrome()` is the right
  helper to call.
- The cursor blink during awaiting / parked is driven by
  per-frame `tick`. After a repaint, the next blink tick
  will repaint the cursor naturally; no special handling is
  needed beyond ensuring `repaint_state` reflects we're back
  in awaiting / parked.
- How does the cursor's *current visible state* (drawn vs
  cleared this half-cycle) survive a repaint? Probably it
  doesn't — after repaint the cursor is freshly cleared, and
  the next tick draws it. Acceptable.
- Should `repaint` work for the locked-bootloader's R/I/A
  prompt, paste-blob, countdown, etc.? Not in this phase —
  the carve-out applies. The `BootingBootloader` variant is
  intentionally state-poor.

**Constraints:**

- Per-glyph BltOp throughout (principle 6). `repaint` must
  not introduce any monolithic framebuffer copy. Reusing
  `play_script` and `draw_telemetry_line` etc. handles this
  automatically.
- Repaint must not advance `clock_ms` or push events. It is
  read-only from the timeline's perspective.
- Repaint must be safe to call from inside a poll loop
  (e.g. mid-`run_awaiting`'s blink loop). It must not
  recurse into `Scene::run` or any of the runner methods.
- The locked-bootloader sub-state-machine owns its own
  drawing; mid-bootloader repaint is a no-op or a fallback
  that draws PRE only. Document the choice in the commit
  message.

**Why opus / why worktree:**

- Opus: the design touches every scene runner, requires
  reasoning about the bootloader carve-out's implications,
  and needs to land an enum that Phase 2 will lean on
  heavily. A wrong shape here costs a redo across the whole
  scene module.
- Worktree: the first attempt may be wrong. Easy to discard
  if review reveals the enum is too rich or too thin.

### 1e — smoke-test exit

After 1a–1d are committed, run:

```sh
make qemu
# advance through awaiting, observe boot, observe parking, ACPI shutdown.
grep "available GOP modes:" dist/serial.log
grep "type=mode_switch" dist/serial.log  # expected: no matches yet.
grep "type=mode_cycle" dist/serial.log   # expected: no matches yet.
```

```sh
make release-verify
```

```sh
pre-commit run --all-files
```

If anything fails, do not paper over — diagnose root cause
per `~/.claude/CLAUDE.md`'s problem-solving guidance.

## Exit criteria

- [ ] `Renderer::set_mode(req_w, req_h) -> (usize, usize)`
      exists, queries back the applied mode from GOP, and
      returns the applied (not requested) dimensions.
- [ ] `Renderer::new` calls `set_mode(1024, 768)` instead of
      its previous ad-hoc selection. Behaviour observably
      identical on real hardware.
- [ ] `Renderer::available_modes()` exposes the full list to
      callers.
- [ ] `nearest_mode` is a free pure function in `src/renderer/mod.rs`,
      with deterministic tie-break documented in its
      doc-comment.
- [ ] `serial::write_available_modes` exists and is called
      once at boot from `main.rs`.
- [ ] The `available GOP modes: …` line appears in
      `dist/serial.log` after a clean `make qemu` run.
- [ ] The observed mode list is recorded back into the master
      plan's *Open questions* section.
- [ ] `Event::ModeSwitch` and `Event::ModeCycle` exist and
      are rendered by `serial::drain` with the documented
      `type=mode_switch` / `type=mode_cycle` tags.
- [ ] `Scene::repaint(&mut Renderer)` exists; `Scene` carries
      the `RepaintState` (or equivalent) needed for it to
      work; scene runners update the state at the documented
      points.
- [ ] No emitter calls `ModeSwitch` / `ModeCycle` events yet
      (those land in Phase 2).
- [ ] No caller of `Scene::repaint` exists yet.
- [ ] `make qemu` boots, plays the same scene, parks, and
      ACPI-shuts-down, with serial output containing the new
      line and otherwise byte-identical to pre-Phase-1
      modulo timestamps.
- [ ] `make release-verify` continues to pass.
- [ ] `pre-commit run --all-files` exits 0 at every commit
      across the phase.
- [ ] Commit messages follow the project's template (subject
      under 50 chars ending in a period, body wrapped at 75,
      `Prompt:` paragraph, `Signed-off-by` and
      `Co-Authored-By` for the model + context + effort
      configuration of the sub-agent that did the work).

## Risks and open questions

- **`-vga std` vs `-vga qxl` mode-list divergence.** The
  default `make qemu` uses `-vga std`; `make spice` and
  `make spice-ryll` may use `qxl`. If the lists differ,
  Phase 2 must pick keystroke bindings that work under
  *both* (or document which target each binding is for).
  Step 1b should capture both lists if there is divergence.
- **`Renderer::available_modes` returning a borrow vs an
  owned vec.** A borrow is more idiomatic but requires
  threading the GOP lifetime through callers. An owned vec
  costs one allocation per call but is simpler. Pick the
  owned-vec form unless step 1a finds the borrow trivial.
- **Repaint while the bootloader is mid-paste-prompt.** The
  carve-out should make this impossible (mode keys ignored
  there). But if it ever happens — say a future scene
  adopts the same input loop — `BootingBootloader`'s
  fallback ("redraw PRE only") leaves the screen in a state
  the bootloader's own runner does not know about. Future
  scenes that adopt this pattern will need to extend the
  carve-out.
- **Discovery — `nearest_mode` may pick a surprising mode.**
  E.g. if OVMF exposes 800×600 + 1280×1024 but not 1024×768,
  the old code kept whatever boot mode came up; the new code
  routes through `nearest_mode` and switches to one of those
  two. Document this in the 1a commit message and verify the
  smoke test in 1e shows the expected outcome.
- **Locked-bootloader interaction.** The bootloader sub-
  state-machine has its own `clock_ms` and ring-buffer
  pushes via `&mut` borrows from `Scene`. `Scene::repaint`
  takes `&mut self` and `&mut Renderer`. As long as the
  bootloader does not retain any references across its own
  call, there's no aliasing problem. Confirm at 1d review.
- **Mode change before `run_awaiting` even starts.** Phase 1
  does not enable this (no keystroke handler); Phase 2 might
  consider it. Out of scope for this phase but worth flagging
  to Phase 2's planning step.

## Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the
work you intend to do aligns with it. In particular: confirm
the carve-out for the locked-bootloader scene, confirm the
"query GOP for the applied mode, never trust the request"
rule, and confirm the host-test deferral with its rationale.
