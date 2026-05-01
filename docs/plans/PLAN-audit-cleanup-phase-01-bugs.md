# Audit cleanup — phase 1: real bug fixes

Parent plan: [PLAN-audit-cleanup.md](PLAN-audit-cleanup.md).

## Outcome

**Status: Complete (commits a42af74, 5fd347c, 56b8028).**

All three real bugs fixed in three small commits. `make
screenshot` confirms the no-keystroke regression test still
drains 59 events.

### What Phase 1 actually delivered

- Bounds-check guards on `Renderer::draw_glyph`,
  `draw_cursor_glyph`, `clear_cell`, and `clear_row` (commit
  `a42af74`). The sub-agent extended slightly beyond the
  brief by also guarding `clear_row`, which was the right
  call: it's reachable from `Scene::repaint`'s toast cleanup
  path. `draw_text_at` gets transitive coverage via
  `draw_glyph`. `draw_text_bitmap` is left unguarded for now
  per the brief's scope; only callers are static chrome /
  probe rendering at well-bounded positions.
- README.md's toast format examples corrected to ASCII
  (`mode 1024x768` and `requested 1280x720 -> using
  1024x768`) with a clarifying sentence about the renderer's
  ASCII-only font (commit `5fd347c`).
- ARCHITECTURE.md's stale `Renderer::draw_logo` reference
  replaced with `draw_text_bitmap` (commit `56b8028`).
  `grep -rn draw_logo` from repo root returns zero live
  references; only archival mentions in this plan and the
  master plan remain.

## Prompt

Re-read the master plan's *Phase 1 sketch*. Skim the three
audit findings this phase addresses (linked from the master
plan's *Mission* section).

This phase is mechanical bug-fix work. No design judgement
needed. **Plan at low effort (sonnet); no opus, no worktree.**

## Goal

Fix the three real bugs the audit found:

1. `Renderer::draw_glyph` / `draw_cursor_glyph` / `clear_cell`
   don't bounds-check `(col, row)` before computing pixel
   coordinates. Reachable in production by pressing `'1'`
   (640×480) before pressing `'i'` and pasting a >64-char
   string at the bootloader prompt — paste echo silently
   fails to render past column 76. UEFI's GOP rejects the
   blt and `let _ =` swallows the error, so the binary
   doesn't crash; it just silently misbehaves.
2. `README.md`'s toast format examples use Unicode `×` and
   `→` that the ASCII-only renderer cannot produce. What
   the operator actually sees is `mode 1024x768` and
   `requested 1280x720 -> using 1024x768`.
3. `ARCHITECTURE.md:55` references `Renderer::draw_logo`
   which doesn't exist; the method is `draw_text_bitmap`.
   Rename predates this milestone but was not caught
   during the display-mode-keystrokes Phase 3 doc fan-out.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | low    | sonnet | none      | Add bounds-check guard `if col >= self.screen_cols() \|\| row >= self.screen_rows() { return; }` to `Renderer::draw_glyph`, `draw_cursor_glyph`, and `clear_cell` in `src/renderer/mod.rs`. Verify `make screenshot` produces the same 59-event transcript. |
| 1b   | low    | sonnet | none      | Replace Unicode `×` with `x` and `→` with `->` in `README.md`'s toast format examples (lines ~152–156). Add `mode ` prefix to the exact-match example. Note in surrounding prose that the renderer is ASCII-only so examples render verbatim. |
| 1c   | low    | sonnet | none      | Replace `Renderer::draw_logo` with `Renderer::draw_text_bitmap` in `ARCHITECTURE.md:55`. Verify by `grep draw_logo` that no other stale references remain anywhere in the repo. |

Three commits expected, one per step. The management session
can run all three sequentially via a single sub-agent or
spawn one agent per step — pick whichever is cheaper. Given
the size (probably <30 lines total), one bundled sub-agent
producing three discrete commits is fine.

## Exit criteria

- [x] `Renderer::draw_glyph`, `Renderer::draw_cursor_glyph`,
      `Renderer::clear_cell`, and `Renderer::clear_row` all
      have the bounds-check guard. *(commit `a42af74`;
      `clear_row` added beyond the brief because it's
      reachable from toast cleanup.)*
- [x] `make screenshot` produces the same 59-event
      transcript as `8255685`'s baseline. *(verified after
      each commit and after the full phase.)*
- [x] `README.md`'s toast format examples are ASCII-only,
      with `mode ` prefix on exact-match. *(commit
      `5fd347c`.)*
- [x] `ARCHITECTURE.md` references `draw_text_bitmap`, not
      `draw_logo`. `grep -r draw_logo` returns no live
      matches anywhere in the repo (only archival mentions
      in this plan and the master plan). *(commit
      `56b8028`.)*
- [x] `pre-commit run --all-files` exits 0 at every commit.
- [x] Commit messages follow project conventions.

## Risks

- **Bounds-check might mask a future bug.** If a caller
  silently passes out-of-bounds coordinates, the new guard
  hides the symptom. Acceptable because (a) the previous
  behaviour was already silent failure via GOP rejection,
  and (b) the alternative (panicking on out-of-bounds) is
  worse in UEFI than in a hosted environment. If a future
  caller wants to detect the case, the right place is a
  `debug_assert!` adjacent to the early return — capture
  in master plan's *Future work* if it becomes load-bearing.

## Back brief

Confirm that the bounds-check is silent (no event, no log),
and that the README and ARCHITECTURE edits don't change any
prose beyond the literal corrections.
