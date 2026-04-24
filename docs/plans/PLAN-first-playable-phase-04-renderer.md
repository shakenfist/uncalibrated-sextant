# First playable — phase 4: Renderer foundation

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read `DESIGN.md` — especially
*Design principles* (principle 6 on per-glyph / per-tile draws
is load-bearing here), *Aesthetic direction*, and the *Boot
sequence (first scene)* section that this phase is building the
primitives for. Also re-read the master plan and the Phase 1-3
plans so you understand the existing Cargo / Docker / style
infrastructure; none of that changes in Phase 4.

This phase is the first one that writes substantive Rust code
beyond "print a banner". We introduce a renderer module that
owns GOP framebuffer access, an embedded bitmap font, per-glyph
BitBlt, a fixed phosphor-on-black palette, and a line-oriented
text layer with dot-leader columns. Phase 5 will drive it with
the actual boot-sequence content (narrator leaks, cursor glitch,
AWAITING screen); Phase 4 stops at "the scaffold renders
legibly, in the right colour, with the right pacing".

**Scope discipline.** Resist adding narrative content, the
CRT cursor glitch, or any scene orchestration here. Every one
of those has a clear home in Phase 5. If a design question
about narrator voice / glitch behaviour / scene sequencing
surfaces mid-Phase-4, capture it in *Open questions* and move
on.

Cross-repo and in-repo references:

- [DESIGN.md](../../DESIGN.md) — the design contract for the
  renderer. Principle 6 (per-glyph BitBlt beats monolithic
  framebuffer memcpy) directly constrains implementation.
- `uefi` crate 0.37 docs —
  `uefi::proto::console::gop::GraphicsOutput`,
  `uefi::boot::locate_handle_buffer` /
  `uefi::boot::open_protocol_exclusive`, and the Blt operations.
- `shakenfist/uefi-latency-guest/efi.c` — C-based prior art
  that already does GOP framebuffer access against OVMF in
  this workspace. Language mismatch but the firmware
  negotiation and pixel-format handling patterns are directly
  applicable.
- Public-domain / permissively-licensed 8x16 bitmap font
  sources — see *Open questions*.

All planning documents go in `docs/plans/`.

## Situation

Phase 3 landed at commits `b5cebc5` (pre-commit + check-rust),
`f2c4ad3` (CI workflow), and `4097c5e` (docs). The repo now
builds, boots under QEMU, produces release artifacts, and
enforces rustfmt / clippy / shellcheck / gitleaks on every
commit.

`src/main.rs` is still the Phase 1 single-file skeleton: init,
clear the SimpleTextOutput stdout, print a two-line banner,
wait for a keypress, issue an ACPI shutdown. That file gets
replaced (or substantially extended) in this phase to
exercise the new renderer.

No Rust code exists for graphics access. The Phase 2 boot
path is verified to reach `BOOTX64.EFI` and print the banner
via SimpleTextOutput, which is firmware-rendered text on
whatever text-mode the firmware has set up. That is not what
we want for the rest of the project — we need framebuffer
pixel access so the boot sequence can render glyphs, a cursor,
scruff overlays, and eventually a game scene.

## Mission and problem statement

Build a small renderer module that:

- Acquires a writable linear framebuffer from GOP, records its
  mode (pixel format, width, height, stride / pixels-per-
  scanline).
- Embeds a single public-domain or permissively-licensed
  8x16 monochrome bitmap font (ASCII only for now; cp437
  optional if the chosen source provides it cleanly).
- Exposes a `draw_glyph(char, col, row)` that issues one GOP
  `Blt` BufferToVideo operation per glyph, so the display
  channel sees one bitmap op per glyph drawn (principle 6).
- Exposes a `draw_line(text, row)` that lays out ASCII text at
  a given text row, one call per glyph.
- Supports dot-leader columns: given `("DISPLAY SUBSYSTEM",
  "OK", row)`, render `DISPLAY SUBSYSTEM .......... OK` with
  the dots filling to a target column, and emit the dots as a
  sequence of per-glyph BitBlts (principle 6 again — not a
  single fat "dots" memcpy).
- Has a fixed palette: phosphor foreground on black
  background. Colour choice resolved by Step 1 open question.
- Uses `uefi::boot::stall` for line pacing.

The phase ends when `src/main.rs`, driving the new renderer,
can display three to five test lines with legible glyphs,
phosphor colour, correct dot-leader alignment, and
perceptible pacing. A keypress still triggers the Phase 2
ACPI shutdown. No narrator leaks, no scruff, no AWAITING
screen, no mode walk — those are Phase 5 and Phase 6.

`pre-commit run --all-files` must still pass.

## Open questions

- **Phosphor colour — green or amber?** The master plan defers
  this to Phase 4. Green (VT100 / Pip-Boy) is the broader
  cultural fit given DESIGN.md's Fallout lean; amber (IBM 5151)
  is more 1970s mainframe and arguably more readable at small
  sizes. Default: **green**, specifically a desaturated P1-style
  `rgb(51, 255, 51)` foreground on pure black. Revisit at Step 2
  if the rendered result feels wrong; amber can be a later
  theme variant under one config constant.
- **Font source and license.** Several plausible options:
  - `spleen` (BSD-2-Clause) — modern bitmap font with 8x16
    available. Simple to vendor in.
  - `cp437` / "VGA" 8x16 fonts — the IBM PC-era glyphs are
    in broad public-domain circulation (the shape of a
    character is not copyrightable, and the 8x16 pixel
    patterns have been redistributed for decades). Many
    clean sources exist; pick one that clearly states PD.
  - Linux kernel console fonts (`drivers/video/console/`) —
    GPL-licensed, therefore **not compatible** with the
    Apache-2.0 crate licence. Skip.
  Decide at Step 1 after a brief scan; embed under
  `src/renderer/font.rs` with a license header comment
  citing the source. Do not introduce a new SPDX tag in
  `Cargo.toml` unless absolutely required.
- **Target GOP mode.** OVMF typically offers 640x480, 800x600,
  and 1024x768. The full *mode walk* discussed in DESIGN.md
  is a Phase 5/6 feature. For Phase 4, pick a single working
  mode at startup (preferably 1024x768 if offered) and render
  into it. If mode-switching proves fragile, fall back to
  using whatever mode GOP has active on entry. Decide at
  Step 1.
- **Module layout.** Single `src/renderer.rs`, a
  `src/renderer/mod.rs` plus submodules, or a separate
  `uncalibrated-sextant-renderer` crate in a Cargo workspace?
  Default: **single-file `src/renderer.rs`** for Phase 4;
  split into submodules when it exceeds ~500 lines or when a
  second consumer appears. A workspace is premature.
- **Pixel format handling.** GOP's `Blt` operation with a
  `BltPixel` buffer abstracts over the underlying pixel format
  and the firmware does the conversion. Prefer this over
  direct linear-framebuffer writes. Confirm at Step 1 that the
  uefi 0.37 API exposes `Blt` cleanly for this pattern.
- **Cursor blink.** DESIGN.md's Boot sequence section calls
  for a blinking cursor at end-of-line and on the AWAITING
  screen. That mechanism — timer loop, glyph variant substitution,
  character-ROM glitching — is Phase 5's minimum-viable glitch.
  Phase 4 does **not** implement cursor blink. It does leave
  the renderer's line layout aware of "where the cursor would
  sit", so Phase 5 can add the blink on top without disturbing
  layout.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | medium | opus   | none      | Research GOP API in uefi 0.37, find a font, confirm mode-switching works, sanity-check principle-6 implications. Report back. See Step 1 below. |
| 2    | high   | sonnet | none      | Implement `src/renderer.rs` (framebuffer, font, palette, per-glyph BitBlt, dot-leader layout) and integrate into `src/main.rs` with a small test scaffold. See Step 2 below. |
| 3    | low    | sonnet | none      | `make qemu` to verify the rendered output — legible font, correct colour, correct alignment, visible pacing. Management session (with operator input) confirms visually. See Step 3 below. |
| 4    | low    | sonnet | none      | Update `README.md`, `ARCHITECTURE.md`, `AGENTS.md` to describe the new renderer module. Keep it terse. See Step 4 below. |

### Step 1 — research and decide

**Confirm:**

- `uefi 0.37` API surface for GOP:
  - How to locate and open the protocol — likely
    `uefi::boot::get_handle_for_protocol::<GraphicsOutput>()` +
    `uefi::boot::open_protocol_exclusive` — confirm exact
    calls against docs.rs.
  - `GraphicsOutput::query_mode`, `set_mode`, `current_mode_info`
    (or equivalents) — what do they return in 0.37?
  - `GraphicsOutput::blt` signature: how do you pass a
    `BufferToVideo` source? Is the buffer `&[BltPixel]` or
    something else?
  - `BltPixel` definition and whether 0.37 provides a
    convenient constructor.
- Does opening GOP as `ExclusiveProtocol` from an entry-point
  UEFI app cause any trouble against OVMF? (It should not —
  OVMF's console driver releases GOP when the app runs.)
- Read `shakenfist/uefi-latency-guest/efi.c` for concrete
  sequencing and any OVMF-specific gotchas.

**Decide (record in Step 1 report):**

- Phosphor colour. Default green unless the agent has a
  concrete reason otherwise.
- Font source. Include the URL and licence text. The font
  should be 8x16, ASCII at minimum, distributable under a
  licence compatible with Apache-2.0.
- Mode strategy. Either "attempt 1024x768 with fallback to
  current mode", or "use current mode" — pick the simpler
  that works.

**Output:** a tight paragraph-per-topic report to the
management session. No file changes.

### Step 2 — implement the renderer

Create `src/renderer.rs` exporting (at minimum):

- `pub struct Renderer` owning the GOP framebuffer
  information (width, height, pixels per scanline, pixel
  format).
- `pub fn Renderer::new() -> Renderer` which acquires GOP,
  switches to the chosen mode (if the Step 1 decision is to
  mode-switch), clears the screen.
- `pub fn Renderer::draw_glyph(&mut self, ch: char, col: u32,
  row: u32)` which resolves the glyph bitmap from the font
  table and issues a single GOP `blt` BufferToVideo call at
  the target pixel coordinates (col * 8, row * 16). Unknown
  chars render as `?` or a blank cell — either is fine.
- `pub fn Renderer::draw_line(&mut self, text: &str, row:
  u32)` which calls `draw_glyph` once per character.
- `pub fn Renderer::draw_telemetry_line(&mut self, label:
  &str, status: &str, row: u32)` which lays out `label`,
  fills with dot leaders to a target column (say column 40),
  then draws `status`. Uses `draw_glyph` under the hood (dots
  are glyphs too — principle 6). The target column should be
  a `const` in the renderer module so we can tune it in one
  place.

And a bundled font under a submodule or adjacent `font.rs`:

- `src/renderer/font.rs` (if going the submodule route) or
  `src/font.rs` (if the renderer stays single-file) carrying
  the 8x16 bitmap data as a `const FONT_8X16: [[u8; 16];
  128]` (ASCII) or `[[u8; 16]; 256]` (if cp437), and a short
  license header comment citing the source.
- If the font needs a separate licence file, place it under
  `LICENSES/FONT_<name>.txt` at the repo root. Keep the
  top-level `LICENSE` file unchanged.

And a palette constant (foreground / background `BltPixel`s).

Update `src/main.rs` to:

1. Build a `Renderer` after `uefi::helpers::init()`.
2. Call a test scaffold that renders three to five lines via
   `draw_telemetry_line`. Example lines (placeholder content;
   not the real boot sequence yet):

   ```
   DISPLAY SUBSYSTEM ................ OK
   POINTER ............................ OK
   KEYBOARD ........................... OK
   ```

3. Stall briefly between lines for pacing (e.g. 150 ms).
4. After the last line, `wait_for_event` on a keypress, then
   ACPI shutdown exactly as today.

The Phase 1 SimpleTextOutput banner is no longer used; the
rendering path is entirely GOP-based.

Make sure `pre-commit run --all-files` still passes
(including clippy). The renderer code is going to run into
clippy lints the existing skeleton did not; fix them in
place, don't add broad `#[allow(...)]`s.

### Step 3 — visual verification

Run `make qemu`. Expected:

- GTK window opens, firmware boots, our binary takes over.
- The screen switches to the chosen GOP mode (if we mode-
  switch) and clears to black.
- Three to five lines of phosphor-green-on-black text appear,
  paced with visible delay between them.
- Font is legible and not mangled (right glyph shapes, right
  size, dot leaders actually line up).
- Keypress triggers ACPI shutdown; QEMU exits cleanly.

**Do not skip the visual check.** Legibility, colour fidelity,
and alignment are judgments a human has to make. The sub-
agent runs `make qemu` and describes what should happen; the
operator confirms what they see. If the rendered output is
wrong in a way the sub-agent cannot see (e.g. wrong colour
that still encodes as phosphor in code), report back with
screenshots or a specific description.

Capture the firmware serial log too, as in Phase 2. Include it
in the Step 3 report.

### Step 4 — documentation

Update:

- **`README.md`** — one or two sentences in the Status section
  acknowledging that Phase 4 has landed and the binary now
  renders via GOP. No need for a whole new section.
- **`AGENTS.md`** — update *Current phase*; add a short pointer
  under *Where to read first* to the new renderer module.
- **`ARCHITECTURE.md`** — add a paragraph describing the
  renderer shape: single-file `src/renderer.rs`, GOP Blt-based
  per-glyph drawing, embedded 8x16 font, phosphor palette,
  telemetry-line layout with dot leaders. Note the font's
  licence and source briefly.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. Four
phase-specific emphases:

- **Principle 6 is not negotiable.** `draw_line` and
  `draw_telemetry_line` must issue one `blt` per glyph, not
  compose into an in-memory framebuffer buffer and blit once.
  Review the generated code (or have the sub-agent show you)
  before accepting.
- **The font is data, not logic.** Keep it in one
  const array, keep its licence header close to it, and do
  not rewrite the glyph data. Lift verbatim from the chosen
  source.
- **clippy will have more to say now.** A bigger Rust surface
  means more lint coverage. Fix findings rather than
  allow-listing them. If a lint is genuinely wrong for
  `no_std` / UEFI idioms, narrow-scope the `#[allow(...)]` and
  comment why.
- **Step 2 is high effort.** The GOP negotiation, pixel-format
  handling, and font integration interact in ways that have
  bitten every UEFI-rendering tutorial. Budget for one cycle
  of "it compiles but the screen is wrong" before the visual
  check passes in Step 3.

## Success criteria

Phase 4 is complete when:

- [ ] `make qemu` opens a QEMU window, the screen shows
      three to five test lines rendered via GOP, in phosphor
      green on black, with correct dot-leader alignment and
      visible line pacing.
- [ ] A keypress triggers ACPI shutdown; QEMU exits.
- [ ] `make release` still produces bootable artifacts that
      behave the same.
- [ ] `pre-commit run --all-files` exits 0.
- [ ] The renderer source lives in `src/renderer.rs` (or
      `src/renderer/`), font data with licence header lives
      next to it, and `src/main.rs` is the orchestration
      layer.
- [ ] `README.md`, `ARCHITECTURE.md`, `AGENTS.md` describe the
      new renderer.

## Bugs fixed during this work

(None yet — populated during execution.)

## Future work

- Mode-walk exercise (DESIGN.md *Boot sequence*) iterating
  through every resolution OVMF offers to stress the SPICE
  display-channel renegotiation path. Phase 5 or 6.
- Amber phosphor palette variant (or user-configurable
  theme) — trivially a different palette constant once the
  renderer is in place.
- cp437 box-drawing glyphs for later UI work if the initial
  font source happens not to include them.
- A GOP "direct framebuffer" fallback mode for performance
  testing, bypassing Blt. Probably not needed, but a useful
  contrast point for principle-6 verification.
- Proportional or variable-width fonts. Almost certainly never
  — fixed-cell monospace is the aesthetic.
- Double-buffering. Not needed for now; UEFI GOP Blt
  operations are already atomic from the firmware's
  perspective.
