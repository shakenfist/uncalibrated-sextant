# Audit cleanup — bug fixes, structural dedup, and headless coverage gaps

## Prompt

Before responding to questions or discussion points in this
document, read the four sub-agent reports that produced the
findings below. Those reports were generated as part of running
`PUSH-AUDIT.md`'s wave 1 + wave 2 audit on the current state
of `main` (commit `8255685`, the closeout of the
display-mode-keystrokes milestone) and are summarised in this
plan rather than reproduced verbatim. The agents' reports
themselves are not committed — they live in the management
session that produced this plan, and the audit is reproducible
by re-running `PUSH-AUDIT.md` against `main`.

Also read:

- [`PUSH-AUDIT.md`](../../PUSH-AUDIT.md) — the audit
  template that this plan responds to. Wave 1 mechanical was
  run by hand because `tools/audit/wave1.sh` does not yet
  exist; that script is itself a possible future-work item but
  is out of scope for this plan.
- [`AGENTS.md`](../../AGENTS.md) — *Most recently landed*
  section, *Design principles to respect*, and the project
  conventions that govern how cleanup work is committed.
- [`ARCHITECTURE.md`](../../ARCHITECTURE.md) — particularly
  the renderer description and the runtime mode-switching
  paragraph added in the previous milestone.
- The two source files most affected by this plan:
  [`src/scene.rs`](../../src/scene.rs) (878 lines) and
  [`src/bootloader.rs`](../../src/bootloader.rs) (693 lines).
  Plus [`src/renderer/mod.rs`](../../src/renderer/mod.rs)
  for the bounds-check fix.

Where a question touches `uefi-rs` semantics (especially GOP's
`set_mode` failure behaviour and `current_mode_info`'s post-
failure guarantees), research as needed and flag any residual
uncertainty explicitly.

This plan is the first **cross-cutting cleanup plan** in the
project — previous plans have all targeted single milestones.
The shape borrows from `PLAN-TEMPLATE.md` but several sections
(*Mission*, *Open questions*) are scoped tighter because the
work is reactive (audit findings) rather than designed.

All planning documents live in `docs/plans/`. Phase plans will
be separate files named `PLAN-audit-cleanup-phase-NN-...md`
and tracked in the *Execution* table below.

One commit per logical change as before. Each commit should
build, pass `pre-commit run --all-files`, and have a clear
message in the project's existing style.

## Situation

A pre-push audit run against `main` (post-display-mode-
keystrokes-closeout, commit `8255685`) found three real bugs,
six structural cleanup opportunities, and two test-coverage
gaps. None block the push of any individual milestone, but
collectively they represent the cleanup that a pre-push audit
discipline would have surfaced earlier. The operator chose to
batch them into a dedicated cleanup plan rather than fold
fixes into the next feature milestone.

The audit's headline observations:

- Wave 1 mechanical (pre-commit, build, smoke) is clean.
- Wave 2 mechanical inventory found zero `unsafe` blocks,
  zero `#[allow(dead_code)]`, zero TODO/FIXME, and only
  two `.unwrap()`/`.expect()` calls — both at boot in
  `main.rs` where panic-on-init-failure is appropriate.
- The four judgement agents (code quality, tests, docs,
  security) produced a punch list that decomposes into the
  *Mission* below.

The operator has additionally decided to move to PR-based
workflows from this point on, with `PUSH-AUDIT.md` as a
PR-gating audit rather than personal discipline. That decision
is independent of this plan but motivates it: the PR
mechanism makes the audit's findings unavoidable on every
future push, so cleaning up the existing backlog is
worthwhile before the new discipline takes hold.

## Mission and problem statement

Address the audit's findings in three phases:

1. **Real bugs.** Three concrete defects: a missing bounds
   check in the renderer's draw helpers (silently
   off-screen blits at small modes during paste echo), a
   README inaccuracy where toast format examples use
   Unicode `×`/`→` that the ASCII-only renderer cannot
   produce, and a stale `Renderer::draw_logo` reference in
   ARCHITECTURE.md (the method was renamed to
   `draw_text_bitmap` earlier and the doc was not
   updated).

2. **Structural dedup and cleanup.** Six items the audit
   surfaced as advisory but worth doing now to make the
   next scene cheaper to add: a `PACE_LINE_MS`
   single-source-of-truth (currently re-declared in two
   files with a comment saying "kept in sync"), a
   `blink_until_key` helper extracting the near-identical
   blink loops in `run_awaiting` and `run_parked`, a
   `blit_glyph_bytes` helper extracting the shared
   BltPixel composition in `draw_glyph` and
   `draw_cursor_glyph`, plus three small defensive items
   (a `debug_assert` on the bootloader's countdown digit
   width, a visibility tightening on
   `try_handle_mode_key` and `cycle_modes`, and a
   doc-comment on `RepaintState` documenting the "one
   variant per scene runner" intent).

3. **Test coverage and release verification.** Two narrow
   gaps the audit identified: `make screenshot-modes` to
   give the display-mode-keystroke dispatcher headless
   coverage (currently zero), and a `verify-release.sh`
   addition to also grep for the `available GOP modes:`
   line in release artefacts.

By the end of this plan:

- Pressing `1` (640×480) then `i` (bootloader paste) and
  pasting a 64-character string echoes correctly inside
  the smaller framebuffer (or returns silently, but
  predictably). No more silent off-screen BltOps.
- README and ARCHITECTURE accurately describe what the
  binary does.
- Adding a fourth scene runner does not require copying a
  blink loop a third time.
- `make screenshot-modes` exists and asserts a
  `type=mode_switch` line in serial after a synthetic
  mode-key press.
- `verify-release.sh` confirms both the startup banner and
  the GOP-mode dump in the release artefacts.

Items the audit identified as worth deferring are listed
under *Future work* with rationale.

## Open questions

Defaults below are strong but worth confirming or iterating at
the relevant phase. Capture changes inline rather than letting
them drift.

- **Renderer bounds-check behaviour: silent return vs
  ring-buffer event for diagnostics.** **Default: silent
  return.** Adding an event variant for "draw skipped, out
  of bounds" couples the renderer to the event system and
  introduces noise in the drain. The bounds violation is a
  programming error, not an operator-driven event;
  `debug_assert!` is the right gate. **If a future
  milestone needs to detect "bootloader echo went off
  screen" specifically, that's a separate state and
  shouldn't be wedged into the renderer.**

- **`PACE_LINE_MS` location: re-export vs new
  `src/constants.rs`.** **Default: re-export
  `pub(crate)` from `src/scene.rs`.** A new
  `constants.rs` module is over-engineering for one
  shared value. If a third sharing user appears, revisit.

- **`blink_until_key` signature.** **Default: a method on
  `Scene` taking `(cursor_col, cursor_row, on_exit:
  impl FnOnce(&mut Self) -> Phase)` or similar.** The exit
  closure is the only thing that genuinely differs between
  `run_awaiting` (transitions to Booting) and `run_parked`
  (transitions to Parked-final). The closure signature is
  the design call; pick what reads cleanest. Avoid
  trait-object dispatch (`dyn FnOnce`) because of the
  `no_std` allocation cost — accepting `FnOnce` by generic
  parameter is cleaner.

- **`blit_glyph_bytes` visibility.** **Default: private
  helper inside `Renderer`'s impl block.** No external
  callers; reduces the public surface.

- **`make screenshot-modes` artefact: separate
  serial log file, or share `dist/screenshot-serial.log`.**
  **Default: separate file
  (`dist/screenshot-modes-serial.log`).** Keeps the
  existing screenshot regression check clean.

- **`make screenshot-modes` should also produce a
  reference frame?** **Default: no.** The mode-switch
  toast is ephemeral (~1.5 s) and capturing a stable
  frame requires precise timing the QMP path doesn't
  guarantee. The serial assertion is the test value;
  the visual is operator-only.

- **Should this plan include the strict 59-event
  assertion in `screenshot.sh`?** **Default: no.** The
  audit's recommendation was either-or; the maintenance
  cost (forcing every milestone to bump the constant) is
  real, and a structural change like a new event type
  will be obvious to a human reviewer. A `>= 50` lower
  bound check would be defensible; capture as future
  work if the operator wants it.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. Real bug fixes | [PLAN-audit-cleanup-phase-01-bugs.md](PLAN-audit-cleanup-phase-01-bugs.md) | Complete (commits a42af74, 5fd347c, 56b8028, plus this closeout) |
| 2. Structural dedup | [PLAN-audit-cleanup-phase-02-structural.md](PLAN-audit-cleanup-phase-02-structural.md) | Complete (commits ee8ddd1, c5d84ca, 46baa27, e0734e1, 64a0297, 118c17f, plus this closeout) |
| 3. Test coverage and release verification | [PLAN-audit-cleanup-phase-03-tests.md](PLAN-audit-cleanup-phase-03-tests.md) | Complete (commits d862ad3, 96720ab, plus this closeout) |

### Phase 1 sketch — real bug fixes

Three commits, all small:

- **1a — Renderer bounds-check.** Add the early-return
  guard `if col >= self.screen_cols() || row >=
  self.screen_rows() { return; }` to `Renderer::draw_glyph`,
  `Renderer::draw_cursor_glyph`, and `Renderer::clear_cell`.
  Smoke-test: confirm `make screenshot` still produces the
  same 59-event transcript (the bounds are not exceeded in
  the standard run). Optionally add a smoke check that
  presses `1` (640×480) → `i` (bootloader) → paste a
  long-enough-to-overflow string under `make qemu` and
  confirms the binary completes the bootloader flow without
  hanging.

- **1b — README toast format.** Replace `1024×768` with
  `mode 1024x768` and `requested 1280×720 → using 1024×768`
  with `requested 1280x720 -> using 1024x768` in
  `README.md` around lines 152–156. Also clarify in the
  surrounding prose that the renderer is ASCII-only so the
  examples render exactly as shown.

- **1c — ARCHITECTURE stale name.** Replace
  `Renderer::draw_logo` with `Renderer::draw_text_bitmap`
  in `ARCHITECTURE.md:55`. Confirm by grep that no other
  occurrences of `draw_logo` exist anywhere in the repo.

End-of-phase exit: pre-commit clean, `make screenshot`
unchanged, README and ARCHITECTURE accurate. **Recommend
planning at low effort (sonnet); no design judgement
required.**

### Phase 2 sketch — structural dedup

Six commits, mostly small:

- **2a — `PACE_LINE_MS` single source.** Re-export
  `pub(crate) const PACE_LINE_MS` from `src/scene.rs` and
  remove the duplicate declaration in `src/bootloader.rs`.
  Update bootloader's `use` line to import it from
  `crate::scene`. The "kept in sync with scene.rs" comment
  goes away.

- **2b — Extract `blit_glyph_bytes` helper.** Add a private
  `Renderer::blit_glyph_bytes(&mut self, bytes: &[u8; 16],
  col: usize, row: usize)` that does the BltPixel
  composition shared by `draw_glyph` and `draw_cursor_glyph`.
  The two callers become one-liners around the index lookup
  / direct-byte-pass. ~20 lines saved.

- **2c — Extract `blink_until_key` helper.** A method on
  `Scene` that takes the cursor position and the exit
  transition (as a closure or as an explicit `Phase` to
  transition to) and runs the blink-poll loop. `run_awaiting`
  and `run_parked` become two-line stubs. **This is the only
  step in this phase with a real design call** — see *Open
  questions* on the closure signature.

- **2d — Defensive `debug_assert!` on countdown digit
  width.** In `bootloader::run_timeout`, add
  `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99);` at the top.
  One line.

- **2e — Tighten visibility on `try_handle_mode_key` and
  `cycle_modes`.** Both are `pub(crate)` in `src/scene.rs`
  but only used within the same file. Change to private
  (`fn` not `pub(crate) fn`). Updates any internal
  references.

- **2f — Document `RepaintState` intent.** Add a doc
  comment on the enum explaining "one variant per scene
  runner; keep variant fields minimal — just enough state
  to replay the visible content." ~3 lines.

End-of-phase exit: `make screenshot` produces the same
59-event transcript byte-identically; `make qemu` runs the
mode keys cleanly; pre-commit clean. **Recommend planning
at medium effort (sonnet) — 2c is the design step but the
others are mechanical.**

### Phase 3 sketch — test coverage and release verification

Three commits:

- **3a — `make screenshot-modes` target.** New script
  `scripts/screenshot-modes.sh` (modelled on
  `scripts/screenshot.sh`) that drives the binary
  headless, presses `'3'` once after parking (or after
  the bootloader completes — pick the simpler path), and
  asserts that `dist/screenshot-modes-serial.log` contains
  exactly one `type=mode_switch` line with
  `requested=1024x768 applied=1024x768`. Adds the target
  to `Makefile`.

- **3b — `verify-release.sh` GOP-mode grep.** Add a one-
  line check after the existing banner grep that confirms
  `available GOP modes:` appears in the release artefact's
  serial log.

- **3c — Closeout.** Mark Phase 3 Complete in the master
  plan's *Execution* table; tick *Success criteria*;
  update `docs/plans/index.md` master-plans row to
  *Complete*. Phase 3 is light enough that 3a + 3b's
  commits can absorb the closeout if there is no
  iteration needed.

End-of-phase exit: `make screenshot` and `make
screenshot-modes` both pass; `make release-verify`
confirms both the banner and the mode dump; the new
script is reproducible. **Recommend planning at medium
effort (sonnet) — script work is mechanical but the
QMP send-key timing for the mode-key press needs care.**

## Agent guidance

### Execution model

Same as previous plans: all implementation work goes to
sub-agents, the management session reviews and commits.
None of this work needs opus or worktree isolation — the
phases are mechanical or near-mechanical.

The dedicated worktree pattern from earlier milestones is
unnecessary here; direct-tree sub-agents are fine.

### Planning effort

Phase 1: low (sonnet) — three small mechanical edits.
Phase 2: medium (sonnet) — one design call (2c's
closure signature), the rest mechanical.
Phase 3: medium (sonnet) — script work, QMP timing care.

The master plan itself was created at low-to-medium
effort because the audit's findings were already
distilled by the four judgement agents; this plan is
mostly a reorganisation of their outputs into a
phased shape.

### Step-level guidance

Each phase plan should include a step table of the same
shape used in earlier milestones:

```
| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | low    | sonnet | none      | ... |
```

### Management session review checklist

Same as earlier milestones:

- [ ] Files that were supposed to change actually changed.
- [ ] No unrelated files modified.
- [ ] `pre-commit run --all-files` clean.
- [ ] Code matches intent of brief.
- [ ] Commit message follows project template.

## Administration and logistics

### Success criteria

This cleanup is complete when:

- [x] Three real bug fixes landed: renderer bounds-check,
      README toast format, ARCHITECTURE stale name.
      *(Phase 1, commits a42af74 / 5fd347c / 56b8028.)*
- [x] `PACE_LINE_MS` declared in exactly one location;
      bootloader imports it. *(Phase 2, commit ee8ddd1.)*
- [x] `Renderer::draw_glyph` and
      `Renderer::draw_cursor_glyph` share a private
      `blit_glyph_bytes` helper. *(Phase 2, commit
      c5d84ca.)*
- [x] `Scene::run_awaiting` and `Scene::run_parked`
      share a `blink_until_key` helper; both runners
      become a two-line stub plus the helper. *(Phase 2,
      commit 118c17f.)*
- [x] `bootloader::run_timeout` carries a
      `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99)`.
      *(Phase 2, commit 46baa27.)*
- [x] `Scene::try_handle_mode_key` and
      `Scene::cycle_modes` are private (not
      `pub(crate)`). *(Phase 2, commit e0734e1.)*
- [x] `RepaintState` carries a doc comment documenting
      the per-scene-runner intent. *(Phase 2, commit
      64a0297.)*
- [x] `make screenshot-modes` exists, runs headless, and
      asserts a `type=mode_switch` line. *(Phase 3,
      commit d862ad3.)*
- [x] `scripts/verify-release.sh` greps for both the
      startup banner *and* the `available GOP modes:`
      line. *(Phase 3, commit 96720ab.)*
- [x] `make screenshot` produces the same 59-event
      transcript as the pre-cleanup baseline (no
      regression in the no-keystroke path). *(verified
      after every commit.)*
- [x] `make qemu`, `make spice`, `make spice-ryll`,
      `make release-verify` all continue to work.
      *(verified at Phase 3 step 3b — `make
      release-verify` PASS for both raw + qcow2 with the
      new combined check.)*
- [x] `pre-commit run --all-files` exits 0 at every
      commit across the plan.
- [x] Commit messages follow the project's template.
- [x] `docs/plans/index.md` master-plans row updated to
      *Complete* with the commit range. *(this closeout.)*

### Future work

Items the audit identified as worth tracking but not
addressing in this plan:

- **`scene.rs` and `bootloader.rs` module splits.**
  Both are ~700–900 lines and sub-section cleanly. Defer
  until adding a fourth scene runner (likely an
  inventory-row implementation in a future milestone).
  The split should be `src/scene/runners.rs`,
  `src/scene/repaint.rs`, `src/scene/mode_keys.rs` (or
  similar) for scene; bootloader is already a single
  function-family and the split would be by concern
  (paste capture, R/I/A prompt, countdown).

- **Workspace split for host-side `cargo test`.** Phase 1
  of display-mode-keystrokes documented this deferral
  and the rationale stands. Re-evaluate once a second or
  third pure function (`format_u32`, `RingBuffer`,
  `CursorState::tick`) accumulates behind the same
  workspace boundary. The estimated 1–2 hour cost
  becomes worthwhile when the test value is
  multi-function rather than single-function.

- **`format_u32` to a shared `src/util.rs`.** Currently
  local to `src/bootloader.rs`. Move to a util module
  when a second user appears.

- **Polling-stall pattern unification across
  `Scene::stall_with_keys`, the bootloader R/I/A loop,
  and the bootloader paste-capture loop.** All three do
  `stall(POLL_MS)` + `poll_key()` + `Event::Keypress`
  push, but the per-loop key filter (mode-key dispatch
  vs R/I/A vs ASCII paste) makes a closure-based
  abstraction non-trivial in `no_std`. Worth revisiting
  if a fourth polling-stall consumer appears, or if the
  duplication grows past three sites.

- **`tools/audit/wave1.sh` and
  `tools/audit/wave2-mechanical.sh`.** `PUSH-AUDIT.md`
  references these scripts but they don't exist yet for
  this repo. Once we move to PRs (a separate operator
  decision, not part of this plan), creating the scripts
  becomes worthwhile so the PR template can run them as
  CI gates. Adapt from `shakenfist/ryll/tools/audit/`
  per the template's note.

- **Strict event-count assertion in `screenshot.sh`.**
  The audit recommended either-or: assert `== 59` (catches
  regressions but forces every milestone to update the
  constant) or document drift acceptance. Currently
  documented-as-drift; the strict assertion is captured
  here for the future operator who decides the
  maintenance cost is worth the regression coverage.

- **Headless cycle-mode test.** Driving a 30-second cycle
  walk under headless QMP is impractical. Worth noting
  that if `CYCLE_DWELL_MS` is ever made
  build-flag-configurable, a 50ms-per-step variant could
  be tested headlessly.

- **`Renderer::set_mode` failure-mode documentation.**
  Add a comment block to `Renderer::set_mode` documenting
  the dependency on UEFI 2.10 §12.9 — that
  `current_mode_info` reflects the active mode after
  `set_mode` succeeds *or fails*, so the query-back
  pattern produces ground truth either way. Defensive
  documentation; not a code change.

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
confirm the renderer bounds-check is silent (no event /
no log) and that the closure-based `blink_until_key`
abstraction does not introduce a `Box<dyn FnOnce>`
allocation.
