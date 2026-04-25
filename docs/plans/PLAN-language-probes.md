# Language probes — establish English-is-no-longer-default

Standalone plan, single phase. Lands before
[PLAN-locked-bootloader.md](PLAN-locked-bootloader.md).

## Post-landing note

Shipped in three commits (bb41e70, 288a839, c67d70a) following
the design below, then revised in a follow-up: the Spanish and
English probe lines were moved out of the bitmap path and onto
the existing spleen `SceneStep::Telemetry` path. Spleen is
ASCII-only, so the Spanish text was changed from
`Detectando soporte para español` to
`Detectando soporte para castellano` (the formal, accent-free
name of the Spanish language). This eliminated a visible font
mismatch between the Latin probes (Unifont) and the rest of the
boot transcript (spleen). `scripts/vendor-language-probes.py`
now only emits Mandarin and Hindi bitmaps. The AWAITING screen
was simultaneously stripped of its English `AWAITING OPERATOR`
text and reduced to a lone blinking cursor — diegetically, the
system has not yet probed for language support and so cannot
prompt in any specific language.

## Prompt

Before working on this plan, re-read the *Voice* and *Boot
sequence* sections of [DESIGN.md](../../DESIGN.md), the Phase
4 / Phase 5 paragraphs of [ARCHITECTURE.md](../../ARCHITECTURE.md)
(in particular how `Renderer::draw_logo` tiles a packed
1-bit-per-pixel bitmap as a grid of 8×16 cells), and the
master plan [PLAN-first-playable.md](PLAN-first-playable.md)
for tone and execution conventions. Skim
[scripts/vendor-logo.py](../../scripts/vendor-logo.py) — the
language-probe vendor script is structurally a sibling.

Cross-refs in-repo:

- `src/renderer/mod.rs` — owns `Renderer`. `draw_logo` is the
  prior-art per-cell bitmap blit and the natural template for
  the new `draw_text_bitmap` helper.
- `src/renderer/font.rs` — the spleen 8×16 ASCII table,
  unchanged by this plan. Dots and (per design choice below)
  English status text continue to render via this table.
- `src/scene.rs` — owns `BOOT_SCRIPT`. The new probe lines
  insert at the top.
- `scripts/vendor-logo.py` — model for the new vendor script.

All planning documents go in `docs/plans/`.

## Situation

The first-playable milestone has shipped a UEFI binary that
plays an opening boot sequence in spleen 8×16 ASCII. The
project's voice has so far been entirely English, and the
fictional system reads as "an English-language diagnostic
console" by default. We want to subvert that default in the
first four lines of the boot sequence by showing the system
probing for language support across several scripts and
finding only English, establishing in worldbuilding terms
that *English is not the default in this universe* — the
system is reaching for everything else first.

The renderer already has the right primitive for this:
`draw_logo` blits a packed 1-bit-per-pixel bitmap as a grid
of 8×16 cells, one cell per `BltOp::BufferToVideo` call
(principle 6 honoured). All we need is a vendor script that
rasterises arbitrary text in arbitrary scripts to the same
packed format, and a small renderer helper that draws it at
a given text-cell position.

This is a small side quest before the locked-bootloader
milestone. Worldbuilding-only — no new mechanics, no Ryll-
side anything, no new SPICE tests.

## Mission and problem statement

Add four lines to the very top of `BOOT_SCRIPT` that read,
in order:

1. *"Probing for Mandarin support"* in idiomatic Mandarin
   ........ *FAILED* (in idiomatic Mandarin)
2. *"Probing for Hindi support"* in idiomatic Hindi
   ........ *FAILED* (in idiomatic Hindi)
3. *"Probing for Spanish support"* in idiomatic Spanish
   ........ *FALLO* (or whatever a Spanish BIOS would say)
4. *"Probing for English support"* in English
   ........ *OK*

The labels and statuses for the non-Latin lines render via
pre-vendored bitmaps; the dots between them and the layout
mechanics use the existing per-glyph ASCII path — the
**hybrid** layout we settled on. Status column-alignment with
the rest of the boot transcript is preserved (status bitmaps
left-align at the same cell column as ASCII status fields in
the existing telemetry lines).

Done when an operator running `make qemu` sees the four lines
render at the top of the boot sequence in their respective
scripts, the dots column-align, the status field column-aligns
with the rest of the transcript, and the screenshot at
`docs/images/boot-sequence.png` is regenerated to match.
Pre-commit clean.

## Open questions

- **Font.** **Default: GNU Unifont** (`fonts-unifont`,
  SIL OFL, 16-pixel bitmap font covering essentially all of
  Unicode). Single dep, bitmap idiom, matches the spleen
  aesthetic naturally. Per-script alternatives (Droid Sans
  Fallback for CJK is already on the host; `fonts-lohit-deva`
  for Devanagari) would give sharper glyphs at the cost of a
  multi-font script and hinted-vs-bitmap visual inconsistency.
  Revisit during visual verification if Unifont's CJK or
  Devanagari rendering is too faint.
- **Render English label as bitmap or ASCII?** **Default:
  bitmap.** All four labels through the same path means
  uniform visual treatment ("the system rendered each probe
  the same way"). If Unifont's Latin glyphs read as visibly
  different from spleen and that bothers the operator,
  trivially switch the English line to ASCII via the existing
  `SceneStep::Telemetry` path with no other code changes.
- **Translate the status field too?** **Confirmed yes.**
  Otherwise the worldbuilding leaks ("the system speaks every
  language but its status words are always English"). Each
  probe line carries its own translated status. English keeps
  `OK`.
- **Translation accuracy.** Translations land as best-effort
  by-eye rough cuts; native-speaker pass tracked as Future
  work. Target audience is English speakers who likely cannot
  read these scripts; small mistakes are low-stakes.
- **Position in boot sequence.** **Default: top, before
  `REMOTE LINK: serial @ COM2`.** Language detection is the
  first probe a system does; the operator sees the failures
  and the English-wins line as the opening beat. The current
  first line becomes the second.
- **What happens between this side quest and locked-
  bootloader.** Land this first; locked-bootloader inserts
  later in the sequence so there's no overlap. Both can
  coexist in `BOOT_SCRIPT` without interaction.

## Suggested translations (rough; native-speaker pass deferred)

| Lang | Label | Status |
|------|-------|--------|
| Mandarin (zh) | `检测中文支持` | `失败` |
| Hindi (hi) | `हिन्दी समर्थन की जाँच` | `विफल` |
| Spanish (es) | `Detectando soporte para español` | `FALLO` |
| English (en) | `Probing for English support` | `OK` |

These are starters. The vendor script reads them from a small
config block at the top of the file; updating later is a one-
line edit + `make vendor-probes` (see Step 1).

## Execution

Single phase, three steps. No sub-agent breakdown — small
enough for direct execution.

### Step 1 — vendor script and generated bitmaps

Add `scripts/vendor-language-probes.py`. Hard-code the eight
strings (four labels + four statuses) from the table above
at the top of the file. Use ImageMagick + GNU Unifont:

```
convert -font /usr/share/fonts/truetype/unifont/unifont.ttf \
        -pointsize 16 +antialias \
        -background white -fill black \
        label:'<text>' \
        -depth 1 -threshold 50% \
        out.pbm
```

Parse the PBM header to discover (width, height) of each
rendered string, pack rows MSB-leftmost into bytes, emit as
Rust constants in `src/probes.rs`:

```rust
pub const PROBE_MANDARIN_LABEL_BITMAP: [u8; N] = [...];
pub const PROBE_MANDARIN_LABEL_WIDTH_PX: usize = ...;
pub const PROBE_MANDARIN_LABEL_HEIGHT_PX: usize = 16;

pub const PROBE_MANDARIN_STATUS_BITMAP: [u8; N] = [...];
pub const PROBE_MANDARIN_STATUS_WIDTH_PX: usize = ...;
pub const PROBE_MANDARIN_STATUS_HEIGHT_PX: usize = 16;

// ...likewise for Hindi, Spanish, English
```

Width may not be a multiple of 8; pad each row to the next
byte boundary (the renderer ignores the trailing bits since
it only reads `width_px / 8` plus possibly one tail byte).

Add a `make vendor-probes` target invoking the script so
regeneration is one command. Document the apt prerequisite
(`apt install fonts-unifont`) in `AGENTS.md`'s build commands
section.

Run the script, commit `src/probes.rs` alongside the script
itself for reproducibility (per the same pattern as
`src/logo.rs`). End-of-step.

### Step 2 — renderer extension and scene insertion

Add `Renderer::draw_text_bitmap`:

```rust
pub fn draw_text_bitmap(
    &mut self,
    bitmap: &[u8],
    width_px: usize,
    height_px: usize,
    col: usize,
    row: usize,
)
```

Same shape as `draw_logo` — tile as 8×16 cells, one
`BltOp::BufferToVideo` per cell. The natural refactor is to
have `draw_logo` call `draw_text_bitmap` (the only difference
is the call site naming).

Add a new `SceneStep::Probe` variant:

```rust
Probe {
    label_bitmap: &'static [u8],
    label_width_px: usize,
    status_bitmap: &'static [u8],
    status_width_px: usize,
}
```

Add a render method `Renderer::draw_probe_line(probe, row)`
that does the hybrid layout:

1. Draw `label_bitmap` at column 0 via `draw_text_bitmap`.
2. Compute `label_end_cell = (label_width_px + CELL_W - 1) /
   CELL_W` (round up).
3. Draw ' ' at `label_end_cell`, then dots at cells
   `label_end_cell + 1` through `DOT_LEADER_COL - 1`, all via
   the existing per-glyph ASCII path (preserves principle 6
   and matches `draw_telemetry_line`'s dot-leader exactly).
4. Draw `status_bitmap` at column `DOT_LEADER_COL + 1` via
   `draw_text_bitmap`. Status bitmaps are left-aligned at
   that column; their rendered width can extend beyond the
   far-right column without wrapping (Unifont CJK at 16 px
   wide × 2 chars = 32 px = 4 cells, fits comfortably).

Insert four `SceneStep::Probe` entries at the top of
`BOOT_SCRIPT` referencing the eight constants from
`src/probes.rs`. The existing `REMOTE LINK: ...` row becomes
row 4 instead of row 0; the rest of the script shifts down
unchanged.

Wire `SceneStep::Probe` into `Scene::run_booting`'s match arm
alongside `Telemetry` and `Line`. Pacing same as existing
lines (`PACE_LINE_MS`).

Compile and `pre-commit run --all-files`. End-of-step.

### Step 3 — visual verification, screenshot, docs

Run `make qemu`. Walk the operator through the new opening
beats:

- All four lines render in their expected scripts.
- The dot leaders end at column 40 across all four lines
  (visual column alignment with the existing transcript
  below).
- The status fields left-align at column 41.
- Unifont's Latin glyphs in the English line are acceptable
  next to the rest of the spleen-rendered transcript — if
  not, switch English back to `SceneStep::Telemetry`.
- The cursor glitch and rest of the boot sequence still
  behave as before.

Regenerate the reference screenshot:

```
make screenshot
```

Confirm the new four lines appear at the top of
`docs/images/boot-sequence.png` and commit the updated PNG.

Documentation updates:

- `README.md`: Status section gains a sentence about the
  language-probe opening beat.
- `ARCHITECTURE.md`: insert a short paragraph after the
  Phase 5 logo paragraph describing the language probes —
  what they are, where they live (`src/probes.rs`,
  `Renderer::draw_text_bitmap`), and why (worldbuilding:
  English is not the default).
- `AGENTS.md`: add `src/probes.rs` and
  `scripts/vendor-language-probes.py` to the where-to-read-
  first list, mention `make vendor-probes` and the
  `fonts-unifont` apt prerequisite.
- `docs/plans/index.md`: move this plan from Standalone /
  Not started to Standalone / Complete with commit SHAs.

End of phase / end of plan.

## Success criteria

This plan is complete when:

- [ ] `make qemu` shows four language-probe lines at the top
      of the boot sequence, three FAILED in non-Latin
      scripts and English OK.
- [ ] The dot leaders end at column 40 and the status fields
      left-align at column 41 across all four lines, matching
      the existing telemetry layout below.
- [ ] `src/probes.rs` is committed alongside the vendor script
      and is byte-for-byte reproducible from a fresh
      `make vendor-probes` run on a host with `fonts-unifont`
      installed.
- [ ] `docs/images/boot-sequence.png` is regenerated to show
      the new four lines.
- [ ] `pre-commit run --all-files` exits 0.
- [ ] `make release-verify` and `make screenshot` still pass.
- [ ] `README.md`, `AGENTS.md`, `ARCHITECTURE.md` describe
      the probes module, vendor script, and apt prerequisite.

## Future work

- **Native-speaker translation pass.** Mandarin / Hindi /
  Spanish strings should be reviewed by speakers and adjusted
  for idiom and BIOS register before any wide demo or release.
- **More languages.** The vendor script extends naturally to
  Arabic (RTL — would need a small renderer awareness for
  bidi, or just bake direction into the bitmap), Japanese,
  Russian, Greek. Each adds one row to the top of
  `BOOT_SCRIPT`.
- **Status word convention research.** Real BIOSes from
  multilingual vendors sometimes keep `OK` / `FAILED` as
  English even on localised systems. Worth checking what
  Lenovo / HP localised BIOSes do, for verisimilitude. Could
  flip back to English-status as a deliberate choice if
  research confirms it's the period-correct convention.
- **Per-script font selection.** If Unifont's coverage of one
  script is too coarse, fall back to a per-script font
  (Droid Sans Fallback for CJK, Lohit for Devanagari, etc.)
  in the vendor script. Adds dependency surface; defer until
  a specific script demands it.

## Bugs fixed during this work

(None yet — populated during execution.)

## Documentation index maintenance

On creation, `docs/plans/index.md` gains a Standalone-plans
row for this plan; `docs/plans/order.yml` gains a
`PLAN-language-probes.md: Language probes` entry. On
completion, the index row moves to Complete with commit SHAs.

## Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the
work you intend to do aligns with that plan.
