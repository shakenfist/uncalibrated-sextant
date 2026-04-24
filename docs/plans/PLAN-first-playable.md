# First playable — baby's first weird test game

## Prompt

Before responding to questions or discussion points in this
document, read `DESIGN.md` carefully — it is the load-bearing
design artifact for this project and this plan is a direct
consequence of its decisions. In particular, re-read the
*First milestone scope* section to confirm the boundaries
of this plan, and the *Design principles*, *Connection
handshake*, *Wire-level control*, *Boot sequence (first
scene)*, and *Voice: unreliable narration leaks* sections
to confirm the shape of what gets built.

Cross-repo references, in order of likely usefulness:

- `shakenfist/ryll/Makefile` — the model we're stealing for
  `make qemu`. Read it; don't reinvent.
- `shakenfist/instar` — prior-art `no_std` UEFI Rust project
  in this workspace. Look at its Cargo layout, build tooling,
  and `.cargo/config.toml` for how it targets
  `x86_64-unknown-uefi` and what `uefi-rs` version it uses.
  The transport pattern is not in scope for this milestone;
  the Cargo/toolchain pattern is.
- `shakenfist/uefi-latency-guest` — the minimal UEFI probe
  this project will eventually complement. If it exists
  locally, its Cargo layout is relevant prior art.
- `uefi-rs` crate documentation — the `Boot::locate_handle`,
  `GraphicsOutput`, `SimpleTextInputEx`, `SimplePointer`, and
  `BootServices::stall` APIs are the surface we need.
- OVMF — usually installed as `/usr/share/OVMF/OVMF_CODE.fd`
  and `OVMF_VARS.fd` on Debian/Ubuntu, or `edk2-ovmf` on
  Fedora/Arch. The Makefile should detect and document.

All planning documents go in `docs/plans/`.

Phase plans will be separate files named
`PLAN-first-playable-phase-NN-descriptive.md` and tracked in
the *Execution* table below.

I prefer one commit per logical change, and at minimum one
commit per phase. Each commit should be self-contained: it
should build, pass tests, and have a clear commit message
explaining what changed and why.

## Situation

The `uncalibrated-sextant` repository currently contains
design documents and planning templates only (commit
`649b8c8`). There is no Rust code, no build tooling, no
tests, and no way to actually run anything. The intent of
the project — a UEFI Rust SPICE channel exercise harness
dressed as a tiny retrofuturist mini-game — is well-
specified in `DESIGN.md`, but nothing yet proves that any
of it is buildable, runnable, or visually compelling.

The decision has been made to bootstrap with a local
`make qemu` target following ryll's pattern, to scope the
first milestone strictly to what a developer can run on
their own machine, and to defer all cross-repo work
(Ryll integration, Shaken Fist changes, Nova orchestration,
serial transport) to subsequent milestones. Wire-level
control stays on rung 1 (GOP via OVMF's QXL-GOP driver).
Handshake uses first-client-input, not Ryll `Start`.

## Mission and problem statement

Produce a bootable UEFI binary, packaged and runnable via
`make qemu`, that renders the opening boot sequence
described in `DESIGN.md` — AWAITING OPERATOR screen, the
system-coming-online telemetry with narrator leaks, the
localised CRT scruff overlay — such that a human sitting
in front of a QEMU window reacts with "oh, that's lovely"
and "oh, that's a bit unsettling" in the right proportions.

Success for this milestone is visual and aesthetic, not
assertional. There is no Ryll, no machine test oracle, no
CI verdict. The outcome we are trying to produce is a
screenshot (and a short screen recording) that tells us
whether the concept has legs.

The secondary mission — equally important, less
glamorous — is to nail down the build, packaging, and
release pipeline early, so that once we *do* want to wire
up Ryll and CI, the path from source to bootable artifact
is solved. "Prove the pipeline with a cheap payload" is the
classic MVP move and we should lean into it.

## Open questions

These do not block starting the plan, but should be
resolved by the phase in which they become relevant.

- **Phosphor colour.** Green (P1 / DEC VT100) or amber
  (IBM 5151 / Wyse)? Green is more Pip-Boy, amber is more
  1970s-minicomputer and possibly more legible at small
  sizes. Decide at Phase 4.
- **Font.** An 8x16 VGA bitmap font (public domain / BSD
  originals are easy to find) is the obvious default.
  Confirm licence and embed as a byte array. Decide at
  Phase 4.
- **Target resolution.** GOP negotiates a mode with the
  firmware. OVMF typically offers 800x600 and 1024x768.
  Pick one (1024x768 probably) and letterbox / center the
  scene within it. Later phases can exercise the mode walk
  across all offered modes as a proper display-channel
  test; this phase does not need that.
- **Rust toolchain.** `x86_64-unknown-uefi` is a Tier 2
  stable target; confirm we can build on stable without
  needing nightly. If `uefi-rs`'s current release demands
  features that require nightly, pin a version that does
  not. Decide at Phase 1.
- **Scruff depth for this milestone.** *Resolved.* The
  character-ROM cursor glitch (see DESIGN.md *Boot sequence*)
  is the minimum-viable glitch effect for this milestone —
  much cheaper than a compositor overlay and sufficient to
  prove the look-and-feel. The scanline-tile overlay is a
  stretch goal within Phase 5; drifting horizontal tear and
  out-of-focus halo are explicitly tracked in *Future work*
  below for a subsequent milestone. Reassess at Phase 5.
- **Where does the sequence end.** After `BOOT COMPLETE IN
  23.4s` there is currently nothing to do. Park on a static
  "System online. Awaiting instructions." screen with a
  blinking cursor? Loop back to AWAITING OPERATOR after a
  keypress for iterating? Decide at Phase 5.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. Cargo / `no_std` UEFI skeleton and Simple Text Output "hello" | PLAN-first-playable-phase-01-skeleton.md | Complete (commits 7712239, 8a0442d) |
| 2. Build tooling and `make qemu` | PLAN-first-playable-phase-02-build.md | Complete (commits 9ee4c21, 98705cb, d3bc0ca, da7f64d) |
| 3. Pre-commit, style, and minimal CI | PLAN-first-playable-phase-03-style.md | Not started |
| 4. Renderer foundation — framebuffer, font, per-glyph blit, palette | PLAN-first-playable-phase-04-renderer.md | Not started |
| 5. Boot sequence scene — AWAITING, telemetry, narrator leaks, scruff | PLAN-first-playable-phase-05-boot-sequence.md | Not started |
| 6. Packaging, screenshot, and documentation polish | PLAN-first-playable-phase-06-packaging.md | Not started |

### Phase 1 sketch — Cargo `no_std` UEFI skeleton

Establish the Rust project shell: workspace `Cargo.toml`, a
single binary crate targeting `x86_64-unknown-uefi`, a
`.cargo/config.toml` that makes `cargo build` produce an
EFI application by default, a `rust-toolchain.toml` if
needed, and a `.gitignore`. Depend on the current stable
`uefi` crate. Implement `efi_main` that clears the screen,
prints `Hello from Uncalibrated Sextant` via GOP text
output, stalls for a key, and exits cleanly.

This phase ends when `cargo build` produces a
`.efi` binary and we have read that binary out of the
target directory successfully. It does not yet have to
*run* — that's Phase 2.

### Phase 2 sketch — `make qemu`

Produce a `Makefile` with `build`, `qemu`, `clean`, and
`release` targets, modelled on
`shakenfist/ryll/Makefile`. Write helper scripts in
`tools/` (per project convention — no large Makefile
recipes) for ESP image assembly (FAT filesystem with
`/EFI/BOOT/BOOTX64.EFI`), OVMF detection with a helpful
error if absent, and QEMU invocation with a display window
and a serial log file for firmware chatter.

This phase ends when `make qemu` opens a QEMU window, OVMF
hands off to our binary, and the "Hello from Uncalibrated
Sextant" screen from Phase 1 is visible.

### Phase 3 sketch — pre-commit, style, CI

Add `.pre-commit-config.yaml` wiring rustfmt, clippy (with
`-D warnings`), shellcheck, trailing-whitespace, and
end-of-file-fixer. Add `rustfmt.toml` / `clippy.toml` as
needed. Add a `scripts/check-rust.sh` helper matching the
ryll pattern. Decide whether to add a minimal GitHub
Actions workflow at this phase or defer — if added, it
should use whatever runner scheme is appropriate (see
memory: shakenfist self-hosted jobs need the full
`[self-hosted, vm, <os>, <size>]` label set).

This phase ends when `pre-commit run --all-files` passes
on a clean tree.

### Phase 4 sketch — renderer foundation

Build the primitives the boot sequence needs:

- GOP framebuffer acquisition and a safe wrapper around
  linear-framebuffer access that respects UEFI's memory
  model.
- An embedded 8x16 monochrome bitmap font (public domain,
  licence recorded in the repo).
- A per-glyph `BitBlt` routine that draws a single glyph
  at a given cell position (principle 6 — repeated glyphs
  must flow to the server as repeated bitmap ops, not as
  a single `memcpy`).
- A fixed palette (phosphor green on black, or amber —
  open question above).
- A line-oriented text renderer with column-aligned dot
  leaders (`"FOO ........ OK"` style).
- Timing primitives via `BootServices::stall` for line-by-
  line pacing.

This phase ends when a test scaffold can render a few
lines of the eventual boot sequence with correct spacing
and pacing. No narrator leaks yet, no scruff yet.

### Phase 5 sketch — boot sequence scene

Compose the full opening sequence per `DESIGN.md`:

- AWAITING OPERATOR screen with a blinking cursor. Hold
  until the first `SimpleTextInputEx` key event or
  `SimplePointer` event arrives.
- The telemetry lines, paced realistically, with the
  narrator-leak parentheticals in a distinct render style
  (italics if the font supports it; otherwise indented
  plain text is acceptable).
- **Minimum-viable glitch: character-ROM cursor glitch.**
  A small pre-authored set of slightly-broken cursor glyph
  variants (missing pixel, smeared edge, shifted column,
  stuck phosphor trail) that substitute for the canonical
  glyph on a noisy-but-scripted schedule. Visible on the
  AWAITING screen and on the post-sequence parking screen.
  See DESIGN.md *Boot sequence* for the full rationale.
- **Stretch: localised CRT scruff overlay.** If time
  permits, a tiled scanline pattern composited on top of
  the clean base framebuffer. Drifting horizontal tear and
  out-of-focus halo remain explicitly deferred to a later
  milestone regardless (see *Future work*).
- A parking screen once the sequence completes.

A host-side ring buffer captures every event (keypress,
pointer move, line rendered) so that Phase 6 can include a
small dump-to-serial test — not the full gRPC-over-serial
transport, just a plain-text dump proving the ring buffer
works and serial output works. This is groundwork for a
future milestone, not part of this one's assertion
surface.

This phase ends when a human can run `make qemu`, press a
key, and watch the boot sequence play out with the voice
and tone described in `DESIGN.md`.

### Phase 6 sketch — packaging, screenshot, docs

- `make release` produces bootable artifacts via
  `qemu-img convert` — at minimum a raw `.img` and a
  `.qcow2`. VHD/VMDK are stretch.
- Capture a screenshot from a running QEMU session (via
  the QMP `screendump` command or VNC capture) and commit
  it into the repo. Embed in `README.md`.
- Expand `ARCHITECTURE.md` to describe the modules that
  now exist (renderer, scene manager, event ring buffer,
  scruff overlay).
- Expand `AGENTS.md` with the real `make` commands and
  the OVMF/QEMU dependencies.
- Update `docs/plans/index.md` to mark this plan complete
  and link to the phase plans.

This phase ends when a fresh clone, followed by
`make qemu`, produces the intended experience, and the
`README.md` screenshot matches.

## Agent guidance

Follow the execution-model, planning-effort, step-level,
and review-checklist guidance in `PLAN-TEMPLATE.md` at
the repo root. The same conventions apply here. A few
phase-specific notes:

- **Phase 1** is mostly mechanical but has to get the
  Cargo toolchain configuration exactly right; a bad
  `.cargo/config.toml` fails silently. Recommend a small
  amount of verification work at the end: inspect the
  produced `.efi` binary with `file` and confirm it is
  a PE32+ image for EFI.
- **Phases 2 and 3** are low-risk and can be planned at
  medium effort once Phase 1 is done.
- **Phase 4** is the research-heavy phase (GOP semantics,
  font layout, UEFI memory model). Plan at high effort.
- **Phase 5** is where aesthetic judgment meets
  implementation; review screenshots rigorously before
  declaring the phase done. The management session should
  look at the output, not just read the diff.
- **Phase 6** is mechanical again, but the screenshot is
  load-bearing for the milestone's success criterion.

## Administration and logistics

### Success criteria

We will know this plan has been successfully implemented
because:

- `make qemu` on a fresh clone (with OVMF installed) opens
  a QEMU window, the binary boots, the AWAITING OPERATOR
  screen is visible, a keypress starts the boot sequence,
  and the boot sequence plays through per `DESIGN.md` with
  narrator leaks and the character-ROM cursor glitch
  present (scanline overlay if Phase 5 reached its stretch,
  otherwise deferred per *Future work*).
- `make release` produces at least one bootable artifact
  (raw `.img` or `.qcow2`) that boots the same way under
  a fresh QEMU invocation.
- `pre-commit run --all-files` passes on a clean tree.
- `README.md` contains a screenshot of the running boot
  sequence, and its run-instructions section describes the
  commands accurately.
- `ARCHITECTURE.md` and `AGENTS.md` describe the modules
  and build commands that now exist.
- The aesthetic and voice match the guidance in
  `DESIGN.md` well enough that we decide the concept is
  worth further investment.

### Future work

Items deliberately deferred out of this milestone:

- **CRT scruff overlay beyond the cursor glitch.** The
  character-ROM cursor glitch is the first-playable
  minimum; the scanline-tile overlay is a Phase 5 stretch
  and falls here if it doesn't land. Drifting horizontal
  tear and out-of-focus halo are explicitly deferred
  regardless.
- gRPC-over-serial transport (from `instar`) as the real
  Ryll-facing event channel
- Ryll-side `Start` handshake
- Second serial port in Shaken Fist / Nova
- CI on shakenfist self-hosted runners, if not covered
  in Phase 3
- Wire-level rungs 2 (targeted QXL) and 3 (full QXL-
  direct)
- Further scenes beyond the boot sequence
- Channel mechanics for clipboard, USB redirection, audio
  record, smartcard, folder share
- Audio playback for the chime beyond the simplest
  possible implementation
- Packaging for distros beyond raw / qcow2

### Bugs fixed during this work

(None yet — will be populated during execution.)

### Documentation index maintenance

On creation of this plan, `docs/plans/index.md` and
`docs/plans/order.yml` are updated per the template. As
phases complete, update the status column in the
*Execution* table above and in `index.md`.

### Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with that plan.
