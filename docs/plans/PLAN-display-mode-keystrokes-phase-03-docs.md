# Display-mode keystrokes — phase 3: docs and inventory closeout

Parent plan:
[PLAN-display-mode-keystrokes.md](PLAN-display-mode-keystrokes.md).
Previous phases:
[Phase 1](PLAN-display-mode-keystrokes-phase-01-renderer.md),
[Phase 2](PLAN-display-mode-keystrokes-phase-02-keystrokes.md).

## Outcome

**Status: Complete (commits 1c06122, d29187d, plus this
closeout).**

The display-mode-keystrokes milestone is closed. Phase 3
captured the qxl mode list (Phase 1 deferral) and confirmed
it is byte-for-byte identical to the std list, fanned out the
documentation across `README.md`, `ARCHITECTURE.md`, and
`AGENTS.md` with consistent locked-bootloader carve-out
language, marked the *Mode walk* row in the SPICE test
inventory with a `binary:` link, and ticked the master plan's
*Success criteria* checklist with one struck-through deferred
item (the host-side `cargo test` for `nearest_mode`).

### What Phase 3 actually delivered

- Headless qxl mode-list capture under the same OVMF + q35
  args as `scripts/spice.sh`, recorded in the master plan's
  *Open questions* alongside the std list. Both lists are the
  same 30 modes; only enumeration order differs. All six
  binding keys present in both. (commit `1c06122`)
- `README.md` gained a new `### Display-mode keystrokes`
  subsection with the binding table, toast format, bootloader
  carve-out, and `make spice-ryll` acceptance-test recipe.
  `## Status` updated to name the milestone landed.
  `ARCHITECTURE.md` gained a paragraph on runtime mode
  switching naming `Renderer::set_mode`, `Scene::repaint`,
  `RepaintState`, and the carve-out. `AGENTS.md` had its
  `## Current phase` heading renamed to `## Most recently
  landed` and its body refreshed with a prominent
  LOAD-BEARING CARVE-OUT paragraph. (commit `d29187d`)
- `docs/spice-test-inventory.md`'s *Mode walk across all
  offered modes* row's status is now
  `binary: [display-mode-keystrokes](plans/PLAN-display-mode-keystrokes.md)`.
  (this closeout)
- Master plan's *Execution* table marks Phase 3 Complete;
  *Success criteria* checklist ticked with per-item commit
  references; the host-side `cargo test` line is struck
  through pointing at Phase 1's deferral rationale.
  `docs/plans/index.md` *Master plans* row updated to
  *Complete*. (this closeout)

### What Phase 3 did NOT deliver, and why

- **`DESIGN.md` updates.** Intentionally excluded: the SPICE
  channel mapping table there is high-level and does not
  carry per-row binary-implementation status — that lives in
  the inventory. The phase plan flagged this so a future
  reviewer doesn't chase the gap.
- **Code changes.** None expected; none made. `git diff
  1c06122..HEAD -- src/` is empty for this phase.

## Prompt

Before working on this phase, re-read:

- The master plan's *Phase 3 sketch* and *Success criteria*
  sections — they enumerate the doc files and inventory rows
  that need updating.
- The Phase 1 *Outcome* section — it documents one item
  (`-vga qxl` mode-list capture) carried over to Phase 3.
- The Phase 2 *Outcome* section — closes Phase 2's scope and
  hands off to this phase exactly the four doc files plus
  the inventory table to update.
- `PLAN-locked-bootloader-phase-03-docs.md` — the most
  recent doc-fan-out closeout in this project. Its *Outcome*
  section is the right shape to copy. Its commit messages
  set the tone (one commit per logical doc surface, terse
  prompts, plain language).
- `README.md`'s `## Building and running` section (around
  lines 53–138) and the `## What it looks like` section
  (line 30) — the two natural homes for a description of
  the new keystroke affordance.
- `ARCHITECTURE.md` — the section discussing the renderer
  (around line 30) and the scene state machine. The new
  paragraph on runtime mode switching lands alongside the
  existing scene-machine description.
- `AGENTS.md`'s `## Current phase` section (around lines
  97–121) — currently describes locked-bootloader phase 3.
  Update it to describe the display-mode-keystrokes
  milestone as the most recently completed.
- `docs/spice-test-inventory.md`'s *Display channel* table
  (around lines 64–80) — *Mode walk across all offered
  modes* row currently has status `—` and is the natural
  home for this milestone's `binary:` link.
- `docs/plans/index.md` and `docs/plans/order.yml` — the
  *Master plans* row for display-mode keystrokes goes from
  *Not started* → *Complete*.

`DESIGN.md`'s SPICE channel mapping table (around lines
76–87) is *high-level* and does not carry per-row binary-
implementation status — that is the inventory's job.
Phase 3 does **not** modify `DESIGN.md`. The master plan's
*Phase 3 sketch* asked us to consider whether a parenthetical
reference belongs there; the answer after looking at the
file is no. Documented here so a future reviewer doesn't
chase the gap.

This phase plans at **low-to-medium effort** overall. Every
step is mechanical doc fan-out plus one short serial-log
capture under `make spice-ryll`. No opus required.

## Goal

Make the work reachable from the project's own documentation,
mark this milestone Complete in the planning index, and close
the one open question carried over from Phase 1.

By the end of this phase:

- `README.md` documents the keystrokes (`'1'`–`'6'` and `'0'`)
  in the operator-controls section, with `make spice-ryll`
  named as the canonical path for ryll-tracking acceptance.
  The locked-bootloader carve-out is mentioned: mode keys
  are silently ignored inside the R/I/A and paste-prompt
  loops.
- `ARCHITECTURE.md` carries a paragraph on runtime mode
  switching alongside the existing scene-state-machine
  description, naming `Renderer::set_mode`,
  `Scene::repaint`, and the per-phase `RepaintState`
  snapshot mechanism.
- `AGENTS.md`'s *Current phase* section names this
  milestone as the most recent completion, with the
  carve-out called out for sub-agents touching scene code.
- `docs/spice-test-inventory.md`'s *Mode walk across all
  offered modes* row's status is
  `binary: [display-mode-keystrokes](plans/PLAN-display-mode-keystrokes.md)`.
  The *Mode set* row's existing `binary: phase 4` link
  stays as-is. The other display-channel rows stay `—`.
- The qxl mode list under `make spice-ryll` is captured and
  recorded in the master plan's *Open questions* alongside
  the std list. Any divergence is flagged for future
  awareness; if a binding key now maps to a fallback
  substitution under qxl, document the substitution.
- The master plan's *Execution* table marks Phase 3
  Complete with the commit range, and *Success criteria*
  checklist is ticked with per-item notes (with the
  `cargo test` line struck through and pointing at Phase
  1's host-test-deferral rationale).
- `docs/plans/index.md` marks the master plan *Complete*
  with the full commit range.

## Scope

**In scope:**

- `-vga qxl` mode-list capture under `make spice-ryll` plus
  the master-plan *Open questions* edit.
- `README.md` operator-controls section: keystroke table,
  toast description, the `make spice-ryll` acceptance-test
  recipe, the bootloader carve-out note.
- `ARCHITECTURE.md` paragraph on runtime mode switching.
- `AGENTS.md` *Current phase* refresh.
- `docs/spice-test-inventory.md` *Mode walk* row status
  update.
- `docs/plans/index.md` + this phase plan's *Outcome*
  section + the master plan's *Execution* table marked
  Complete.
- The master plan's *Success criteria* checklist ticked
  with per-item verification notes; the `cargo test` line
  struck through with the Phase-1 deferral rationale.

**Out of scope:**

- `DESIGN.md` (high-level table; per-row binary status
  belongs in the inventory).
- Code changes. If a tuning iteration during 3a flushes out
  a small bug, fix it in a separate commit with a clear
  message rather than folding it in here. None expected.
- New features, scenes, or test affordances. Future work
  per the master plan.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 3a   | medium | sonnet | none      | Capture the `-vga qxl` mode list under `make spice-ryll`, paste back into the master plan's *Open questions* alongside the std list. Flag any binding keys whose qxl resolution falls back to a substitution. |
| 3b   | medium | sonnet | none      | Documentation fan-out: `README.md` (operator controls + carve-out + acceptance test), `ARCHITECTURE.md` (runtime mode switching paragraph), `AGENTS.md` (Current phase refresh). One commit per file is fine; one bundled commit is also fine — pick whichever produces a cleaner review. |
| 3c   | low    | sonnet | none      | Inventory + master-plan closeout: `docs/spice-test-inventory.md` *Mode walk* row gets the `binary:` link; master plan's *Execution* table marked Phase 3 Complete and *Success criteria* checklist ticked (with the `cargo test` line struck through pointing at Phase 1's deferral); `docs/plans/index.md` *Master plans* row updated to *Complete*. |

Three commits expected, one per step. Step 3b may produce
multiple commits if the doc fan-out is bundled by file —
acceptable; the management session reviews and merges.

## Detailed step briefs

### 3a — qxl mode-list capture

**Files:** `docs/plans/PLAN-display-mode-keystrokes.md`
(*Open questions* section).

**What to do:**

1. Build ryll's `display-mode-ui` branch if it is not already
   built. The master plan's *Mission*-section recipe is the
   canonical incantation:

   ```sh
   cd ../ryll-wt-display-mode-ui
   make release
   cd -
   ```

   (Adjust the path if the worktree name differs locally.)

2. Run `make spice-ryll` with the `RYLL` environment override
   pointing at the built binary. Let the binary reach the
   awaiting screen, press a key to advance to booting, let
   it park.

   ```sh
   RYLL=/path/to/ryll-wt-display-mode-ui/target/release/ryll \
       make spice-ryll
   ```

3. Capture the `available GOP modes:` line from
   `dist/serial.log` (or wherever `scripts/spice-ryll.sh`
   redirects serial — read the script first if unsure).

4. Compare against the std list already recorded in the
   master plan's *Open questions* (the 30-mode list under
   commit `f1acc97`). Note any difference.

5. Edit the master plan's *Open questions* entry for
   *"Which GOP modes does OVMF actually expose under
   QEMU?"* to add the qxl observation. The format follows
   the existing std-list block:

   ```text
   Observed under `make spice-ryll` (OVMF + qemu-system-x86_64
   with `-vga qxl`), commit <sha>:

     available GOP modes: …
   ```

6. **If the lists diverge** in a way that affects the
   binding table: add a note explaining which key now maps
   to a substitution under qxl, and what the substitution
   resolves to. Operator already smoke-tested all six
   bindings under `make spice-ryll` in Phase 2; if anything
   was visibly off, capture it here. If the lists are
   identical (or the differences do not affect any binding),
   say so explicitly so a future reviewer knows the question
   is closed.

**Commit message:** "Record qxl GOP mode list." or similar
short imperative. `Prompt:` paragraph explains: "Phase 1
deferred capturing the qxl mode list because no ryll
display-mode-ui build was on hand; Phase 3's smoke-test
session against ryll closed that gap."

### 3b — documentation fan-out

**Files:** `README.md`, `ARCHITECTURE.md`, `AGENTS.md`.

**`README.md` changes:**

1. In the `## Building and running` section (or a new
   `### Display-mode keystrokes` subsection beneath
   `### Locked-bootloader scene`), add a paragraph
   describing the keystrokes:

   - Keys `'1'`–`'6'` switch GOP framebuffer to the
     resolutions in the master plan's binding table
     (640×480, 800×600, 1024×768, 1280×720, 1280×1024,
     1920×1080).
   - Key `'0'` walks every available mode with a 1-second
     dwell per step, interruptible by any keystroke.
   - On-screen toast on the bottom row names the applied
     resolution (or the substitution form when the
     firmware doesn't expose the requested mode).
   - Mode keys work in awaiting / booting / parked. They
     are silently ignored inside the locked-bootloader
     R/I/A and paste-prompt loops (which own their own
     keys).
   - `make spice-ryll` against ryll's `display-mode-ui`
     branch is the canonical acceptance-test path.

2. Update the `## Status` section if it lists landed
   milestones; add a one-liner for display-mode keystrokes.

**`ARCHITECTURE.md` changes:**

Add a paragraph on runtime mode switching to the existing
scene-state-machine discussion. Cover:

- `Renderer::set_mode(req_w, req_h)` queries back from GOP
  for the applied dimensions and is the canonical mode-
  change entry point.
- `Scene::repaint` is the framebuffer-invalidation-contract
  honourer: per UEFI 2.10 §12.9, `set_mode` invalidates the
  framebuffer, so every mode switch is followed by
  `repaint`.
- `RepaintState` is the per-phase snapshot the runners
  update as they play; `repaint` consults it to replay
  just the right prefix of the boot scripts at the new
  dimensions.
- The locked-bootloader sub-state-machine is carved out:
  `try_handle_mode_key` is not invoked from
  `src/bootloader.rs`. Mode keys received during the R/I/A
  or paste-prompt loops are logged-and-ignored.

Place the paragraph adjacent to the existing renderer /
scene description so a reader sees the mechanism in
context. Aim for ~10 lines.

**`AGENTS.md` changes:**

Update the `## Current phase` section. Replace the
locked-bootloader phase 3 description with display-mode
keystrokes phase 3 (now landing). Cover:

- All three phases of display-mode-keystrokes have landed.
  Keystrokes `'1'`–`'6'` and `'0'` are first-class control
  surface; `make spice-ryll` is the acceptance path.
- The locked-bootloader carve-out: sub-agents touching
  scene code must not call `try_handle_mode_key` from
  inside `src/bootloader.rs`'s input loops.
- The previous mention of locked-bootloader as the
  "current phase" gets retired; if the section already
  notes the locked-bootloader-as-most-recently-landed,
  this milestone supersedes it.

### 3c — inventory + master-plan closeout

**Files:** `docs/spice-test-inventory.md`,
`docs/plans/PLAN-display-mode-keystrokes.md`,
`docs/plans/index.md`. Plus this phase plan's *Outcome*
and *Exit criteria* sections.

**`docs/spice-test-inventory.md`:**

In the *Display channel* table (around lines 64–80),
update the *Mode walk across all offered modes* row's
status from `—` to:

```
binary: [display-mode-keystrokes](plans/PLAN-display-mode-keystrokes.md)
```

The *Mode set* row's existing `binary: phase 4` link
stays. Other display-channel rows stay `—`.

**`docs/plans/PLAN-display-mode-keystrokes.md` (master
plan):**

1. Mark Phase 3 *Complete* in the *Execution* table with
   the commit range from this phase's commits.
2. In the *Success criteria* section, tick every checkbox
   with a per-item note pointing at the relevant phase
   commit (1a–1e for Phase 1 items, 2a–2e for Phase 2
   items, 3a–3c for Phase 3 items). The checklist already
   has many specific items; verify each one against what
   landed.
3. The `cargo test` line in *Success criteria* (added
   originally in the master plan, then deferred in Phase
   1's plan) gets struck through:

   ```
   - [ ] ~~Host-side `cargo test` passes, including a
         `nearest_mode` unit test.~~ *(Deferred: see Phase 1
         plan's Scope → Deferred from the master plan
         section. Validated by construction + indirect via
         `Renderer::new` + Phase 2 smoke tests instead.)*
   ```
4. Spot-check the *Future work* and *Bugs fixed during
   this work* sections; if anything was deferred or
   discovered during execution, capture it. None expected.

**`docs/plans/index.md`:**

Update the *Master plans* row for display-mode keystrokes:

- *Status* column from *Not started* → *Complete (commits
  46aae05 through this closeout)* (or whichever first
  and last commits the range covers).

**This phase plan:**

- Populate the *Outcome* section with what shipped + what
  was deferred (none expected for Phase 3).
- Tick the *Exit criteria* checklist.

## Exit criteria

- [x] qxl mode list captured under headless QEMU
      (equivalent to `make spice-ryll`'s qxl device) and
      pasted into the master plan's *Open questions*
      alongside the std list. No binding-key substitutions
      under either device. *(commit `1c06122`.)*
- [x] `README.md` documents the keystroke affordance, the
      bootloader carve-out, and the `make spice-ryll`
      acceptance path. *(commit `d29187d`.)*
- [x] `ARCHITECTURE.md` carries a paragraph on runtime
      mode switching naming `Renderer::set_mode`,
      `Scene::repaint`, and `RepaintState`. *(commit
      `d29187d`.)*
- [x] `AGENTS.md` *Current phase* section updated to
      describe display-mode keystrokes as the most
      recently completed milestone, with the carve-out
      called out for sub-agents. (Heading renamed to
      `## Most recently landed` per the phase plan's
      flagged judgement call.) *(commit `d29187d`.)*
- [x] `docs/spice-test-inventory.md` *Mode walk across all
      offered modes* row's status carries a `binary:` link
      to the master plan. *(this closeout.)*
- [x] Master plan's *Execution* table marks Phase 3
      Complete with the commit range. *(this closeout.)*
- [x] Master plan's *Success criteria* checklist ticked
      with per-item verification notes; `cargo test` line
      struck through with the Phase-1 deferral rationale.
      *(this closeout.)*
- [x] `docs/plans/index.md` *Master plans* row updated to
      *Complete* with the full commit range from
      `455d2b5` (master plan) through this closeout.
      *(this closeout.)*
- [x] `pre-commit run --all-files` exits 0 at every commit
      across the phase. *(verified per commit at 3a, 3b,
      and after the closeout.)*
- [x] No code changes in this phase. `git diff
      f1aeac6..HEAD -- src/` returns empty for Phase 3's
      commits. *(verified.)*
- [x] `make screenshot`, `make qemu`, `make spice-ryll`,
      and `make release-verify` all continue to work
      identically to Phase 2's baseline. *(no behavioural
      changes since Phase 2's `1b1b446`.)*
- [x] Commit messages follow the project's template.
      *(verified by inspection of `1c06122` and `d29187d`;
      this closeout follows the same shape.)*

## Risks and open questions

- **qxl mode list might be richer or sparser than the std
  list.** Either way, all six binding keys were
  operator-confirmed working under `make spice-ryll` in
  Phase 2's smoke test, so any divergence is informational
  rather than a blocker. Step 3a captures the divergence
  for future reviewers; no code changes follow.

- **`AGENTS.md` Current-phase wording.** This is the
  third milestone in a row to claim "Current phase" — at
  some point the section's framing stops being right
  ("phase" implies one in flight, not a chain of completed
  ones). Step 3b should rename or restructure if the
  current text feels awkward against this milestone's
  closeout. If a rename is non-obvious, leave it alone
  and capture the awkwardness as future work.

- **`README.md` ordering.** The keystrokes are a
  cross-cutting affordance (work in awaiting / booting /
  parked, except inside the bootloader). Whether the
  description belongs as a top-level
  `### Display-mode keystrokes` subsection alongside
  `### Locked-bootloader scene` or as a paragraph in the
  general operator-controls flow is a judgement call.
  Step 3b should pick what reads better in context. If
  unclear, lean toward a top-level subsection — it is
  easier to find from the table of contents.

- **Locked-bootloader carve-out wording.** Repeated in
  three places after this phase (master plan, phase 1
  plan, phase 2 plan, README, ARCHITECTURE, AGENTS).
  That is fine; each surface has a different audience.
  Skim once at review time to make sure the descriptions
  do not contradict each other.

## Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with it. In particular:
confirm that no source code changes are expected in Phase
3, confirm the locked-bootloader carve-out language stays
consistent across all the doc surfaces, and confirm that
DESIGN.md is intentionally excluded from the doc fan-out.
