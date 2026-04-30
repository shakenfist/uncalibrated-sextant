# Display-mode keystrokes — interactive GOP mode switching for client-side resize testing

## Prompt

Before responding to questions or discussion points in this
document, read [DESIGN.md](../../DESIGN.md) (especially the
*SPICE channel mapping* table, where the *Display channel* rows
live), [docs/spice-test-inventory.md](../spice-test-inventory.md)
(especially the *Display channel* section), the renderer at
[`src/renderer/mod.rs`](../../src/renderer/mod.rs), the scene
runners and `poll_key()` helper in
[`src/scene.rs`](../../src/scene.rs), the existing keypress
handling pattern in
[`src/bootloader.rs`](../../src/bootloader.rs) (around lines 282
and 523), and the ring-buffer event enum in
[`src/event.rs`](../../src/event.rs). Ground every design choice
in what the code does today; do not speculate when you can read
instead.

Where a question touches external behaviour (which GOP modes
OVMF actually exposes under QEMU's `-vga std` and `-vga qxl`,
how `gop.set_mode()` invalidates the framebuffer, how SPICE
display-channel mode renegotiation surfaces to a connected
client), research as needed and flag any residual uncertainty
explicitly.

This plan is the implementing-session expansion of the
context-drop notes that previously occupied this filename
(seeded from the `display-mode-ui` worktree of `shakenfist/ryll`).
The motivation, suggested key mapping, and cross-repo references
in those notes have been folded into the *Situation* and *Mission*
sections below; the original notes were never committed, so
this file is the canonical record from this point on.

All planning documents live in `docs/plans/`. Phase plans will
be separate files named
`PLAN-display-mode-keystrokes-phase-NN-...md` and tracked in the
*Execution* table below.

One commit per logical change as before; minimum one commit per
phase. Each commit should build, pass `pre-commit run
--all-files`, and have a clear message in the project's
existing style (see `CLAUDE.md` for the commit-message
template, including the required `Signed-off-by`,
`Co-Authored-By`, and `Prompt:` paragraph).

Cross-repo references, in order of likely usefulness:

- `shakenfist/ryll`, branch `display-mode-ui` — the consumer of
  this affordance. Lands a fix for ryll's window only
  auto-fitting to the *first* `SURFACE_CREATE` of a session;
  adds a `Obey guest size hints` hamburger toggle and a
  `--no-obey-guest-size` CLI flag. The keystrokes added by this
  plan are how a human operator drives that fix and its
  follow-ups (window-maximised behaviour, drag-then-mode-change,
  toggle-off-then-on round-trips).
- `shakenfist/ryll/docs/plans/PLAN-display-window-sizing.md` —
  master plan for the ryll-side fix; phase 5 of that plan adds
  resolution-change notifications and will reuse this same
  keystroke affordance.
- External: the [UEFI 2.10 spec, §12.9 *Graphics Output
  Protocol*](https://uefi.org/specs/UEFI/2.10/) for `set_mode`
  semantics and framebuffer invalidation; `uefi-rs` crate docs
  for the Rust binding shape (`GraphicsOutput::query_mode`,
  `set_mode(&Mode)`, `Mode::info().resolution()`).

## Situation

The first-playable milestone (commits `862b638` → `a30bc38`)
landed a UEFI binary that picks a single GOP mode at boot —
[`src/renderer/mod.rs:52-72`](../../src/renderer/mod.rs)
enumerates `gop.modes()`, looks for `(1024, 768)`, calls
`gop.set_mode(&mode)`, and caches `width` / `height` on the
`Renderer`. Once set, the mode never changes for the life of
the binary. The subsequent boot-sequence text *narrates* a
mode probe (`640x480x8 ... OK` etc. at
[`src/scene.rs:127-130`](../../src/scene.rs)) but does not
actually retune GOP — it is diegetic flavour.

Triaging the ryll `display-mode-ui` fix exposed a testing
problem this static-mode design creates: the most interesting
guest behaviour for a SPICE client to track is *the guest
changing modes mid-session*, and we have no way to trigger that
on demand. The closest analogue is the OVMF firmware itself,
which cycles `640×480 → 800×600 → 1024×768` during boot in
~1 second — far too fast for a human to verify the client's
window-tracking, and impossible to use for edge cases like:

* "Toggle obey-guest-size off, change mode, toggle on" — does
  the SPICE client window stay pinned, then snap back on the
  next change?
* "Maximise the SPICE client window, change mode" — does the
  surface render at native size inside the maximised window
  without trying to resize the window?
* "Drag the SPICE client window, then have the guest pick a
  *different* mode than the one the client requested" — does
  the client follow the guest's actual choice rather than
  echoing its own request?

Within the SPICE test inventory at
[docs/spice-test-inventory.md](../spice-test-inventory.md), this
work lands on the *Display channel* section. The *Mode set*
row already carries `binary: phase 4` (the boot-time set in
`Renderer::new`); the *Mode walk across all offered modes* row
is currently `—` and is the natural home for this milestone's
`binary:` link. (The *Bit-depth changes* row stays `—` —
GOP under OVMF is effectively 32 bpp BGR; we will not pursue
8/16 bpp here.)

This is also the first uncalibrated-sextant change motivated by
a sibling project's testing needs rather than the inventory's
own roadmap. That is fine — the affordance is reusable for
future display-mode work in this binary too (scaling,
letterboxing, multi-head simulation), and treating it as a
first-class control surface rather than a hidden backdoor keeps
it in step with the project's "this is a test guest, test
affordances belong in the documented control surface" stance.

## Mission and problem statement

Add documented keystrokes to uncalibrated-sextant that switch
the GOP framebuffer to a chosen resolution at any point after
boot, and redraw the current scene at the new dimensions. The
keystrokes are first-class — they appear in `README.md` and the
on-screen / in-binary control reference, not as a hidden
debug-only thing.

Initial key mapping (the implementing session may iterate based
on what GOP modes OVMF actually exposes under
`-vga std` / `-vga qxl`):

| Key | Resolution                                        |
|-----|---------------------------------------------------|
| `1` | 640×480                                           |
| `2` | 800×600                                           |
| `3` | 1024×768                                          |
| `4` | 1280×720                                          |
| `5` | 1280×1024                                         |
| `6` | 1920×1080                                         |
| `0` | cycle through every available mode, ~1 s per mode |

When the requested mode is unavailable, the binary picks the
nearest available mode, displays a brief on-screen note that
makes the substitution visible to the operator (e.g.
`requested 1280×720 → using 1024×768`), and emits a
`ModeSwitch` ring-buffer event with the *actual* resolution
applied so ryll-driven assertions see ground truth.

Mode switching must work in `run_awaiting`, `run_booting`
(where reasonable — see *Open questions*), and `run_parked`.
The locked-bootloader scene has its own input loop and is
**out of scope** for this milestone; keystrokes there continue
to mean what they mean today (`R`/`I`/`A` and paste capture).

The milestone is done when:

1. An operator running `make spice-ryll` against the
   `display-mode-ui` branch of ryll can press `1`–`6` and `0`
   from the awaiting / parked screens and watch ryll's window
   re-fit to each new resolution.
2. The same operator can toggle ryll's `Obey guest size hints`
   off, press another resolution key, and observe ryll stays
   pinned — i.e. the GOP mode genuinely changed (the binary's
   own scene now renders at the new size visible *inside* the
   pinned window) but ryll did not refit.
3. Ring-buffer events for every mode switch arrive on serial in
   order, with the actual applied resolution, so a future ryll
   assertion harness has machine-checkable ground truth.
4. `make qemu` (GTK) still works — the keystrokes are reachable
   there too, and QEMU's GTK display reflects the new size
   exactly as ryll does.

## Open questions

Defaults below are strong but worth confirming or iterating at
the relevant phase. Capture changes inline rather than letting
them drift.

- **Which GOP modes does OVMF actually expose under QEMU?**
  **Default to be confirmed in Phase 1.** The implementing
  session should log the full output of `gop.modes()` once at
  boot to serial (under a debug feature or unconditionally as a
  one-time line) and record the observed list inline in this
  section before Phase 2 begins. Strong prior: 640×480,
  800×600, 1024×768, plus whatever QEMU's stdvga / qxl ROM
  advertises (commonly 1280×1024, sometimes 1920×1080). If a
  table key has no corresponding mode under our default
  hardware, that key falls back per the *unavailable mode*
  rule above; we do not silently drop it from the documented
  reference.

- **Should `0` (cycle) be interruptible?** **Default: yes.**
  Pressing any other key during the cycle stops the walk and
  applies the most recent mode. Otherwise the operator has to
  wait out the whole sequence. The cycle emits a single
  `ModeCycle { start, end, count }` event when it finishes
  (or is interrupted) in addition to the per-step `ModeSwitch`
  events.

- **Cycle dwell time.** **Default: 1.0 s per mode.** Long enough
  for an operator to *see* the change in ryll's window, short
  enough that walking 6 modes is still fast.

- **Mode switching during `run_booting`.** **Default: enabled,
  but the boot script does not advance until the switch
  completes.** The boot sequence is paced at 200 ms per line,
  so a mode switch between lines is fine — the boot script
  re-renders at the new dimensions and continues. Re-rendering
  the *whole* boot sequence so far is the safe default
  (Phase 1's redraw hook handles this); if that proves
  visually jarring, Phase 3 may switch `run_booting` to a
  policy of "buffer the keystroke and apply at the next scene
  boundary." Capture the decision in Phase 3's iteration.

- **Mode switching during the locked-bootloader scene.**
  **Default: disabled.** The scene's input loop owns keys 0–9
  for the paste buffer; remapping them to mode-switch would be
  semantic chaos. The mode keys are silently ignored inside
  the bootloader; on exit (correct paste / abort path), the
  awaiting / parked runners pick them up again. Document this
  in `README.md` and `ARCHITECTURE.md`.

- **On-screen toast vs serial-only feedback.** **Default:
  minimal on-screen toast.** A single row near the bottom of
  the screen for ~1.5 s reading e.g. `1024×768` (or the
  fallback form for substitutions). This is cheap given the
  per-glyph BltOp pattern already in use. The same information
  always goes to serial as a `ModeSwitch` event regardless.

- **Toast row collision with scene content.** **Default: pick
  a row that's reliably empty in awaiting / parked (the
  bottom row).** If the chosen row overlaps scene content,
  capture the exact row in the toast layout decision and
  redraw the row as part of toast-clear.

- **Should pressing the *same* mode twice be a no-op?**
  **Default: no — re-apply the mode and emit the event.**
  The point is to provide a re-trigger affordance for ryll
  testing (e.g. "the client missed a `SURFACE_CREATE`,
  let me poke it again"). Re-applying is cheap; suppressing
  it would cost test surface.

- **Key choice — overlap with the locked-bootloader's `0`.**
  Confirmed not a problem: the locked-bootloader scene has its
  own input loop and consumes its own keys (see previous
  question). In awaiting / parked, `0` is unused today.

- **Scancodes vs unicode.** **Default: match on unicode `'0'`
  through `'6'`** for symmetry with the bootloader's
  `ch.to_ascii_lowercase()` pattern. Scancodes only matter for
  non-printable keys. Scene `poll_key()` already returns both
  (`Option<(char, u16)>`) so we keep the option open.

- **`make qemu` (GTK) versus `make spice-ryll` for development.**
  **Default: develop and smoke-test under `make qemu`,
  acceptance-test against `make spice-ryll` with a checkout of
  ryll's `display-mode-ui` branch.** GTK reflects mode changes
  faithfully (it's just a framebuffer surface) and is faster to
  iterate. The whole point of the affordance is that ryll
  cares; the acceptance test must use ryll.

- **Should the affordance be feature-flagged (e.g. `--no-mode-keys`
  CLI flag) so it can be disabled for some scenes that may want
  the keys for their own purposes?** **Default: no flag.**
  Scenes that need 0–6 will own their input loop the way the
  locked-bootloader does, and will silently consume those keys
  during their lifetime. A global flag is over-engineering for
  a single binary with one operator at a time.

- **Should the actual applied mode be queried back from GOP
  after `set_mode`, or trusted from the requested `(width,
  height)`?** **Default: query back.** `gop.current_mode_info()`
  is the source of truth; trusting the request is a recipe for
  desync if OVMF substitutes silently. The `ModeSwitch` event
  carries the queried-back values.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. Renderer `set_mode` + redraw hook + ring-buffer events | PLAN-display-mode-keystrokes-phase-01-renderer.md | Not started |
| 2. Keystroke handlers in awaiting / booting / parked + cycle + on-screen toast | PLAN-display-mode-keystrokes-phase-02-keystrokes.md | Not started |
| 3. Iteration against ryll `display-mode-ui`, documentation, inventory closeout | PLAN-display-mode-keystrokes-phase-03-docs.md | Not started |

### Phase 1 sketch — Renderer `set_mode`, redraw hook, ring-buffer events

Foundation work that does not yet expose any user-facing
keystroke. End state: the binary still behaves identically
from the operator's seat, but internally everything needed to
switch modes at runtime exists and is tested (where testable).

Concretely:

- **`Renderer::set_mode(&mut self, width: usize, height: usize)`**
  in `src/renderer/mod.rs`. Re-walk `gop.modes()`, pick the
  exact match if available, otherwise the nearest by
  `(abs(dw) + abs(dh))` (deterministic tie-break: smaller
  width first, then smaller height). Call `gop.set_mode(&mode)`,
  query back the applied mode via `gop.current_mode_info()`,
  update `self.width` and `self.height` from that query.
  Return the actual `(width, height)` applied (not the
  request) so the caller can emit an event with truth.
- **One-time GOP mode dump to serial at boot.** A single
  `available GOP modes: 640x480, 800x600, 1024x768, ...`
  line written from `Renderer::new`, so the open question
  about OVMF's mode list closes itself the first time anyone
  runs the binary after this phase. Append the observed list
  back into this plan's *Open questions* before Phase 2 starts.
- **Redraw hook on the scene side.** Today, scene runners draw
  content "once per state transition" and don't keep enough
  state to repaint. We need a minimal mechanism whereby a
  runner can ask "redraw current state at current dimensions"
  after a mode switch. Concretely: a `Scene::repaint(&mut self,
  &mut Renderer)` method that each runner state implements,
  paired with a `redraw_after_mode_switch(&mut Renderer,
  &mut Scene)` helper that calls `clear() + draw_chrome() +
  scene.repaint(...)`. For `run_awaiting` this is "redraw the
  awaiting prompt + cursor"; for `run_parked` it is "redraw
  SYSTEM ONLINE"; for `run_booting` it is "redraw all
  already-played boot lines up to the current index."
- **Ring-buffer events** in `src/event.rs`:
  `ModeSwitch { requested_w: u32, requested_h: u32,
  applied_w: u32, applied_h: u32, timestamp_ms: u64 }`
  and `ModeCycle { count: u32, interrupted: bool,
  timestamp_ms: u64 }`. Both `Copy`, `Clone`, `Debug` for
  symmetry with existing variants. Update the plain-text drain
  formatter to emit human-readable lines.
- **Tests where practical.** `Renderer` is firmware-only, but
  the nearest-mode tie-break is a pure function — extract it
  into a free function (`nearest_mode((req_w, req_h), &[(w, h)])
  -> (w, h)`) that *can* live in `#[cfg(test)]`-able territory.
  This is the only host-testable seam in this phase.

End-of-phase exit: `make qemu` boots, plays the scene to the
parking screen, dumps the GOP mode list to serial, and behaves
exactly as it did before. `pre-commit run --all-files` clean.
A single `cargo test` for `nearest_mode`.

Recommend planning at **medium effort**: the renderer change is
mechanical, the redraw hook is the only design call (and it's
small), and the events follow the established pattern.

### Phase 2 sketch — keystroke handlers, cycle, on-screen toast

Wire up the user-facing affordance.

- **Mode-key dispatcher.** A `try_handle_mode_key(ch: char,
  scene: &mut Scene, renderer: &mut Renderer) -> bool`
  helper, called early in each scene runner's poll loop. If
  it returns `true`, the key was a mode key and was consumed;
  otherwise the runner handles the key as today. Maps `'1'`
  through `'6'` to the table above; `'0'` triggers the cycle.
- **Cycle.** Walks the actual GOP mode list (queried at cycle
  start so it is current), 1.0 s dwell per step, interruptible
  by any key (the interrupting key is *not* consumed — it
  falls through to the regular handler so e.g. pressing `3`
  during a cycle stops the walk *and* leaves the binary at
  1024×768). Emit a `ModeCycle` event at the end with
  `interrupted` set appropriately.
- **On-screen toast.** A bottom-row, ~1.5 s ephemeral text
  written through the existing per-glyph BltOp path. After the
  duration elapses the row is cleared (drawn over with the
  background colour) and the underlying scene content
  underneath the toast is restored via a localised `repaint`
  (the toast row is reliably empty in awaiting / parked; in
  booting, ensure the row chosen does not collide with played
  boot lines — if it does, redraw that row's boot content).
- **Locked-bootloader carve-out.** The bootloader's input loop
  in `src/bootloader.rs` does *not* call the dispatcher; its
  existing `R`/`I`/`A` and paste-capture handling is
  unchanged. Confirm with a smoke test that pressing `1`
  during the bootloader prompt does what it does today (i.e.
  is logged as a stray keypress and otherwise ignored).
- **Integration with the existing `Keypress` ring-buffer
  event.** Mode-key presses still emit `Keypress` events as
  the dispatcher itself sits inside `poll_key`'s consumer.
  The *additional* `ModeSwitch` event makes the semantic
  consequence machine-checkable separately from the raw
  keystroke.

End-of-phase exit: an operator running `make qemu` can press
`1`–`6` and `0` and watch the GTK window resize and the scene
repaint cleanly. `pre-commit run --all-files` clean.

Recommend planning at **medium effort**, with one **high-effort
opus** sub-step for the redraw-during-`run_booting` policy
decision (because it requires reasoning about visual
acceptability under live operator viewing and is the only
non-mechanical part of the phase).

### Phase 3 sketch — iteration against ryll, docs, inventory closeout

Operator-driven iteration against ryll's `display-mode-ui`
branch. Tune dwell time, toast position, fallback-substitution
wording. Validate all four edge cases from the *Mission*
section against ryll. Then:

- Update `README.md`: document the keystrokes in the operator
  controls section; note `make spice-ryll` is the canonical
  path for the ryll-tracking acceptance test.
- Update `DESIGN.md`: in the *Display channel* row of the
  channel mapping table, the existing `binary: phase 4`
  reference for *Mode set* stays; consider whether a
  parenthetical reference to this milestone belongs there or
  whether the inventory link below is enough.
- Update `ARCHITECTURE.md`: a paragraph on runtime mode
  switching alongside the existing scene state-machine
  description; mention the `Scene::repaint` mechanism.
- Update `AGENTS.md`: add a one-liner on the new affordance
  and the carve-outs (locked-bootloader does not see mode
  keys).
- Update `docs/spice-test-inventory.md`: change the
  *Mode walk across all offered modes* row's status to
  `binary: [display-mode-keystrokes](plans/PLAN-display-mode-keystrokes.md)`.
  Leave bit-depth and other rows as `—`.
- Update this file's *Execution* table and `docs/plans/index.md`
  to mark the master plan Complete (with commit ranges).
- Tick the *Success criteria* checklist below with notes on
  how each criterion was met.

Recommend planning at **medium effort**.

## Agent guidance

### Execution model

All implementation work is done by sub-agents, never in the
management session. The management session (this conversation)
is reserved for planning, review, and decision-making. This
keeps the management context lean and avoids drowning it in
implementation diffs.

The workflow is:

1. **Plan** at high effort in the management session.
2. **Spawn a sub-agent** for each implementation step with the
   brief from the plan, at the recommended effort level and
   model.
3. **Review** the sub-agent's output in the management session.
   Check the actual files — the sub-agent's summary describes
   what it intended, not necessarily what it did.
4. **Fix or retry** if the output is wrong. Diagnose whether
   the brief was insufficient (improve it) or the model was
   too light (upgrade it), then re-run.
5. **Commit** once the management session is satisfied with
   the result.

This applies to all steps, including high-effort ones. If a
sub-agent can't succeed even with a detailed brief and the
right model, that's a signal the brief needs improving, not
that the management session should do the implementation
itself.

Use `isolation: "worktree"` for sub-agents when the change is
risky or experimental. The renderer changes in Phase 1 and the
input dispatcher in Phase 2 are well-understood enough that a
direct-tree sub-agent is fine. Cycle-mode interaction with
existing scene runners is the most plausible candidate for a
worktree-isolated experiment if the first attempt looks off.

### Planning effort

Phase 1 plans at **medium**: small, mechanical, well-bounded.

Phase 2 plans at **medium**, with one **high-effort opus**
step for the `run_booting` redraw policy (visual judgement
plus per-glyph BltOp interaction).

Phase 3 plans at **medium**: documentation pass plus operator
iteration; opus is overkill.

The master plan itself was created at high effort — broad
codebase reading, cross-referencing the ryll-side fix, and
judgement calls on phase scoping.

### Step-level guidance

Each phase plan should include a table of the form:

```
| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | Add Renderer::set_mode and nearest_mode helper; cover nearest_mode with a unit test |
| 1b   | medium | sonnet | none      | Add ModeSwitch / ModeCycle ring-buffer events, plain-text formatter rows |
| 1c   | high   | opus   | worktree  | Design Scene::repaint + redraw-after-mode-switch; requires reasoning about each runner's state |
```

**Effort levels:**

- **high** — Requires reading multiple files, making judgement
  calls, understanding non-obvious invariants (UEFI memory
  layout, firmware quirks, SPICE channel semantics), or
  researching external references.
- **medium** — The plan provides enough context that the
  sub-agent can follow a clear brief. May need to read a few
  files but the approach is well-defined.
- **low** — Purely mechanical (rename, reformat, add a log
  line). The brief is a complete instruction.

**Model choice.** Default to opus for Phase 1c (redraw hook
design) and any cycle-mode visual-policy step in Phase 2;
sonnet for everything else. Skew to the more capable model
when in doubt.

**Brief for sub-agent.** Include: what to change, which files
to touch, what patterns to follow, and any non-obvious
constraints. The locked-bootloader scene's input carve-out
is one such constraint — sub-agents touching the keystroke
dispatcher in Phase 2 must not break it.

### Management session review checklist

After a sub-agent completes, the management session should
verify:

- [ ] The files that were supposed to change actually changed
      (read them, don't trust the summary).
- [ ] No unrelated files were modified.
- [ ] The code builds (`pre-commit run --all-files`).
- [ ] Host-side `cargo test` still passes; the new `nearest_mode`
      unit test is present and passes.
- [ ] The changes match the intent of the brief — semantically
      right, not just syntactically correct.
- [ ] Commit message follows project conventions including
      `Signed-off-by`, the `Co-Authored-By` line with model,
      context window, effort level, and any other settings,
      and the `Prompt:` paragraph capturing intent.

### Phase-specific emphases

- **Mode switching is a *test affordance*, not a scene
  mechanic.** Resist any temptation to weave it into game
  content (e.g. "the instrument loses focus, switches mode by
  itself"). Future plans can do that; this one is plumbing.
- **Ground truth from `current_mode_info()`, not from the
  request.** The `ModeSwitch` event must carry the *applied*
  resolution. Otherwise ryll-side assertions become
  best-effort.
- **Per-glyph rendering still binds.** The on-screen toast
  goes through the same per-glyph BltOp path the rest of the
  binary uses (principle 6 of the project's rendering
  philosophy). No shortcuts.
- **Locked-bootloader carve-out is load-bearing.** Confirm via
  smoke test in Phase 2 — pressing `1` at the R/I/A prompt
  must continue to be logged-and-ignored, not interpreted as
  640×480.

## Administration and logistics

### Success criteria

This milestone is complete when:

- [ ] `Renderer::set_mode(width, height)` exists, accepts a
      requested mode, applies the nearest available, queries
      back the actual applied mode, and returns it.
- [ ] A one-time `available GOP modes: ...` line is emitted to
      serial at boot, and the observed list under default
      QEMU + OVMF has been recorded inline in this plan's
      *Open questions* section.
- [ ] `Scene::repaint` (or equivalent) exists and lets each
      scene runner redraw its current state at the current
      Renderer dimensions.
- [ ] Ring-buffer events `ModeSwitch` and `ModeCycle` are
      defined, emitted at the right moments, and rendered as
      plain-text serial lines.
- [ ] An operator running `make qemu` can press `1`–`6` and
      `0` from the awaiting and parked scenes and observe
      the GTK window resize plus the scene repaint cleanly.
- [ ] An operator running `make spice-ryll` (with a release
      build of ryll's `display-mode-ui` branch supplied via
      `RYLL=`) can press `1`–`6` and `0` and observe ryll's
      window track each new resolution.
- [ ] With ryll's `Obey guest size hints` toggled off, the
      binary's mode change is visible inside ryll's pinned
      window without ryll re-fitting; toggling back on causes
      the *next* mode change to re-fit. (i.e. the affordance
      genuinely exercises both branches of the ryll feature.)
- [ ] Pressing a mode key during the locked-bootloader scene
      is logged as a stray keypress and otherwise ignored —
      the bootloader's R/I/A and paste-capture flow are
      unchanged.
- [ ] The on-screen toast appears, names the *applied*
      resolution (and the *requested* one if the two
      differ), and clears after ~1.5 s without leaving
      stale pixels under it.
- [ ] Cycle mode (`0`) is interruptible: any keypress during
      the cycle stops the walk; if the interrupting key is
      itself a mode key, it is honoured (stop + apply the
      new request).
- [ ] `make qemu`, `make spice`, `make spice-ryll`,
      `make release-verify`, and `make screenshot` continue
      to work.
- [ ] `pre-commit run --all-files` exits 0.
- [ ] Host-side `cargo test` passes, including a
      `nearest_mode` unit test.
- [ ] `README.md`, `ARCHITECTURE.md`, `AGENTS.md`, and
      `docs/spice-test-inventory.md` reflect the new
      affordance and the locked-bootloader carve-out.
- [ ] `docs/spice-test-inventory.md`'s *Mode walk across all
      offered modes* row carries a `binary:` link to this
      plan.
- [ ] `docs/plans/index.md` and `docs/plans/order.yml` have
      been updated for this master plan; per-phase plan
      files are linked from the *Execution* table.

### Future work

Items deliberately deferred out of this milestone:

- **Bit-depth keystrokes.** GOP under OVMF is effectively
  32 bpp BGR; pursuing 8/16 bpp would be a separate piece of
  work and is the *Bit-depth changes* row in the inventory.
- **Multi-head simulation.** Hot-add / hot-remove monitor and
  per-monitor mode are inventory rows in their own right and
  are out of scope.
- **Scene-driven mode changes.** A scene that *narratively*
  changes resolution as part of its content (e.g. "the
  instrument blinks; for half a second it perceives in
  640×480"). Reuses the same `Renderer::set_mode` plumbing
  but is content, not infrastructure.
- **Ryll-driven mode requests.** Once gRPC-over-serial lands
  (per `instar`), ryll itself can request mode changes and
  assert on the resulting `ModeSwitch` events without operator
  keystrokes. Until then, the keystrokes are how a human
  drives the test.
- **Damaged / partial frame recovery, MJPEG / H.264 streams,
  bandwidth degradation.** All inventory rows on the *Display
  channel* whose `binary:` status remains `—`. They are
  bigger, separate milestones.
- **CI screenshot of mode-switch behaviour.** `make screenshot`
  currently captures one frame; a sequence-screenshot harness
  that drives the keystrokes via QMP and captures one frame
  per mode would be a nice CI-recordable artefact, but is not
  required for the affordance to be useful and is its own
  small piece of work.

### Bugs fixed during this work

(None yet — populated during execution.)

### Documentation index maintenance

On creation of this plan, `docs/plans/index.md` and
`docs/plans/order.yml` get a new master-plan entry. As phases
complete, update the status column in the Execution table
above and in `index.md`.

When all phases complete, update `index.md`'s status column
to *Complete* with the commit range, and update this file's
*Execution* table likewise.

### Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the
work you intend to do aligns with that plan.
