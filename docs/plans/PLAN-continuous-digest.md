# Continuous multi-channel visual digest — per-line state oracle

## Prompt

Before responding to questions or discussion points in this
document, explore the uncalibrated-sextant codebase thoroughly.
Read the existing digest encoder (`src/digest.rs`), the renderer
methods that paint and hash the framebuffer (`src/renderer/mod.rs`,
particularly `draw_digest` and
`crc32c_framebuffer_excluding_digest`), the scene runners and the
existing refresh sites (`src/scene.rs`, especially `Scene::run`
and `Scene::refresh_digest`), the bootloader sub-state-machine
(`src/bootloader.rs`), the ring buffer (`src/event.rs`), and the
existing wire-format spec (`docs/visual-digest-format.md`).

Read [PLAN-visual-digest.md](PLAN-visual-digest.md) end to end —
this plan extends and partially supersedes it. Read
[PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md)
for context on why the read-back path has been treated
gingerly. Read `DESIGN.md`'s *Two-channel test architecture* and
*Wire-level control* sections; this plan re-frames what the
on-screen digest is *for* (operator-quote: substitute for the
never-built second-serial gRPC channel, not just a display
oracle).

Where a question touches `uefi-rs` semantics — particularly the
ordering and cost of successive `BltOp::BufferToVideo` calls and
whether incremental hashing alongside paint operations introduces
visible flicker — research as needed and flag any residual
uncertainty explicitly. Do not assume that bare-hardware GOP
behaviour matches OVMF+QXL or OVMF+stdvga.

External references: the QR Code 2005 specification
(ISO/IEC 18004) Table 7 for Version × ECC byte-mode capacity (we
are pinned at V5/L = 106 bytes today and may bump in this plan).
The CRC32C (Castagnoli) algorithm is what the existing TLV
trailer already uses; new per-channel hashes should use the same
algorithm for consistency.

All planning documents live in `docs/plans/`. Phase plans are
separate files named `PLAN-continuous-digest-phase-NN-...md` and
tracked in the *Execution* table below. One commit per logical
change; each commit builds, passes `pre-commit run --all-files`,
and has a clear message in the project's existing style.

## Situation

The visual on-screen digest landed via PLAN-visual-digest
(phases 1–3) and is fully operational today. What it does:

- `Scene::refresh_digest` runs at exactly three scene-phase
  boundaries (`src/scene.rs:307–313`): after AWAITING ends, after
  BOOTING ends, after PARKED ends. It also runs at the end of
  `Scene::repaint` (`src/scene.rs:631–633`) so a mode switch
  doesn't leave the QR stale.
- It uses "path A": `crc32c_framebuffer_excluding_digest`
  (`src/renderer/mod.rs:556`) reads the framebuffer back via
  `BltOp::VideoToBltBuffer` and CRC32Cs every non-digest byte.
  ~7 ms per call at 3 GHz, per the phase-2 plan.
- The bootloader sub-state-machine is **explicitly carved out**.
  `Scene::refresh_digest`'s `assert!`
  (`src/scene.rs:658–661`) crashes the firmware if a refresh
  happens during `RepaintState::BootingBootloader`.
- AWAITING has no QR on screen at all — the first refresh fires
  only after AWAITING transitions to BOOTING.
- The TLV payload is fixed at V5/L = 106 bytes
  (`src/digest.rs:89`), with 14 bytes overhead for header +
  trailer, leaving ~92 bytes of TLV body. Event records are
  carried verbatim; the trailer is one CRC32C of the
  non-digest framebuffer bytes.

The original PLAN-visual-digest framed all of this as "the
visual half of the two-channel test architecture": a display
correctness oracle that a screenshot decoder could match
against a server-side framebuffer read-back.

The operator has since clarified the actual intent. The QR is
the **substitute for the never-built second-serial gRPC
channel**. UEFI does not make a second serial port portable
across the clouds we care about (OpenStack, Shaken Fist,
Proxmox, oVirt), so the cross-channel state-sync mechanism
needed for unit-tested SPICE-client behaviour has to ride
side-band through the display. The QR is that side-band. It
should carry a periodic snapshot of every I/O surface the
firmware exposes — display state today, plus last N keypresses
today, plus last N USB-redir packets / pointer events / audio
frames / whatever channel comes next.

Three concrete consequences of that re-framing, each of which
the current implementation gets wrong:

1. **Wrong hash domain.** Path A hashes what the server *read
   back* from its own framebuffer. The use case is "did the
   client render what the server intended" — a client-side
   wedge in the SPICE display pipeline (or a SPICE-client paint
   bug, or a ryll capture-pipeline race) doesn't perturb the
   server's read-back. Path A catches GOP-level wedges; it does
   not catch the wedges the QR is supposed to flag.
2. **Wrong cadence.** Three snapshots per boot is far too coarse
   for bisection. If the client wedges on line 12 of the boot
   transcript, the QR after BOOTING completes is the first
   evidence and the human debugger has 30+ lines of suspect
   territory. Per-line refresh (operator confirmation) bisects
   to a single line.
3. **Wrong coverage.** The bootloader carve-out and the
   AWAITING-has-no-QR gap are the highest-diagnostic-value
   surfaces in the whole scene: the bootloader paste sequence
   is the entire point of the locked-bootloader test (does the
   client correctly deliver Ctrl+Alt+V?), and a pre-keypress
   display wedge in AWAITING is currently completely invisible.

The phase-2 rationale for path A and for the bootloader
carve-out was sound *under PLAN-visual-digest's original
framing*. Under the operator's clarified framing, both choices
flip: path A's "concentrates cost into three calls" becomes a
liability because we want frequent refreshes; the bootloader
carve-out's "could mask paste-correctness bugs" was about a
ring-buffer-event refresh racing the paste record, which is a
solvable per-refresh-point ordering question, not a reason to
blind the channel through the whole sub-state-machine.

The headless GOP read-back bug
([PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md))
was ultimately a QR-capacity-constant bug, not a real
read-back problem; the read-back path is functional. We are
choosing to retire it from the client-comparison oracle role
because it's the wrong oracle, not because it's broken.

## Mission and problem statement

Convert the visual digest from a display-only,
phase-boundary, server-side-read-back snapshot into a
continuous, multi-channel, intent-based state oracle suitable
for client-side wedge detection across every I/O surface this
firmware exposes — present and future.

By the end of this plan:

- **Intent hashing (path B).** The renderer maintains a running
  CRC32C of every byte it asks the framebuffer to display, in
  the non-digest region. This is the "what the server intended
  to put on screen" hash, computed at write time, no read-back
  required. The hash is exposed to `Scene::refresh_digest` via
  a getter on `Renderer`.
- **Per-line refresh.** The boot-transcript runner refreshes
  the digest after every painted line. `Scene::run_awaiting`
  refreshes on every cursor blink transition (the only visible
  state change). `Scene::run_parked` refreshes after the
  SYSTEM ONLINE line and on every blink transition.
  `bootloader::run` refreshes at named, well-defined points
  inside its sub-state-machine — not from inside polling
  loops, but at every visible state transition (after each
  prompt render, between paste record and validation, around
  countdown ticks).
- **Bootloader carve-out replaced.** The blanket
  `RepaintState::BootingBootloader` assertion in
  `refresh_digest` is removed. The bootloader gains its own
  refresh-point calls placed to satisfy the original
  paste-correctness concern (refresh *after* the
  `PasteReceived` event is recorded and *before* the validation
  branch, so the QR observed by the test driver reflects the
  paste the firmware actually received).
- **Multi-channel TLV.** The wire format grows new TLV record
  types for per-channel rolling hashes — one record per
  channel, each carrying a 4-byte CRC32C of that channel's
  event stream since boot. Initial channels: display intent,
  keypresses, bootloader decisions, paste records, mode
  switches. Channel IDs in a documented reserved range so
  future USB-redir / pointer / audio channels slot in without
  a schema bump (TLV's whole point).
- **Capacity strategy decided and documented.** With per-line
  refresh, the question of whether the QR carries raw recent
  events plus rolling-hash summaries, or rolling-hash
  summaries only, has to be resolved before encoding hits the
  V5/L wall. Default below; revisit in phase 3.
- **AWAITING gains a QR from the moment chrome is painted.**
  Pre-keypress wedge is a real failure mode and is currently
  invisible.
- **Path A is retired from the client-comparison oracle role.**
  `crc32c_framebuffer_excluding_digest` and its read-back
  remain available as an optional server-side self-check
  (Phase 4 decides whether to keep, gate, or delete), but
  refresh no longer depends on them.
- **Docs reflect the actual role.** `DESIGN.md` is updated to
  describe the digest as the substitute for the never-built
  second-serial gRPC channel, not just "the visual half".
  `docs/visual-digest-format.md` documents the new TLV types
  and the channel ID range. `ARCHITECTURE.md`'s refresh-cadence
  paragraph is rewritten. The note in `AGENTS.md` about the
  bootloader carve-out is updated to describe the replacement
  refresh-points.

Out of scope for this plan:

- The actual gRPC-over-serial transport (still future work; the
  digest is the substitute, not the replacement).
- New channels beyond what the firmware already records (no USB
  redir, no pointer, no audio; those land with the protocol
  work that introduces them, hooking into the channel ID range
  this plan reserves).
- ryll-side decoder changes. ryll's decoder ignores unknown
  TLV types today (verify in phase 4) so the new records
  don't break its current parser; whether ryll uses them for
  assertions is a ryll-side decision tracked in their repo.

## Open questions

Defaults below are strong but worth confirming or iterating at
the relevant phase. Capture changes inline rather than letting
them drift.

- **Hash domain: pure intent, or intent + opt-in read-back?**
  **Default: pure intent for the client-comparison oracle;
  keep `crc32c_framebuffer_excluding_digest` as a separate,
  opt-in self-check (e.g. `make digest-readback-check`) that
  is run in CI but not on every boot.** Path A caught the
  headless-readback wedge once; the diagnostic capability is
  worth preserving even though it's no longer the primary
  oracle. If phase 1 measurement shows path A is unused
  anywhere downstream and adds maintenance weight without
  carrying it, delete it instead.

- **Where the intent CRC lives.** **Default: a `u32` field on
  `Renderer`, updated inside every helper that issues a
  `BltOp::BufferToVideo` or `BltOp::VideoFill` against the
  non-digest region.** Alternatives — computing it from the
  event log at refresh time, or maintaining it in a wrapper
  type — either lose fidelity (the event log doesn't carry
  pixel-level intent) or add an abstraction that doesn't earn
  its keep. The renderer already owns the BLT call sites; it
  is the natural home.

- **What counts as "non-digest region" for path B.** The
  existing read-back path uses a right-anchored rectangle
  computed at runtime. Path B has to use the same rectangle
  or the two hashes can never be compared, which matters if
  we keep path A as a self-check. **Default: factor the
  rectangle calculation out of
  `crc32c_framebuffer_excluding_digest` into a shared helper,
  call it from both paths.**

- **Per-line refresh cost.** Path B updates the CRC at write
  time (negligible per-byte cost), but each refresh still
  encodes a QR and issues ~37×37 = ~1400 BLT calls. Per the
  visual-digest phase-2 measurement note, QR encode is a few
  ms and the BLT calls are individually cheap but accumulate.
  **No firm default; phase 1 should measure end-to-end paint
  budget impact on the boot transcript pacing (200 ms per
  line today) and decide whether to refresh every line, every
  other line, or batch refreshes at sub-phase boundaries
  (e.g. after each `BOOT_SCRIPT` group).** Per-line is the
  preference; only step back if the measurement forces it.

- **AWAITING refresh trigger: blink-tick or state-change-only?**
  **Default: every blink transition (twice per second).** The
  cursor glitch glyphs change content visibly; refreshing only
  on glitch-substitution would skip ordinary blink frames and
  the digest would miss the "the cursor stopped blinking"
  failure mode. Twice a second is well under the per-line
  paint budget so cost isn't a concern; the question is just
  what the right cadence semantic is.

- **Bootloader refresh-point placement.** The carve-out is
  replaced by explicit calls, but exactly *where* matters for
  the original paste-correctness concern. **Default:** (a)
  refresh after each `Advanced b64 cryptographic coprocessor`
  preamble line; (b) refresh after every R/I/A prompt render
  (initial and retry repaints); (c) refresh after the
  `PasteReceived` event is pushed into the ring buffer and
  before the validation branch — so a screenshot taken at
  validation time sees the paste content in the digest; (d)
  refresh on each countdown tick during the visible
  shutdown sequence. The retry attempt-counter rendering and
  the cleared-and-re-rendered prompt should each carry a
  refresh. Phase 2 should walk `bootloader.rs` end-to-end and
  produce the final list.

- **TLV capacity strategy.** With multi-channel rolling
  hashes added (5 channels × 6-byte records = 30 bytes), the
  92-byte TLV body has ~62 bytes left for raw events. That's
  ~10 small events. Two options: **(a)** stay at V5/L, cap
  raw events at "last 8 that fit", treat the rolling hashes
  as the primary diagnostic; **(b)** bump to V10/L (~213
  bytes) to comfortably carry rolling hashes + ~30 raw
  events. V10 is ~57×57 modules vs V5's 37×37 — at our
  current 4-pixel-per-module scale that's 228×228 px vs
  148×148 px, which still fits the bottom-right corner with
  room to spare. **Default: option (b) — bump to V10/L.**
  The operator's "we just need to flag that an error
  occurred" framing argues for (a), but raw events are what
  the iterative interactive-debug session uses to localise
  *which* event diverged. The screen real estate is
  available; spend it.

- **What channels exist on day one.** **Default: display
  intent, keypresses, bootloader decisions, paste records,
  mode switches.** These are exactly the event variants the
  firmware records today (`src/event.rs`'s `Event` enum)
  plus the new display-intent CRC. Channel IDs: 0x01–0x0F
  reserved for these "boot-time core" channels; 0x10–0x1F
  reserved for future SPICE-channel-derived state (USB redir,
  pointer, audio, smartcard, clipboard, etc); 0x20+
  available for whatever else later.

- **Per-channel hash domain.** **Default: CRC32C of the
  concatenated TLV-encoded event records for that channel,
  in chronological order, from boot.** Same algorithm as the
  existing framebuffer hash, so the host-side decoder only
  needs one implementation. "From boot" rather than "last N"
  means the hash diverges on the *first* missed event and
  stays diverged, which is the right semantic for "flag that
  something went wrong" — last-N would mask transient
  divergences once they scroll out of the window.

- **Display-intent hash: full-history or current-state?**
  **Default: current-state — the running CRC of "what's on
  screen right now" (i.e. the path-B intent hash described
  above).** A full-history-of-paint-operations hash would
  diverge on benign repaints (mode switch, scene transition)
  that don't represent client-side wedges. The
  current-state hash diverges if and only if the visible
  framebuffer differs, which is the client-comparison
  semantic.

- **Bootloader scene's clock_ms vs digest_frame_counter.** The
  existing frame counter is incremented inside
  `refresh_digest`. With far more refreshes per boot, the u32
  wrap horizon shrinks from 136 years (current) to roughly
  "still way more than any plausible boot". Confirm at phase
  1 that no downstream parser assumes counter values fit in a
  smaller integer.

- **Migration of the digest-payload-smoke harness.**
  `make digest-payload-smoke` decodes the existing TLV format.
  The new format adds records but keeps existing tags. **Default:
  the smoke harness gains new assertions for the rolling-hash
  records; existing assertions stay green.** If the capacity
  bump to V10/L is taken, the harness also needs to know the
  new QR version for the decoder configuration.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. Intent hash (path B) + retire path A from refresh | PLAN-continuous-digest-phase-01-intent-hash.md | Not started |
| 2. Refresh cadence + coverage (per-line, AWAITING, bootloader carve-out replacement) | PLAN-continuous-digest-phase-02-cadence.md | Not started |
| 3. Multi-channel TLV + capacity decision (V5/L vs V10/L) | PLAN-continuous-digest-phase-03-multi-channel.md | Not started |
| 4. Docs, decoder coordination, closeout | PLAN-continuous-digest-phase-04-closeout.md | Not started |

### Phase 1 sketch — intent hash (path B)

Replace the framebuffer-hash source under `Scene::refresh_digest`
without changing cadence or coverage. The QR's *value* changes
(intent CRC instead of read-back CRC) but its *position and
frequency* don't. This is the smallest unit that proves path B
is viable on its own; if measurement shows per-paint CRC cost
is prohibitive (it shouldn't be — it's a per-byte XOR — but
measure), this phase is the bail-out point.

Sub-steps roughly:

- Add a `u32` field on `Renderer` initialised to the CRC32C of
  an empty stream.
- Add a private helper on `Renderer` that updates the running
  CRC with a byte slice (the same bytes about to be BLTed).
- Wire the helper into every BLT call site that touches the
  non-digest region. Audit `draw_glyph`, `draw_cursor_glyph`,
  `clear_cell`, `clear_row`, `clear`, `draw_text_bitmap`'s
  per-cell BLT, and the digest's own write-back (which is
  excluded from the hash).
- Factor the non-digest-rectangle math out of
  `crc32c_framebuffer_excluding_digest` into a shared helper.
- Make `Scene::refresh_digest` read the intent CRC instead of
  calling `crc32c_framebuffer_excluding_digest`.
- Keep `crc32c_framebuffer_excluding_digest` and add an opt-in
  `make digest-readback-check` target (or feature flag) that
  exercises path A as a self-check for the GOP store-then-read
  invariant. Phase 4 decides its long-term fate.
- Measure per-line paint budget impact and record in the phase
  closeout. If the cost is non-trivial, this is the place to
  notice before phase 2 amplifies it.

### Phase 2 sketch — refresh cadence + coverage

With path B in place, refresh becomes cheap enough to do
frequently. This phase distributes refresh calls to the right
places and removes the bootloader carve-out.

- Per-line refresh in the boot transcript runner.
- AWAITING refresh on every cursor blink transition.
- PARKED refresh on the SYSTEM ONLINE line and on every blink
  transition.
- Walk `bootloader.rs` and add the refresh-points listed in the
  "Bootloader refresh-point placement" open question. Remove the
  `assert!` in `Scene::refresh_digest`.
- Update `RepaintState::BootingBootloader`'s repaint flow to
  also refresh on completion (it can no longer be the
  unreachable case it currently models).
- Update `Scene::repaint` if necessary so a mode switch
  mid-bootloader-scene doesn't leave the QR stale.
- Re-run `make digest-payload-smoke` after each refresh-site
  addition to confirm no regression in the existing format.

### Phase 3 sketch — multi-channel TLV

Extend the TLV format with rolling-hash records, decide the
capacity strategy, and bump the QR version if (b) wins.

- Add TLV tag constants for per-channel rolling hashes
  (`TAG_HASH_DISPLAY`, `TAG_HASH_KEYPRESS`, ...) in the
  reserved 0x10–0x1F range — or pick a different range now if
  that conflicts with anticipated SPICE-channel state tags.
- Maintain a per-channel CRC32C in `Scene` (or in a new
  `ChannelHashes` struct) updated every time an event is
  pushed to the ring buffer. Display-intent hash comes from
  the renderer per phase 1.
- Encoder writes the rolling-hash records before raw event
  records, so they survive capacity truncation.
- If the V10/L bump is taken: update `DIGEST_PAYLOAD_CAPACITY`,
  update the QR version constant in `draw_digest`, update the
  rectangle-fits-on-screen assertion, update the
  digest-payload-smoke decoder configuration. The
  `_: () = assert!(...)` capacity guard in `src/digest.rs`
  needs its allowed maximum raised in step with the change.
- If staying at V5/L: implement raw-event truncation that
  prioritises the rolling-hash records.
- Bump `DIGEST_SCHEMA_VERSION` (adding records is technically
  TLV-compatible, but the channel ID conventions are new
  semantics worth signalling).

### Phase 4 sketch — docs, decoder coordination, closeout

- Update `DESIGN.md`'s framing of the on-screen digest.
- Rewrite the `docs/visual-digest-format.md` TLV catalogue.
- Update `ARCHITECTURE.md`'s refresh-cadence paragraph and
  the description of what the digest's hash represents.
- Update the bootloader carve-out note in `AGENTS.md`.
- Verify ryll's decoder ignores unknown TLV records gracefully
  (read ryll-side code; do not push changes there). If it
  doesn't, this becomes a coordination note for the ryll repo
  rather than a blocker.
- Decide path A's long-term fate (keep as opt-in self-check,
  gate behind a feature flag, or delete) and execute.
- Update `docs/plans/index.md` row for this plan to *Complete*
  and link the final commit range.

## Agent guidance

### Execution model

All implementation work is done by sub-agents, never in the
management session. The management session (this conversation)
is reserved for planning, review, and decision-making.

The workflow is:

1. **Plan** at high effort in the management session.
2. **Spawn a sub-agent** for each implementation step with the
   brief from the phase plan, at the recommended effort level
   and model.
3. **Review** the sub-agent's output in the management session.
   Check the actual files — the sub-agent's summary describes
   what it intended, not necessarily what it did.
4. **Fix or retry** if the output is wrong. Diagnose whether
   the brief was insufficient (improve it) or the model was too
   light (upgrade it), then re-run.
5. **Commit** once the management session is satisfied.

Use `isolation: "worktree"` for risky / experimental sub-agents.
Phases 1 and 3 are good candidates for worktree isolation
(touching the renderer's hot path and the wire format
respectively). Phase 2's coverage changes are localised enough
to land in the main tree.

### Planning effort

The master plan itself is at **high effort** (this document).
Phase plans should specify effort per step. Recommended:

- **Phase 1** — medium-to-high for planning. The intent-hash
  plumbing is mechanical once the audit of BLT call sites is
  complete, but the audit is the part that has to be right.
  Implementation steps mostly medium; the audit is high.
- **Phase 2** — medium for planning. Refresh-point placement
  in `bootloader.rs` requires careful walking but the criteria
  are well-defined in this plan.
- **Phase 3** — high for planning. The capacity decision
  cross-cuts encoder, renderer, decoder, and downstream parsers;
  the channel-ID conventions are an API surface we should not
  re-cut later.
- **Phase 4** — low for planning, medium for execution. The doc
  updates are mostly mechanical but cross-reference several
  source files.

**Model choice:** Phase 1's BLT-call-site audit and phase 3's
wire-format design should use **opus**. Most implementation
steps can use **sonnet** with a detailed brief. No phase
warrants haiku.

### Management session review checklist

After each sub-agent completes, verify:

- [ ] The files that were supposed to change actually changed
      (read them, don't trust the summary).
- [ ] No unrelated files were modified.
- [ ] `pre-commit run --all-files` is green.
- [ ] `make digest-payload-smoke` still passes (after phase 3,
      with updated assertions).
- [ ] The QR is visually present in the expected places
      (screenshot via `make screenshot` after phase 2).
- [ ] Per-line refresh has not visibly slowed the boot transcript
      (the 200 ms per-line pacing should still feel deliberate,
      not labored).
- [ ] Commit message follows project conventions (including the
      `Co-Authored-By` line with model, context window, effort
      level, and other settings).

## Administration and logistics

### Success criteria

We will know when this plan has been successfully implemented
because the following statements will be true:

* The QR digest is visible in every scene phase, including
  AWAITING (from the moment chrome is painted) and throughout
  the bootloader sub-state-machine.
* The digest refreshes at least once per painted line of the
  boot transcript, on every cursor blink transition in AWAITING
  and PARKED, and at every visible state change inside the
  bootloader scene.
* The QR's framebuffer hash reflects "what the server intended
  to paint" (path B intent CRC), not "what the server read back
  from its own framebuffer" (path A).
* The QR carries per-channel rolling hashes for display intent,
  keypresses, bootloader decisions, paste records, and mode
  switches, in addition to the raw recent events it already
  carries.
* `make digest-payload-smoke` passes against the new format.
* `pre-commit run --all-files` is green.
* `DESIGN.md`, `ARCHITECTURE.md`, `AGENTS.md`, and
  `docs/visual-digest-format.md` reflect the new framing,
  cadence, and TLV record types.
* Path A (`crc32c_framebuffer_excluding_digest`) is either
  retired or moved behind an opt-in self-check target;
  `Scene::refresh_digest` no longer depends on it.

### Future work

* The actual gRPC-over-serial transport (PLAN-visual-digest's
  intended companion) still needs writing. With per-channel
  hashes on the visual side, the eventual gRPC transport has a
  cheap way to cross-check its own framing against the visual
  oracle.
* New channels for USB redir, pointer events, audio frames, and
  smartcard activity slot into the reserved channel-ID range as
  the firmware grows support for those SPICE channels.
* A "QR-only failure-localisation playbook" — a one-page doc
  the on-call human reads when CI halts with a hash mismatch —
  is worth writing once we have field experience with
  divergence patterns.
* If per-line refresh cost ever becomes a real bottleneck,
  pre-encoded QR caching keyed on payload hash would amortise
  the encode cost across repeated identical payloads (rare,
  but the AWAITING blink case generates one).

### Bugs fixed during this work

(Populated as we encounter them.)

### Documentation index maintenance

When this plan is created, update the following files in
`docs/plans/`:

* **`index.md`** — add a row to the *Master plans* table with
  today's date, a link to this plan, a one-line intent summary,
  the initial status ("Not started" or first phase in progress),
  and links to each phase plan file as they're written.
* **`order.yml`** — add an entry for this master plan so it
  appears in the documentation navigation bar. Phase files
  should *not* be added to `order.yml`.

When all phases are complete, update the status column in
`index.md` to *Complete*.

### Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the work
you intend to do aligns with that plan.
