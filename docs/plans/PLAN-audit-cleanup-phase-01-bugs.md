# Audit cleanup — phase 1: real bug fixes

Parent plan: [PLAN-audit-cleanup.md](PLAN-audit-cleanup.md).

## Outcome

**Status: Not started.**

This section will be populated as Phase 1 lands.

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

- [ ] `Renderer::draw_glyph`, `Renderer::draw_cursor_glyph`,
      and `Renderer::clear_cell` all have the bounds-check
      guard.
- [ ] `make screenshot` produces the same 59-event
      transcript as `8255685`'s baseline (no regression in
      the no-keystroke path).
- [ ] `README.md`'s toast format examples are ASCII-only,
      with `mode ` prefix on exact-match.
- [ ] `ARCHITECTURE.md` references `draw_text_bitmap`, not
      `draw_logo`. `grep -r draw_logo` returns no matches
      anywhere in the repo.
- [ ] `pre-commit run --all-files` exits 0 at every commit.
- [ ] Commit messages follow project conventions.

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
