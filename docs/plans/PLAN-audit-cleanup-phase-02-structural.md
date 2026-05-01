# Audit cleanup — phase 2: structural dedup

Parent plan: [PLAN-audit-cleanup.md](PLAN-audit-cleanup.md).
Previous phase:
[PLAN-audit-cleanup-phase-01-bugs.md](PLAN-audit-cleanup-phase-01-bugs.md).

## Outcome

**Status: Not started.**

This section will be populated as Phase 2 lands.

## Prompt

Re-read the master plan's *Phase 2 sketch* and *Open
questions*. Skim:

- [`src/scene.rs`](../../src/scene.rs) — `run_awaiting`
  (around lines 340–390), `run_parked` (around lines
  810–880), `try_handle_mode_key` and `cycle_modes`
  (around lines 776 and 675), and `RepaintState` (around
  line 220).
- [`src/bootloader.rs`](../../src/bootloader.rs) — only
  for the duplicated `PACE_LINE_MS` constant near the top
  and the `run_timeout` countdown logic. The bootloader's
  input loops are **not** touched by this phase.
- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) —
  `draw_glyph` and `draw_cursor_glyph` for the
  `blit_glyph_bytes` extraction.

This phase is mostly mechanical with one design call:
`blink_until_key`'s closure signature. The default in the
master plan's *Open questions* is a generic
`FnOnce(&mut Self)` closure, accepted by generic parameter
to avoid the `Box<dyn FnOnce>` heap allocation that a
trait-object dispatch would require in `no_std`.

**Plan at medium effort (sonnet).** No opus, no worktree.

## Goal

Land six structural-cleanup commits that the audit flagged
as advisory. Each is small individually; together they make
adding a fourth scene runner cheaper and remove the only
remaining drift hazard between `scene.rs` and
`bootloader.rs`.

By the end of this phase:

- `PACE_LINE_MS` exists in exactly one location in `src/`,
  re-exported `pub(crate)` from `scene.rs` and imported by
  `bootloader.rs`. The "kept in sync with scene.rs" comment
  in `bootloader.rs` is gone.
- `Renderer::draw_glyph` and `Renderer::draw_cursor_glyph`
  share a private `blit_glyph_bytes` helper. ~20 lines of
  duplicated `BltPixel` composition removed.
- `Scene::run_awaiting` and `Scene::run_parked` are
  two-line stubs around a shared `Scene::blink_until_key`
  helper. Adding a third waiting-screen scene becomes a
  three-liner, not a copy-paste of the entire blink loop.
- `bootloader::run_timeout` carries
  `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99)` so a future
  bump past two-digit countdown silently rendering garbage
  is impossible.
- `Scene::try_handle_mode_key` and `Scene::cycle_modes`
  are private (`fn`, not `pub(crate) fn`) since neither
  has any caller outside `scene.rs`.
- `RepaintState` carries a doc comment documenting the
  "one variant per scene runner; keep variant fields
  minimal" intent.

## Scope

**In scope (six steps, six commits):**

- 2a: `PACE_LINE_MS` re-export from `scene.rs`; remove
  duplicate from `bootloader.rs`.
- 2b: `Renderer::blit_glyph_bytes` private helper.
- 2c: `Scene::blink_until_key` extraction.
- 2d: `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99)` in
  `run_timeout`.
- 2e: Visibility tightening on `try_handle_mode_key` and
  `cycle_modes`.
- 2f: Doc comment on `RepaintState`.

**Out of scope (deferred to Phase 3 or *Future work*):**

- `make screenshot-modes` — Phase 3.
- `verify-release.sh` GOP-mode grep — Phase 3.
- The polling-stall pattern unification across
  `Scene::stall_with_keys` + bootloader R/I/A loop +
  bootloader paste-capture loop. Master-plan *Future
  work*; not addressed here because the per-loop key
  filter makes a closure-based abstraction non-trivial in
  `no_std`.
- Module splits for `scene.rs` / `bootloader.rs` — master
  plan *Future work*, defer until adding a fourth scene.
- `format_u32` to a shared `src/util.rs` — master plan
  *Future work*, defer until a second user.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 2a   | low    | sonnet | none      | Re-export `pub(crate) const PACE_LINE_MS` from `src/scene.rs`. Remove the duplicate declaration in `src/bootloader.rs` and update its `use` line to import the constant from `crate::scene`. Drop the "kept in sync with scene.rs" comment. |
| 2b   | low    | sonnet | none      | Add a private `Renderer::blit_glyph_bytes(&mut self, bytes: &[u8; 16], col, row)` helper that does the `BltPixel` composition currently duplicated by `draw_glyph` and `draw_cursor_glyph`. Both callers become wrappers around the lookup / direct-byte-pass plus one call to the helper. Bounds-check guard moves into the helper if cleaner; otherwise stays in each caller. |
| 2c   | medium | sonnet | none      | Extract `Scene::blink_until_key<F: FnOnce(&mut Self)>(&mut self, &mut Renderer, cursor_col, cursor_row, on_exit: F)` from the duplicated logic in `run_awaiting` and `run_parked`. Both runners become two-line stubs that call the helper with the right cursor position and an exit closure that pushes the appropriate `SceneTransition` event and (for `run_awaiting`) sets `self.phase = Phase::Booting`. |
| 2d   | low    | sonnet | none      | Add `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99);` at the top of `bootloader::run_timeout`. Document inline (one-line comment) why: the countdown uses two fixed cells and a three-digit value would silently render garbage. |
| 2e   | low    | sonnet | none      | Change `Scene::try_handle_mode_key` and `Scene::cycle_modes` from `pub(crate) fn` to private `fn`. Confirm there are no external callers (a `grep` for each name from the repo root should return matches only inside `src/scene.rs`). |
| 2f   | low    | sonnet | none      | Add a doc comment on the `RepaintState` enum (~3 lines) documenting: "one variant per scene runner; carry just enough state for `Scene::repaint` to reconstruct the visible content; resist adding fields beyond what the repaint actually consumes." |

Two sub-agent runs expected: one bundled run for the five
mechanical items (2a, 2b, 2d, 2e, 2f), one focused run for
2c (the design step). Alternatively, one bundled run for
all six — your call as the management session. The diff
total is probably under 200 lines.

## Detailed step briefs

### 2a — `PACE_LINE_MS` single source

**Files:** `src/scene.rs`, `src/bootloader.rs`.

In `src/scene.rs`, change the existing `const PACE_LINE_MS:
u64 = 200;` to `pub(crate) const PACE_LINE_MS: u64 = 200;`.

In `src/bootloader.rs`:

- Remove the local `const PACE_LINE_MS: u64 = 200;`
  declaration and any "kept in sync with scene.rs" comment.
- Add `PACE_LINE_MS` to the existing `use crate::scene::{...};`
  line (which already imports `poll_key`, `stall`,
  `POLL_MS`).

`grep -rn 'PACE_LINE_MS' src/` afterwards should show one
declaration in `scene.rs` and one or more usages in both
files.

### 2b — `blit_glyph_bytes` helper

**File:** `src/renderer/mod.rs`.

Add a private helper:

```rust
/// Blit an 8x16 glyph from raw bitmap bytes to the cell
/// at `(col, row)`. Used by `draw_glyph` (which looks up
/// the bitmap from the font table) and `draw_cursor_glyph`
/// (which is given raw bytes for the cursor variants).
///
/// Issues exactly one `BltOp::BufferToVideo` call
/// (principle 6). Out-of-bounds `(col, row)` is the
/// caller's contract — see the bounds-check guards on
/// `draw_glyph` and `draw_cursor_glyph`.
fn blit_glyph_bytes(&mut self, bytes: &[u8; 16], col: usize, row: usize) {
    let mut buf = [BG; CELL_W * CELL_H];
    for (r, &byte) in bytes.iter().enumerate() {
        for c in 0..CELL_W {
            if byte & (0x80 >> c) != 0 {
                buf[r * CELL_W + c] = FG;
            }
        }
    }

    let px = MARGIN_X + col * CELL_W;
    let py = MARGIN_Y + row * CELL_H;
    let _ = self.gop.blt(BltOp::BufferToVideo {
        buffer: &buf,
        src: BltRegion::Full,
        dest: (px, py),
        dims: (CELL_W, CELL_H),
    });
}
```

Refactor `draw_glyph` to look up the font bitmap and
delegate:

```rust
pub fn draw_glyph(&mut self, ch: char, col: usize, row: usize) {
    if col >= self.screen_cols() || row >= self.screen_rows() {
        return;
    }
    let index = if (ch as u32) < 0x80 { ch as usize } else { b'?' as usize };
    self.blit_glyph_bytes(&font::FONT_8X16[index], col, row);
}
```

Refactor `draw_cursor_glyph` to delegate:

```rust
pub fn draw_cursor_glyph(&mut self, bytes: &[u8; 16], col: usize, row: usize) {
    if col >= self.screen_cols() || row >= self.screen_rows() {
        return;
    }
    self.blit_glyph_bytes(bytes, col, row);
}
```

The bounds-check guards stay in each caller (so the helper
itself can be called from any future caller without
re-checking). Net diff: ~20 lines removed, 1 new method
added.

### 2c — `blink_until_key` extraction

**File:** `src/scene.rs`.

This is the design step of Phase 2. The two runners
`run_awaiting` and `run_parked` are near-identical blink
loops; extract the shared logic into a helper.

**Helper signature (per master plan's *Open questions*
default — generic `FnOnce` to avoid `Box<dyn FnOnce>`):**

```rust
/// Run a cursor-blink poll loop until a non-mode keystroke
/// arrives. The cursor blinks at `(cursor_col, cursor_row)`;
/// each tick polls keys, dispatches mode keys via
/// `try_handle_mode_key` (which the loop calls in a way
/// that respects the locked-bootloader carve-out — `this
/// helper is not invoked from src/bootloader.rs`), and ticks
/// the toast TTL.
///
/// On a non-mode keystroke, pushes a `Keypress` event and
/// then invokes `on_exit(self)`. The closure is responsible
/// for any phase transition and final event pushes (e.g.
/// `SceneTransition`); after it returns, the helper
/// returns.
fn blink_until_key<F: FnOnce(&mut Self)>(
    &mut self,
    renderer: &mut Renderer,
    cursor_col: usize,
    cursor_row: usize,
    on_exit: F,
) {
    loop {
        let glyph = self.cursor.tick(POLL_MS);
        Self::draw_or_clear_cursor(renderer, glyph, cursor_col, cursor_row);
        self.tick_toast(renderer, POLL_MS);
        stall(&mut self.clock_ms, POLL_MS);

        if let Some((ch, sc)) = poll_key() {
            self.ring.push(Event::Keypress {
                unicode: ch,
                scancode: sc,
                timestamp_ms: self.clock_ms,
            });

            if self.try_handle_mode_key(renderer, ch) {
                continue;
            }

            on_exit(self);
            return;
        }
    }
}
```

**`run_awaiting` becomes:**

```rust
fn run_awaiting(&mut self, renderer: &mut Renderer) {
    const CURSOR_COL: usize = 0;
    const CURSOR_ROW: usize = 0;

    self.repaint_state = RepaintState::Awaiting;

    self.blink_until_key(renderer, CURSOR_COL, CURSOR_ROW, |scene| {
        scene.ring.push(Event::SceneTransition {
            from: Phase::Awaiting,
            to: Phase::Booting,
            timestamp_ms: scene.clock_ms,
        });
        scene.phase = Phase::Booting;
    });
}
```

**`run_parked` becomes** (after the existing setup that
draws SYSTEM ONLINE and computes `cursor_col` / `text_row`):

```rust
fn run_parked(&mut self, renderer: &mut Renderer, start_row: usize) {
    let text_row = start_row + 1;
    let cursor_col: usize = SYSTEM_ONLINE_TEXT.len() + 1;
    let cursor_row: usize = text_row;

    renderer.draw_line(SYSTEM_ONLINE_TEXT, text_row);

    // ... existing repaint_state lift logic for Parked ...

    self.blink_until_key(renderer, cursor_col, cursor_row, |scene| {
        scene.ring.push(Event::SceneTransition {
            from: Phase::Parked,
            to: Phase::Parked,
            timestamp_ms: scene.clock_ms,
        });
    });
}
```

Note `run_parked`'s closure does NOT mutate `scene.phase`
because Parked → Parked is the terminal transition that
exits to ACPI shutdown.

**Constraints:**

- Generic `FnOnce` (not `Box<dyn FnOnce>`) to avoid heap
  allocation in the closure path.
- The `repaint_state` lift logic in `run_parked` (the
  match-on-`BootingPost` block) stays *outside* the
  closure — it must run before `blink_until_key` so the
  parked snapshot is correct from the moment the helper
  starts blinking.
- Confirm by inspection that the closure captures only
  `Self`-internal state via the `&mut Self` parameter; it
  must NOT close over any local in `run_awaiting` or
  `run_parked` outside what the closure parameter
  provides. This keeps the borrow check straightforward.
- After the change, `run_awaiting` should be ~10 lines
  and `run_parked` ~20 lines (the latter has the parked-
  snapshot lift). The blink loop itself is now a single
  helper.

**Smoke test:** `make screenshot` must still drain 59
events. The behaviour is byte-identical pre/post; the
refactor is structural.

### 2d — `debug_assert!` on countdown digit width

**File:** `src/bootloader.rs`.

In `bootloader::run_timeout`, find the function start.
Add at the top:

```rust
debug_assert!(
    TIMEOUT_COUNTDOWN_S <= 99,
    "run_timeout's countdown uses two cells; a three-digit \
     value would silently render garbage in the third cell"
);
```

Inline rationale comment is the assertion's failure
message. Visible to a future reader who tries to bump the
constant.

### 2e — Tighten visibility

**File:** `src/scene.rs`.

Find the two `pub(crate) fn` declarations:

- `try_handle_mode_key` (around line 776)
- `cycle_modes` (around line 675)

Change both to plain `fn`.

Verify by `grep -rn` from the repo root that neither name
appears outside `src/scene.rs` (other than archival
mentions in plan files, which is fine).

### 2f — `RepaintState` doc comment

**File:** `src/scene.rs`.

Find the `RepaintState` enum (around line 220, after the
existing `BOOT_SCRIPT_POST` static).

Add a brief module-level intent doc, e.g.:

```rust
/// Snapshot of what is currently on screen, sufficient for
/// `Scene::repaint` to reconstruct the visible content
/// after a runtime mode switch.
///
/// Add one variant per scene runner. Carry just enough
/// state — script index, row, etc. — for the repainter
/// to replay the correct prefix; resist storing data that
/// repaint does not actually consume. Variant fields drift
/// fastest; keep them lean.
```

Replace any existing doc comment if there is one (likely
brief from Phase 1 of display-mode-keystrokes); merge
content rather than duplicating.

## Exit criteria

- [ ] `PACE_LINE_MS` is declared once in `src/`, in
      `src/scene.rs` as `pub(crate)`. `bootloader.rs`
      imports it via `use crate::scene::PACE_LINE_MS;` (or
      the existing combined `use` line).
- [ ] `Renderer::blit_glyph_bytes` is a private helper;
      `draw_glyph` and `draw_cursor_glyph` are wrappers
      around the helper plus their own bounds-check
      guards.
- [ ] `Scene::blink_until_key` exists with the signature
      `<F: FnOnce(&mut Self)>(&mut self, &mut Renderer,
      usize, usize, F)`. `run_awaiting` and `run_parked`
      are two-to-three-line stubs calling the helper.
- [ ] `bootloader::run_timeout` carries the
      `debug_assert!(TIMEOUT_COUNTDOWN_S <= 99)`.
- [ ] `Scene::try_handle_mode_key` and `Scene::cycle_modes`
      are private (`fn`, not `pub(crate) fn`).
- [ ] `RepaintState` carries the new doc comment.
- [ ] `make screenshot` produces the same 59-event
      transcript as Phase 1's closeout baseline (commit
      `3385ca1`).
- [ ] `pre-commit run --all-files` exits 0 at every
      commit.
- [ ] Commit messages follow project conventions.

## Risks

- **`blink_until_key`'s closure capture.** A poorly-
  written closure could try to capture `renderer` (which
  is a separate `&mut` parameter to `blink_until_key`)
  and trigger a borrow-check error. Both example closures
  in the brief above only mutate `scene` (passed as the
  closure argument), avoiding this. Confirm at commit
  time that neither closure touches `renderer` after the
  helper is called.
- **`run_parked`'s repaint-state lift inside vs outside
  the helper.** The lift must stay outside, before
  `blink_until_key`. Verify the order by reading the
  resulting `run_parked` carefully.
- **`try_handle_mode_key` visibility tightening might
  break a future hypothetical caller.** No current
  callers exist outside `scene.rs`; if a future scene
  needs the dispatcher externally, change the visibility
  back. Reversible.
- **`PACE_LINE_MS` re-export forces `bootloader.rs` to
  depend on `crate::scene`.** It already depends on
  several of `scene`'s exports (`poll_key`, `stall`,
  `POLL_MS`), so no new dependency. Acceptable.

## Back brief

Confirm the closure shape (`F: FnOnce(&mut Self)`,
generic, no `Box`), confirm `bootloader.rs` is otherwise
unchanged (no behaviour change to the locked-bootloader
scene), and confirm `make screenshot` still drains 59
events post-phase.
