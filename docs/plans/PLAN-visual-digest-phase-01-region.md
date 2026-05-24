# Visual on-screen digest — phase 1: region and QR encoder

Parent plan:
[PLAN-visual-digest.md](PLAN-visual-digest.md).

## Outcome

**Status: Not started.**

## Prompt

Before working on this phase, re-read the master plan's
*Phase 1 sketch* and the relevant *Open questions* it points
to (QR-vs-text, crate choice, region location, host decode
tooling). Skim:

- [`src/renderer/mod.rs`](../../src/renderer/mod.rs) — the
  `Renderer` struct and its existing per-glyph BltOp call
  sites (`blit_glyph_bytes`, `draw_text_bitmap`). The new
  `draw_digest` method goes here.
- [`src/scene.rs`](../../src/scene.rs) — `draw_chrome` for the
  logo placement, `MODE_KEYS` for the supported GOP modes,
  `run_awaiting` and `run_parked` for parking-screen patterns.
- [`src/logo.rs`](../../src/logo.rs) — confirms
  `LOGO_WIDTH = LOGO_HEIGHT = 128` placed at top-right.
- [`Cargo.toml`](../../Cargo.toml) — current
  `uefi = "=0.37.0"` pin with `alloc` + `global_allocator`.
  Phase 1 adds exactly one new dependency.
- [`Makefile`](../../Makefile) and
  [`scripts/screenshot.sh`](../../scripts/screenshot.sh) —
  the QMP `screendump` path that the smoke target re-uses.

External references:

- [`qrcodegen`](https://crates.io/crates/qrcodegen) — chosen
  crate per the master plan default. v1.x is the stable line;
  the `no_std` build path needs `default-features = false`.
- [`qrcode`](https://crates.io/crates/qrcode) — fallback if
  `qrcodegen` drags in incompatible dependencies.
- [`zbarimg`](https://manpages.debian.org/bookworm/zbar-tools/zbarimg.1.en.html)
  — host-side decoder for the smoke target.

This phase plans at **medium effort overall (sonnet)**. The
region geometry math is the only design call; the rest is
mechanical.

## Goal

Prove we can lay a scannable QR code into a chosen region of
the framebuffer, at every GOP mode the harness supports, with
a hard-coded payload. By the end of this phase:

- A new dependency (default: `qrcodegen`) is in `Cargo.toml`
  and builds clean under `make build`, clippy, and rustfmt.
- `DIGEST_*` pixel constants exist in `src/renderer/mod.rs`
  alongside `MARGIN_X` / `MARGIN_Y`, with a `const _: () =
  assert!(…)` proving the chosen QR Version fits in the
  smallest supported mode (640×480) without colliding with
  the logo, AWAITING cursor, or the bottom-row toast.
- `Renderer::draw_digest(payload: &[u8])` encodes the
  payload, then renders the QR module-by-module via one
  `BltOp::BufferToVideo` per module (Principle 6).
- A new `make digest-smoke` target boots headless, holds in
  AWAITING with a hard-coded `draw_digest(b"hello")` injection
  gated behind a new `digest-smoke` cargo feature,
  `screendump`s the result to PNG, runs `zbarimg`, and
  asserts the decoded bytes equal `b"hello"`.
- The hard-coded QR is visible across at least three different
  mode keys (`'1'`, `'3'`, `'5'`) under `make spice-ryll`
  (manual verification — no automated assertion across modes
  in this phase; that comes with the Phase 2 repaint
  integration).

Nothing in this phase wires the ring buffer, computes a
framebuffer hash, or integrates with `Scene::repaint`. The
digest is a static pixel pattern with one hard-coded payload.

## Scope

**In scope:**

- One new Cargo dependency for QR encoding, with the
  appropriate no_std feature flag.
- `DIGEST_REGION_X`, `DIGEST_REGION_Y`, `DIGEST_REGION_PX`,
  `DIGEST_MODULE_PX`, and `DIGEST_QR_VERSION` constants (or
  equivalent shape) in `src/renderer/mod.rs`, with compile-
  time fit assertions.
- `Renderer::draw_digest(&mut self, payload: &[u8])`.
- A `digest-smoke` cargo feature that, when enabled, calls
  `draw_digest(b"hello")` from `run_awaiting` after
  `draw_chrome`.
- `make digest-smoke` target: builds with the feature, boots
  headless via the existing screenshot.sh pattern,
  `screendump`s to a PNG, runs `zbarimg`, asserts payload.
- A short `scripts/digest-smoke.sh` driver if the logic
  doesn't fit cleanly inside the Makefile recipe (per
  `~/.claude/CLAUDE.md` — "Do not write large scripts in CI
  workflow steps").

**Out of scope (Phase 2):**

- Ring-buffer payload encoding (TLV wire format).
- Per-scene refresh wiring inside `Scene::run_*`.
- Bootloader-scene carve-out (the digest does not appear in
  the bootloader scene in this phase because it only renders
  while AWAITING, behind the feature gate).
- Framebuffer hash via `BltOp::VideoToBuffer`.

**Out of scope (Phase 3):**

- Repaint integration with `Scene::repaint`.
- `docs/visual-digest-format.md` (no wire format yet — Phase
  2 produces the spec).
- DESIGN.md / ARCHITECTURE.md / AGENTS.md / README.md updates
  beyond a brief mention of the digest region in
  `ARCHITECTURE.md`'s renderer paragraph (deferred to Phase 3
  closeout).

**Deferred from the master plan:**

- **Cross-mode automated smoke.** The master plan's
  *Phase 1 sketch* exit says "a hard-coded QR in the chosen
  region across at least three different mode keys" should
  be verifiable. This phase will verify that *manually* under
  `make spice-ryll`, not as an automated assertion — without
  Phase 2's repaint integration, mode-switch keystrokes wipe
  the digest (`set_mode` invalidates the framebuffer; nothing
  redraws the digest in this phase). The manual cross-mode
  check confirms the region geometry is sound at each size;
  the automated cross-mode regression lands in Phase 3.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | Add `qrcodegen = { version = "1", default-features = false }` (or chosen equivalent) to `Cargo.toml`. Confirm `make build`, clippy, and rustfmt all pass. If `qrcodegen` cannot build no_std against our toolchain, fall back to `qrcode` with the matching feature flag; if neither works, stop and report — do not hand-roll an encoder. |
| 1b   | medium | sonnet | none      | Add `DIGEST_*` constants to `src/renderer/mod.rs` next to `MARGIN_X` / `MARGIN_Y`. Compute the bottom-right region at 640×480, write a `const _: () = assert!(…)` proving (a) the chosen QR Version's module count × scale fits inside the region, (b) the region does not overlap with the toast row, (c) the region does not overlap with the logo when the logo is placed in the top-right. Document the math in a doc-comment. |
| 1c   | medium | sonnet | none      | Implement `Renderer::draw_digest(&mut self, payload: &[u8])`. Encode the payload via the chosen QR crate (use ECC `Medium` as a sane default — survives the future CRT-scruff overlay). Render the bit matrix one module per `BltOp::BufferToVideo` call. Foreground = `FG`, background = `BG`. Quiet zone is handled by the QR library's `border` parameter; render the entire matrix-with-border inside the region. Add a brief doc-comment explaining the per-module-BltOp choice references Principle 6. |
| 1d   | low    | sonnet | none      | Add the `digest-smoke` cargo feature to `Cargo.toml` (no deps gated). Add a `#[cfg(feature = "digest-smoke")]` call to `draw_digest(b"hello")` in `run_awaiting`, after `draw_chrome` and before the blink loop. |
| 1e   | medium | sonnet | none      | Add `make digest-smoke` target that (1) builds with `--features digest-smoke`, (2) reuses the screenshot.sh boot pattern but holds in AWAITING (do not send the space keystroke that advances to the boot script), (3) `screendump`s to a PNG, (4) shells out to `zbarimg` to decode, (5) asserts the decoded payload equals `hello`. If `scripts/screenshot.sh` is not easily parameterisable for the "hold in AWAITING" case, copy the minimum needed into `scripts/digest-smoke.sh` rather than overloading the existing script with flags. |
| 1f   | low    | sonnet | none      | Smoke exit: run `make digest-smoke` to green. Run `make spice-ryll` with the feature flag, manually press `'1'`, `'3'`, `'5'` to confirm the QR is visible (or wiped — see *Deferred* above) at each mode. Capture findings in this plan's *Outcome* section at closeout. Run `pre-commit run --all-files`. |

Commits expected: one per step (1a–1e), with 1f folded into
the 1e commit's verification. Five commits total is fine; six
is acceptable if 1f produces a notable finding.

## Detailed step briefs

### 1a — pick QR crate, vendor it, prove no_std build

**Files:** `Cargo.toml`.

**What to add (default path — `qrcodegen`):**

```toml
[dependencies]
uefi = { version = "=0.37.0", features = ["panic_handler", "alloc", "global_allocator"] }
qrcodegen = { version = "1", default-features = false }
```

**Constraints:**

- Must build against `x86_64-unknown-uefi` with the existing
  toolchain (`rust-toolchain.toml` pins `1.88.0`).
- Must not introduce a `std` requirement transitively.
  `qrcodegen` 1.8 is no_std by default when
  `default-features = false`; confirm via
  `cargo tree -e features` or by inspecting the crate's
  `Cargo.toml` after vendoring.
- Must not require a build-script with host-only deps.
- If the chosen crate fails any of these, fall back to
  `qrcode = { version = "0", default-features = false }`
  with `image` and `svg` features disabled. If `qrcode` also
  fails, **stop and report** — do not hand-roll a QR encoder
  in this step. The master plan flagged hand-rolling as a
  last-resort path; that decision needs management review,
  not a sub-agent autonomous swap.

**Smoke test:**

```sh
make build
cargo clippy --release --target x86_64-unknown-uefi -- -D warnings
cargo fmt --check
```

**Commit message style:** matches recent commits — subject
under 50 chars ending in a period, 2-3 sentence body
explaining the why, `Prompt:` paragraph, `Signed-off-by`,
`Co-Authored-By` with model + context window + effort level.

### 1b — region geometry constants

**Files:** `src/renderer/mod.rs`.

**The math at 640×480:**

- Logo: top-right, `LOGO_WIDTH = LOGO_HEIGHT = 128`, placed
  at pixel coords `x ∈ [496, 624)`, `y ∈ [16, 144)`.
- AWAITING cursor: top-left, `x ∈ [16, 24)`, `y ∈ [16, 32)`.
- Toast row: bottom, `y ∈ [448, 464)`.
- Boot transcript: rows 0–26, `y ∈ [0, 432)`.
- Right margin: `MARGIN_X = 16`.
- Bottom margin: `MARGIN_Y = 16` (but the toast row sits
  inside that margin).

The bottom-right corner — above the toast, right of the logo
exclusion zone (which only matters for the top half) — is
available. Choosing QR Version 5 (37×37 modules) at
4-pixel-per-module scale gives a 148×148 px square. Adding a
4-module border (default `qrcodegen` quiet zone) gives
(37 + 8) × 4 = 180×180 px.

**Default placement at 640×480:**

```text
DIGEST_REGION_PX     = 180         // includes 4-module quiet zone
DIGEST_REGION_X      = 640 - MARGIN_X - DIGEST_REGION_PX = 444
DIGEST_REGION_Y      = 480 - MARGIN_Y - CELL_H - DIGEST_REGION_PX = 268
                       // 16 (margin) + 16 (toast row) = 32 reserved at bottom
DIGEST_MODULE_PX     = 4
DIGEST_QR_VERSION    = 5           // 37×37 modules
DIGEST_QR_MODULES    = 37
DIGEST_QR_BORDER     = 4           // qrcodegen default; the master plan
                                   // accepts the overscan margin as the
                                   // quiet zone, but qrcodegen renders its
                                   // own border into the bit matrix, so the
                                   // border is *part of* DIGEST_REGION_PX,
                                   // not adjacent to it.
```

**Compile-time fit assertions:**

```rust
const _: () = {
    // Region must be a multiple of DIGEST_MODULE_PX.
    assert!(DIGEST_REGION_PX % DIGEST_MODULE_PX == 0);
    // Region must fit (DIGEST_QR_MODULES + 2*BORDER) modules.
    assert!(
        DIGEST_REGION_PX
            == (DIGEST_QR_MODULES + 2 * DIGEST_QR_BORDER) * DIGEST_MODULE_PX
    );
    // Region must not overlap the toast row at 640×480.
    assert!(DIGEST_REGION_Y + DIGEST_REGION_PX + MARGIN_Y <= 480);
    // Region must not overlap the logo at 640×480 (logo is
    // top-right, [496..624) x [16..144); region is bottom-
    // right, [444..624) x [268..448)). The y-ranges are
    // disjoint — assertion encodes that.
    assert!(DIGEST_REGION_Y >= 144);
    // Region must fit horizontally at 640px wide.
    assert!(DIGEST_REGION_X + DIGEST_REGION_PX + MARGIN_X <= 640);
};
```

(Tweak constant names and exact assertion shapes for taste
— the *content* of the assertions is what matters.)

**Doc-comment:** explain the geometry in prose at the top of
the constants block, citing 640×480 as the worst case and
noting that at larger modes the same `(DIGEST_REGION_X,
DIGEST_REGION_Y)` coords place the digest farther from the
edges (which is harmless — the right and bottom margins just
grow). If we ever want to right-anchor at runtime instead of
fixed-pixel-position, that's a Phase 3 enhancement (and the
constants stay as the 640×480 baseline).

**Constraints:**

- All constants `usize`, matching existing `MARGIN_X` etc.
- No runtime computation in this step. Geometry is
  compile-time fixed.

### 1c — `Renderer::draw_digest`

**Files:** `src/renderer/mod.rs`.

**Signature:**

```rust
impl Renderer {
    /// Encode `payload` as a QR code and render it into the
    /// digest region in the bottom-right of the framebuffer.
    ///
    /// Renders one `BltOp::BufferToVideo` call per QR module
    /// (Principle 6). At `DIGEST_MODULE_PX = 4`, each module
    /// is a 4×4 pixel buffer of either `FG` or `BG`.
    ///
    /// `payload` length is capped by the chosen
    /// `DIGEST_QR_VERSION`'s capacity at ECC level Medium.
    /// Caller's responsibility to size payloads; longer
    /// payloads will be rejected by the encoder.
    pub fn draw_digest(&mut self, payload: &[u8]) {
        // 1. Encode via qrcodegen with version = DIGEST_QR_VERSION,
        //    ECC = Medium, segment = bytes. Use the fixed-version
        //    constructor so we never silently pick a larger Version
        //    and overflow the region.
        // 2. For y in 0..(MODULES + 2*BORDER):
        //      For x in 0..(MODULES + 2*BORDER):
        //        pixel_x = DIGEST_REGION_X + x * DIGEST_MODULE_PX
        //        pixel_y = DIGEST_REGION_Y + y * DIGEST_MODULE_PX
        //        colour  = if matrix.get_module(x - BORDER, y - BORDER) { FG } else { BG }
        //        // (out-of-bounds get_module returns false — the border)
        //        BltOp::BufferToVideo with a [colour; MODULE_PX * MODULE_PX] buffer
        //        of size (MODULE_PX, MODULE_PX) at (pixel_x, pixel_y).
    }
}
```

**Constraints:**

- One BltOp per module. Do NOT precompute a 180×180 buffer
  and blit it in one call — that defeats Principle 6 and the
  GLZ-dictionary stress test that motivates it.
- Use the fixed-version `qrcodegen` constructor
  (`QrCode::encode_segments_advanced` with a `Version`
  argument and `mask = None`), not the auto-size
  constructor. Auto-size could pick a larger version than
  `DIGEST_QR_VERSION` and silently overflow the region.
- Reject (or panic — fail-fast is fine here, this is a debug
  / smoke method) payloads that don't fit at the chosen
  version + ECC level. Don't silently truncate.
- The per-module buffer can be a `[BltPixel; 16]`
  (`MODULE_PX * MODULE_PX = 16` at scale 4) allocated on the
  stack once per call, recoloured per module.
- ECC level: `Medium` (default). Low loses resilience for
  small gain in capacity; High costs capacity for resilience
  we don't currently need.
- No event emission. `draw_digest` is read-only from the
  timeline's perspective, same as `Scene::repaint`'s contract
  in the keystrokes plan.

### 1d — feature gate

**Files:** `Cargo.toml`, `src/scene.rs`.

```toml
[features]
default = []
digest-smoke = []
```

```rust
// in src/scene.rs::run_awaiting, after draw_chrome and before
// the blink loop:
#[cfg(feature = "digest-smoke")]
renderer.draw_digest(b"hello");
```

**Constraints:**

- The feature must not change behaviour when off. `cargo
  build` (no feature flag) must produce a binary
  byte-identical-modulo-timestamps to today's main.
- No conditional compilation outside this single call site
  and the `Cargo.toml` feature row.

### 1e — `make digest-smoke` target

**Files:** `Makefile`, possibly new `scripts/digest-smoke.sh`.

**What it does:**

1. Build with the feature: `cargo build --release --target
   x86_64-unknown-uefi --features digest-smoke` (or
   `scripts/build.sh --features digest-smoke` if the build
   script can take the pass-through flag — check before
   adding).
2. Stage the ESP (reusing `scripts/mkesp.sh`).
3. Boot QEMU headless with QMP socket (mirroring
   `scripts/screenshot.sh`'s flags), *without* sending the
   space-to-advance keystroke. Wait for the startup banner
   on serial, then wait another ~500 ms for the AWAITING
   chrome + digest to settle.
4. `screendump` to a PNG (`dist/digest-smoke.png`).
5. Run `zbarimg --raw -q dist/digest-smoke.png`. Capture
   stdout.
6. Assert stdout (trimmed) equals `hello`. Exit 0 if yes,
   non-zero with a diff-style error otherwise.
7. ACPI shutdown the guest.

**Driver shape (if a helper script is needed):**

```sh
# scripts/digest-smoke.sh — invoked by make digest-smoke
set -euo pipefail
out=dist/digest-smoke.png
mkdir -p dist
# … QEMU launch + QMP wait_banner + screendump …
decoded=$(zbarimg --raw -q "$out" | tr -d '\n')
expected=hello
if [ "$decoded" != "$expected" ]; then
    printf 'digest decode mismatch: got %q, expected %q\n' "$decoded" "$expected" >&2
    exit 1
fi
echo "digest-smoke: ok ($decoded)"
```

**Constraints:**

- Reuse `scripts/screenshot.sh`'s QEMU launch profile
  (same `-vga`, same `-bios`, same QMP socket pattern) to
  keep behaviour predictable.
- Do not modify `scripts/screenshot.sh` to gain a "hold in
  AWAITING" mode unless it already has a clean place to do
  so. Copying the minimum necessary into
  `scripts/digest-smoke.sh` is acceptable per the global
  CLAUDE.md guidance on tools/ scripts vs CI inlining.
- `zbarimg` must be available on the host. Add a `command -v
  zbarimg` check at the top of the script with a friendly
  error message ("apt install zbar-tools") if missing —
  matches existing tool-presence checks in
  `scripts/screenshot.sh` (e.g. its `command -v qemu-img`
  pattern, if present; otherwise add the pattern from
  scratch).
- The target should be `.PHONY` and should clean up the QMP
  socket and QEMU process on exit (trap pattern lifted from
  `scripts/screenshot.sh`).

### 1f — smoke exit

After 1a–1e are committed, run:

```sh
make digest-smoke
# expect: "digest-smoke: ok (hello)"
```

```sh
make spice-ryll DIGEST_FEATURE=1
# manually press '1', '3', '5' — observe QR rendering at each
# mode. Expected: QR visible at the initial mode; wiped on
# mode switch (Phase 2 issue, documented in Deferred above).
```

(If `make spice-ryll` does not have a clean way to pass the
feature flag through, document that as a finding for Phase 2
and run the manual cross-mode check via a hand-issued
`scripts/spice-ryll.sh` with the feature-built binary
pre-staged.)

```sh
pre-commit run --all-files
```

If anything fails, diagnose root cause rather than papering
over — per `~/.claude/CLAUDE.md`'s problem-solving guidance.

Update this plan's *Outcome* section with:

- The commit SHAs for steps 1a–1e.
- The chosen crate (`qrcodegen` or fallback).
- Whether the cross-mode manual check showed the digest at
  each mode key, and whether mode switches wiped it as
  expected.
- Any deviations from the defaults documented above (region
  geometry, QR Version, ECC level), with reasoning.

## Exit criteria

- [ ] A QR encoder crate is in `Cargo.toml` with no_std-
      compatible features; `make build`, `cargo clippy`, and
      `cargo fmt --check` all pass.
- [ ] `DIGEST_*` constants exist in `src/renderer/mod.rs`
      with compile-time `assert!` proving the chosen Version
      fits at 640×480 without overlapping logo, AWAITING
      cursor, or toast row.
- [ ] `Renderer::draw_digest(&mut self, payload: &[u8])`
      exists, uses the fixed-version constructor, renders
      one `BltOp::BufferToVideo` per QR module, and
      panics (or returns an error — caller chooses) on
      oversized payloads.
- [ ] `digest-smoke` cargo feature gates a single
      `draw_digest(b"hello")` call in `run_awaiting`.
- [ ] `make digest-smoke` builds, boots headless, captures
      a PNG, runs `zbarimg`, and asserts `hello` round-trips.
- [ ] Manual cross-mode check under `make spice-ryll`
      (feature-built binary): digest is visible at the
      starting mode. Mode-switch behaviour (digest wipe vs
      survival) is documented in *Outcome* — survival is not
      required in this phase.
- [ ] No call to `draw_digest` exists outside the feature-
      gated site. (Verified by grep.)
- [ ] No regression: `make build` (no feature) produces the
      same binary modulo timestamps; `make screenshot`
      continues to pass with its current parking-screen
      reference image; `make release-verify` continues to
      pass.
- [ ] `pre-commit run --all-files` exits 0 at every commit
      across the phase.
- [ ] Commit messages follow the project's template (subject
      under 50 chars ending in a period, body wrapped at 75,
      `Prompt:` paragraph, `Signed-off-by`, `Co-Authored-By`
      with model + context + effort).

## Risks and open questions

- **`qrcodegen` no_std build under `uefi = "=0.37.0"`.** The
  crate advertises no_std support but the version pin is
  tight; if a dependency conflict surfaces, fall back to
  `qrcode` per step 1a. If both fail, **stop and escalate**
  — the master plan's open question on this expects a
  measurement-driven swap, not a hand-rolled encoder.
- **`qrcodegen`'s internal allocation.** Even with no_std it
  may use `alloc::vec::Vec` internally — that's fine, our
  `uefi` features include `alloc` + `global_allocator`. The
  rejection criterion is `std`, not `alloc`.
- **Module-by-module BltOp performance.** 180×180 / 4 / 4 =
  2025 BltOp calls per `draw_digest`. At ~microseconds per
  call under OVMF+QXL, that's 2-10 ms — well within frame
  budget for AWAITING. If Phase 2's per-tick refresh makes
  this measurably slow, the fix is to drop the refresh rate,
  not to violate Principle 6.
- **Hard-coded payload visibility under `make spice-ryll`.**
  The `digest-smoke` feature must be passed through to the
  build that `spice-ryll.sh` consumes. If `spice-ryll.sh`
  pulls a pre-built binary from a fixed path, the manual
  cross-mode check requires a manual rebuild. Document the
  workflow in this plan's *Outcome*.
- **`zbarimg` availability and version skew.** Debian's
  `zbar-tools` is the canonical decoder; behaviour has been
  stable for years. If a developer's host lacks it, the
  `make digest-smoke` target fails with a clear error, not a
  silent skip.
- **`run_awaiting`'s blink loop drawing over the digest.**
  The cursor blink toggles a single cell at `(0, 0)`; the
  digest is at the bottom-right. No collision. The blink
  loop does not call `renderer.clear()`. Confirm at step 1d.
- **Border-vs-region geometry mismatch.** `qrcodegen`'s
  border parameter renders the quiet zone *inside* the bit
  matrix. The constant layout in step 1b accounts for this
  by including `2 * BORDER` modules in `DIGEST_REGION_PX`.
  If a future change separates the border from the region,
  update the assertion to match.

## Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with it. In particular:
confirm the choice of `qrcodegen` (and the fallback chain),
confirm the bottom-right region geometry at 640×480 with the
calculated coordinates, and confirm that the `digest-smoke`
feature is the only behaviour-changing knob this phase
introduces.
