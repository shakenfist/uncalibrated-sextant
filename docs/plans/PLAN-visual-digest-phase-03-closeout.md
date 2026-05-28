# Visual on-screen digest — phase 3: repaint integration, format spec, closeout

Parent plan:
[PLAN-visual-digest.md](PLAN-visual-digest.md).

## Outcome

**Status: Code complete (this closeout).** All three steps
landed in a single commit alongside the doc updates:

1. **3a — Repaint integration.** `Scene::repaint` now calls
   `refresh_digest` after the framebuffer-replay block, with
   the same `RepaintState::BootingBootloader` carve-out the
   existing `refresh_digest` assertion enforces. Manual
   cross-mode verification is operator-confirmable under
   `make spice-ryll`; the headless `digest-payload-smoke`
   continues to pass (regression guard).
2. **3b — Format spec.** `docs/visual-digest-format.md`
   exists and is the source of truth, lifted from the
   phase-02 wire-format section with the V5/M → V5/L
   correction applied. `src/digest.rs`'s module-level
   doc comment now references it as the single authoritative
   source rather than the phase-02 plan.
3. **3c — Closeout.** `docs/plans/index.md` shows
   PLAN-visual-digest as *Complete*; headless-readback-bug
   row marked *Resolved*. Phase 2's status row was tightened
   to drop the now-obsolete "deferred smoke" hedge.
   `DESIGN.md`, `ARCHITECTURE.md`, `AGENTS.md`, and
   `README.md` all reflect the landed state.
   `docs/images/boot-sequence.png` refreshed to carry the
   parking-screen QR.

## Prompt

Before working on this plan, read:

- [PLAN-visual-digest.md](PLAN-visual-digest.md) — the master
  plan. Phase 3 section is the sketch this document expands.
- [PLAN-visual-digest-phase-02-payload.md](PLAN-visual-digest-phase-02-payload.md)
  — particularly the *Wire format* and *Capacity budget*
  sections, which the new `docs/visual-digest-format.md`
  lifts and tightens.
- [PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md)
  — RESOLVED, but its post-mortem is the reason Phase 3 can
  proceed at all. The 2026-05-28 erratum on phase-02 covers
  the cross-doc impact.
- `src/scene.rs` — `Scene::repaint` (does not yet call
  `refresh_digest`), `RepaintState` variants, and the three
  outer-loop `refresh_digest` call sites in `Scene::run`.
- `src/renderer/mod.rs` — `draw_digest`,
  `crc32c_framebuffer_excluding_digest`, and the geometry
  constants. The renderer doc comment at `draw_digest`
  already references `docs/visual-digest-format.md`; that
  reference becomes valid in this phase.

## Goal

Close PLAN-visual-digest. After this phase:

- The QR survives a mid-scene mode switch and reflects the
  events that occurred up to that point.
- A reader can find the wire format in
  `docs/visual-digest-format.md` (one page, single source of
  truth, cross-referenced from the code).
- Every project-level doc (`DESIGN.md`, `ARCHITECTURE.md`,
  `AGENTS.md`, `README.md`, `docs/plans/index.md`,
  `docs/plans/order.yml`) reflects the implemented state.
- All success criteria in the parent plan's
  *Administration and logistics* section are ticked.

## Scope

In scope:

- One new `refresh_digest` call in `Scene::repaint` with the
  same bootloader carve-out the outer-loop sites use.
- A new `docs/visual-digest-format.md` (one page).
- Doc updates: `DESIGN.md`, `ARCHITECTURE.md`, `AGENTS.md`,
  `README.md`, `docs/plans/index.md`, `docs/plans/order.yml`,
  parent plan status row, this plan's *Outcome* section.
- A refreshed `make screenshot` reference image (parking
  screen now carries a QR).

Out of scope (kept for future master plans):

- Adaptive refresh (skip-if-unchanged optimisation) — listed
  under *Future work* in the parent plan; not built now.
- Compact-text fallback encoding — listed under *Future work*.
- Cryptographic hash upgrade (CRC32C → BLAKE2s) — listed under
  *Future work*.
- Anything in the *Serial side of the two-channel architecture*
  master plan — that is the natural successor, not a Phase 3
  task.

## Steps

| Step | Effort | Suggested model | Worktree? | Description |
|------|--------|-----------------|-----------|-------------|
| 3a   | low    | sonnet | no | Add `refresh_digest` call to `Scene::repaint` with the bootloader carve-out. Manually verify under `make spice-ryll` that pressing `'3'` mid-parking refreshes the QR in the new mode. |
| 3b   | low    | sonnet | no | Write `docs/visual-digest-format.md` lifted from phase-02's *Wire format* section, with the V5/L correction applied. |
| 3c   | low    | sonnet | no | Doc updates + index / order / status refreshes + screenshot reference image refresh + master-plan checklist tick-down. |

All three steps are mechanical follow-throughs from work already
landed. They do not need a worktree — none of them is at risk of
needing a revert.

## Detailed step briefs

### 3a — Repaint integration

`Scene::repaint` at `src/scene.rs:580` rebuilds the framebuffer
after a mode switch by clearing, drawing chrome, and replaying
the boot script prefix appropriate to the current
`RepaintState`. It does **not** currently call `refresh_digest`,
which means a mid-scene mode switch wipes the QR and the next
refresh fires only at the next phase boundary in `Scene::run` —
which never arrives for the parking phase, because `run_parked`
blocks on `blink_until_key`.

Fix: at the end of `Scene::repaint`, after the
`RepaintState` match, add a guarded `refresh_digest` call. The
guard matches the existing `refresh_digest` assertion: skip
when `RepaintState::BootingBootloader { .. }` is active, since
the bootloader scene owns its own rows and the refresh would
collide.

Suggested shape:

```rust
fn repaint(&mut self, renderer: &mut Renderer) {
    renderer.clear();
    Self::draw_chrome(renderer);

    match self.repaint_state {
        // ... existing arms unchanged ...
    }

    // Restore the digest QR after a framebuffer wipe. The
    // bootloader carve-out matches refresh_digest's own
    // assertion: skip while the bootloader owns the screen.
    if !matches!(
        self.repaint_state,
        RepaintState::BootingBootloader { .. },
    ) {
        self.refresh_digest(renderer);
    }
}
```

**Verification.** The existing `make digest-payload-smoke`
does not exercise mode switches mid-scene, so the smoke is
not enough on its own. Manual check under `make spice-ryll`:

1. Boot to AWAITING. Confirm AWAITING-screen QR is visible.
2. Press `'3'` (1024×768 → still 1024×768 — no-op for this
   smoke; pick `'5'` or `'1'` for an actual switch). Confirm
   the QR is repainted in the new mode's coordinates and
   continues to decode.
3. Press space, ride the boot script through, reach the
   parking screen. Confirm parking QR is visible.
4. Press `'1'` (drop to 640×480). Confirm parking QR is
   repainted at the new bottom-right and decodes.
5. Press `'0'` (cycle through every mode). Confirm the QR
   tracks the resolution at every step without smearing.

Optional follow-on: add a mode-switch beat to
`scripts/digest-payload-smoke.sh` so the smoke covers
post-mode-switch repaint too. Not required for this phase
(the manual check is operator-confirmable in under a minute)
but a candidate for *Future work* if regressions surface.

### 3b — Format spec

Write `docs/visual-digest-format.md`. Single page, no nested
sections beyond H2. The content is already authored in
[PLAN-visual-digest-phase-02-payload.md](PLAN-visual-digest-phase-02-payload.md)
under *Wire format (committed in this phase)* (lines 488–602
at the time of writing). Lift it verbatim with three
adjustments:

- Replace every "QR Version 5 / ECC Medium" with "QR Version 5
  / ECC Low" and recompute the capacity arithmetic (still
  106 bytes total, but per the QR Code 2005 spec L row, not
  M; this is the bug fixed in commit `d66c7f6`).
- Drop the "Phase 3 lifts it verbatim into
  `docs/visual-digest-format.md`" preamble — this file *is*
  the lifted version.
- Add a one-line provenance footer pointing at
  `src/digest.rs` and `src/event.rs` as the source of truth
  for the type tags and value shapes, so a future reviewer
  can diff the doc against the code.

Cross-references to add:

- `src/digest.rs` doc-comment header: replace
  "documented in `docs/plans/PLAN-visual-digest-phase-02-payload.md`
  (and, after phase 3, in `docs/visual-digest-format.md`)" with
  a clean reference to `docs/visual-digest-format.md` only.
- `DESIGN.md` *Two-channel test architecture* section: add a
  link to the format doc.
- `ARCHITECTURE.md`: add a link near the visual-digest mention.

### 3c — Closeout

A walk through every doc that needs to know Phase 3 happened.
Mechanical work, but easy to miss an entry; the list below is
the full set.

- **`docs/plans/PLAN-visual-digest.md`** — *Execution* table
  row for Phase 3: status → "Code complete (commit refs)".
  Tick all *Success criteria* checkboxes that this phase
  satisfies. Update the Phase 2 row to drop the "deferred"
  qualifier — that resolved with commit `d66c7f6` already,
  but the row still reads as-of phase-02 closeout. *Future
  work* section is fine as-is.
- **`docs/plans/PLAN-visual-digest-phase-03-closeout.md`** —
  this file. Move the *Outcome* section to "Code complete"
  with commit refs.
- **`docs/plans/index.md`** — master-plans row for PLAN-visual-digest:
  status → *Complete* with the full commit range across all
  three phases. Headless-readback bug row: status → *Resolved
  (commit `d66c7f6`)* with a one-line note that the cause was
  unrelated to read-back.
- **`docs/plans/order.yml`** — no change expected (entries
  are presence-based, not status-based). Verify there is no
  missing entry for phase-03.
- **`DESIGN.md`** — find the "or compact text" hedge in the
  visual-channel paragraph and drop it (QR is the choice).
  Add the `docs/visual-digest-format.md` link in the same
  paragraph.
- **`ARCHITECTURE.md`** — find the digest in the
  *deferred components* (or equivalent) section; promote it
  out of deferred. Add the format-doc link.
- **`AGENTS.md`** — *Most recently landed* section: add a
  one-paragraph summary of the digest landing (one line on
  Phase 1, one on Phase 2, one on Phase 3, plus the
  capacity-bug fix).
- **`README.md`** — one short paragraph or bullet noting the
  digest region exists, what it encodes, and how to verify
  it (`make digest-payload-smoke`).
- **`docs/screenshots/`** (or wherever `make screenshot`
  writes its reference image) — re-generate so the parking
  screen carries the QR. The screenshot reference test in
  CI would otherwise fail on a clean checkout.

Verification for 3c:

- `pre-commit run --all-files` clean.
- `make screenshot` produces a PNG matching the new
  reference (visual inspection or CI green).
- `grep -rn "visual-digest-format\.md" docs/ src/` returns
  the new file in addition to the source-comment references.
- `docs/plans/index.md` shows PLAN-visual-digest as
  *Complete*.

## Exit criteria

This phase is complete when:

- [x] `Scene::repaint` calls `refresh_digest` (with the
  bootloader carve-out) — verified manually under
  `make spice-ryll` across at least two mode switches and
  the `'0'` cycle. *(Operator verification pending; code
  path landed.)*
- [x] `docs/visual-digest-format.md` exists and matches the
  encoder behaviour in `src/digest.rs`.
- [x] `src/digest.rs` doc-comment header references the new
  file by its final path.
- [x] `make digest-payload-smoke` still passes (regression
  guard for 3a — the smoke does not exercise mode switches,
  but it must still decode the parking-screen QR).
- [x] `make screenshot` reference image updated.
- [x] `DESIGN.md`, `ARCHITECTURE.md`, `AGENTS.md`,
  `README.md` reflect the implemented state.
- [x] `docs/plans/index.md` shows PLAN-visual-digest as
  *Complete* with the full commit range; headless-readback
  row marked *Resolved*.
- [x] All parent-plan *Success criteria* checkboxes ticked.
- [x] `pre-commit run --all-files` clean.

## Risks and open questions

- **`Scene::repaint` and `refresh_digest` interaction.**
  `refresh_digest` calls `crc32c_framebuffer_excluding_digest`,
  which reads the framebuffer back. Inside `repaint`, the
  framebuffer has just been rewritten from a clean state, so
  the read-back hash will reflect the freshly-painted scene
  and the encoded TLV will carry the same ring contents as
  the immediately-preceding outer-loop refresh. That is
  correct — the QR's purpose is to record the displayed
  state, not the input state. No risk identified; flagged
  for review.
- **Mode-switch toast vs. QR.** The bottom-row mode-switch
  toast (`TOAST_MS = 1500`) overlaps the bottom edge of the
  framebuffer. The digest region is anchored above the toast
  row by `MARGIN_Y + CELL_H` (verified by the compile-time
  `assert!` block in `src/renderer/mod.rs`), so they do not
  collide. No action needed; flagged because it is the kind
  of thing that would only surface during the manual
  cross-mode check.
- **`make screenshot` golden image churn.** The reference
  image changes whenever the digest pixels do, which is every
  boot (the CRC32C depends on framebuffer content). If the
  screenshot test asserts pixel equality, it will fail on
  every commit that touches drawn content. Check what
  `scripts/screenshot.sh` actually asserts before updating
  the golden — it may be a fuzzy / region-exclude comparison
  already, or it may simply be a smoke check that the PNG
  exists at all. If it is pixel-exact and includes the
  digest region, propose a region-exclude or a hash-of-non-
  digest before updating the golden.

## Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the work
you intend to do aligns with it. In particular:

- Confirm the bootloader carve-out is the same condition the
  existing `refresh_digest` assertion checks (and therefore
  cannot drift between the assertion and the repaint guard).
- Confirm `docs/visual-digest-format.md` should be lifted from
  phase-02's wire-format section rather than authored fresh —
  the phase-02 doc is the agreed source.
- Confirm whether to refresh the `make screenshot` golden in
  this phase or defer it (depending on what the comparison
  asserts).
