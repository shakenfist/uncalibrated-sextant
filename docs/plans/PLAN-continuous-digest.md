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

Two concrete consequences of that re-framing, each of which
the current implementation gets wrong:

1. **Wrong cadence.** Three snapshots per boot is far too coarse
   for bisection. If the client wedges on line 12 of the boot
   transcript, the QR after BOOTING completes is the first
   evidence and the human debugger has 30+ lines of suspect
   territory. Per-line refresh (operator confirmation) bisects
   to a single line.
2. **Wrong coverage.** The bootloader carve-out and the
   AWAITING-has-no-QR gap are the highest-diagnostic-value
   surfaces in the whole scene: the bootloader paste sequence
   is the entire point of the locked-bootloader test (does the
   client correctly deliver Ctrl+Alt+V?), and a pre-keypress
   display wedge in AWAITING is currently completely invisible.

The hash domain (path A: read the framebuffer back via
`BltOp::VideoToBltBuffer` and CRC32C the non-digest bytes) is
not on the list of things to change. Path A's read-back
captures exactly what SPICE transmits to the client, which is
the right reference point for "did the client render what the
server emitted." The phase-2 rejection of per-paint refresh was
framed in terms of cost per *paint*; under per-line refresh
the cost arithmetic is ~7 ms × ~30 lines ≈ 210 ms over a 6 s
boot transcript — a +3.5% overhead that does not warrant
inventing an intent-CRC + shadow-framebuffer mechanism to
avoid. Phase 1 measurement confirms the number before we
commit; if it is much worse than expected, that is the
fall-back decision point.

The bootloader carve-out's "could mask paste-correctness bugs"
was about a ring-buffer-event refresh racing the paste
record. That is a solvable per-refresh-point ordering
question, not a reason to blind the channel through the whole
sub-state-machine.

The headless GOP read-back bug
([PLAN-headless-readback-bug.md](PLAN-headless-readback-bug.md))
was ultimately a QR-capacity-constant bug, not a real
read-back problem; the read-back path is functional, which is
why this plan can lean on it more heavily without rebuilding
it.

## Mission and problem statement

Convert the visual digest from a phase-boundary, display-only
snapshot into a continuous, multi-channel state oracle suitable
for client-side wedge detection across every I/O surface this
firmware exposes — present and future. The hash domain stays
on path A (framebuffer read-back); the changes are cadence,
coverage, and payload structure.

By the end of this plan:

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
- **Path A measured under the new cadence.** Phase 1 records
  the per-call cost of `crc32c_framebuffer_excluding_digest`
  in practice and the cumulative overhead over the full boot
  transcript. If the overhead is materially worse than the
  estimated ~210 ms (~3.5% of a 6 s transcript), phase 1 is
  the bail-out point: either thin the cadence (refresh every
  other line, batch at sub-phase boundaries) or revisit an
  intent-CRC / shadow-framebuffer alternative as a follow-up
  plan.
- **Multi-channel TLV.** The wire format grows new TLV record
  types for per-channel rolling hashes — one record per
  channel, each carrying a 4-byte CRC32C of that channel's
  event stream since boot. Initial channels: keypresses,
  bootloader decisions, paste records, mode switches. The
  display channel continues to be represented by the existing
  framebuffer-hash trailer (path A). Channel IDs in a
  documented reserved range so future USB-redir / pointer /
  audio channels slot in without a schema bump (TLV's whole
  point).
- **Capacity strategy decided and documented.** With per-line
  refresh, the question of whether the QR carries raw recent
  events plus rolling-hash summaries, or rolling-hash
  summaries only, has to be resolved before encoding hits the
  V5/L wall. Default below; revisit in phase 2.
- **AWAITING gains a QR from the moment chrome is painted.**
  Pre-keypress wedge is a real failure mode and is currently
  invisible.
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
  TLV types today (verify in phase 3) so the new records
  don't break its current parser; whether ryll uses them for
  assertions is a ryll-side decision tracked in their repo.

## Open questions

Defaults below are strong but worth confirming or iterating at
the relevant phase. Capture changes inline rather than letting
them drift.

- **Per-refresh cost under per-line cadence.** Each refresh
  runs `crc32c_framebuffer_excluding_digest` (~7 ms at 3 GHz
  per the phase-2 measurement note) plus a QR encode (a few
  ms) plus ~37×37 ≈ 1400 BLT calls to paint the modules. The
  estimated per-boot overhead is ~210 ms over a 6 s transcript
  (~3.5%). **Default: refresh every line; phase 1 measures
  the actual end-to-end overhead and confirms it stays under
  ~5% of transcript wall-clock before locking the cadence
  in.** If measurement comes in materially worse, the fall-back
  is every-other-line or sub-phase batching; the further
  fall-back (a shadow-framebuffer intent CRC) is deferred to a
  follow-up plan.

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
  refresh. Phase 1 should walk `bootloader.rs` end-to-end and
  produce the final list.

- **TLV capacity strategy.** With multi-channel rolling
  hashes added (5 channels × 6-byte records = 30 bytes), the
  92-byte TLV body has ~62 bytes left for raw events. That's
  ~10 small events. Two options: **(a)** stay at V5/L, cap
  raw events at "last 8 that fit", treat the rolling hashes
  as the primary diagnostic; **(b)** bump to V10/L (271
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

- **Why QR and not a denser code (JAB, HCCB, colour-QR)?**
  **Default: stay on QR; do not consider polychrome or
  higher-density 2D codes for this work or for foreseeable
  follow-up.** The density argument for JAB Code
  (ISO/IEC 23634:2022, ~3–4× QR density at 8 colours) is real
  but is dominated by four pipeline-specific costs:
  - *Colour fidelity through SPICE.* The digest's whole value
    proposition is byte-exact round-trip from server
    framebuffer to client capture. QXL+SPICE negotiates pixel
    format with the client and can compress lossy under
    bandwidth pressure; multicolour regions are where those
    optimisations bite hardest. Monochrome QR survives this
    pipeline trivially; a polychrome code would need
    end-to-end palette accuracy across QXL, SPICE wire format,
    and client-side rendering — and the failure mode for "the
    digest decodes wrong because of colour drift" looks
    identical to "the digest decodes wrong because of an
    actual client-side wedge", which destroys the oracle.
  - *Decoder ecosystem.* `zbarimg`, every Rust QR crate, and
    every test-driver QR pipeline decodes QR. JAB has
    essentially one decoder — Fraunhofer's reference C library
    `libjabcode` (LGPL 2.1). Ryll would either bind to it
    (LGPL has dynamic-linking implications for ryll's
    distribution) or port from scratch. Our spec already says
    ryll's decoder is its own problem; we shouldn't actively
    make it harder.
  - *Rust no_std encoder availability.* No no_std Rust JAB
    encoder is known to exist. The firmware would have to
    pre-compute encodings (defeats per-line refresh), call out
    to C (drags in `libjabcode` plus an allocator we don't
    currently need), or port the encoder ourselves — a
    multi-week side quest for a no_std target.
  - *Capacity headroom we do not need.* V10/L (this plan's
    bump) carries 271 bytes, sufficient for ~10 channels'
    worth of rolling hashes plus ~25 raw events. QR scales to
    V20/L = 858 bytes or V40/L = 2953 bytes before running
    out of the QR design space entirely. The density wall is
    far away.

  **Escape hatch if we ever hit a real capacity wall:**
  multi-frame QR (rotating slices carrying channel-slice +
  frame index, decoder reassembles over a few captures) gives
  effectively unlimited capacity at any QR version and works
  with the existing decoder ecosystem. That is a cheaper
  escape than switching encoding, and the multi-frame option
  would also be available if we ever did switch to JAB —
  switching encoding first does not unlock anything we cannot
  unlock more cheaply by staying on QR.

- **What channels exist on day one.** **Default: keypresses,
  bootloader decisions, paste records, mode switches.** These
  are exactly the non-display event variants the firmware
  records today (`src/event.rs`'s `Event` enum). The display
  channel continues to be the existing framebuffer-hash
  trailer (path A); it is *not* duplicated as a per-channel
  rolling hash. Channel IDs: 0x01–0x0F reserved for these
  "boot-time core" channels; 0x10–0x1F reserved for future
  SPICE-channel-derived state (USB redir, pointer, audio,
  smartcard, clipboard, etc); 0x20+ available for whatever
  else later.

- **Per-channel hash domain.** **Default: CRC32C of the
  concatenated TLV-encoded event records for that channel,
  in chronological order, from boot.** Same algorithm as the
  existing framebuffer hash, so the host-side decoder only
  needs one implementation. "From boot" rather than "last N"
  means the hash diverges on the *first* missed event and
  stays diverged, which is the right semantic for "flag that
  something went wrong" — last-N would mask transient
  divergences once they scroll out of the window.

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
| 1. Refresh cadence + coverage (per-line, AWAITING, bootloader carve-out replacement) + path-A cost measurement | [PLAN-continuous-digest-phase-01-cadence.md](PLAN-continuous-digest-phase-01-cadence.md) | Complete (commits 8814fab through 06f6d1c). Bail-out criterion PASS at 3.1% overhead (74 refreshes / 434 ms over a ~14 s smoke transcript). |
| 2. Multi-channel TLV + capacity decision (V5/L vs V10/L) | [PLAN-continuous-digest-phase-02-multi-channel.md](PLAN-continuous-digest-phase-02-multi-channel.md) | Complete (commits 7df2a7b through a3639a5). Capacity decision: stay V5/L (parent's V10/L default overturned — doesn't fit at 640×480 without overlapping bootloader content); 8 rolling-hash records consume 48 bytes leaving 44 bytes for raw events; CRC chaining math empirically verified. |
| 3. Docs, decoder coordination, closeout | [PLAN-continuous-digest-phase-03-closeout.md](PLAN-continuous-digest-phase-03-closeout.md) | Complete (commits 8de8b03 through this closeout). Wire-format spec rewritten for schema v2; DESIGN.md reframed; ARCHITECTURE.md + AGENTS.md cadence updated; parent-plan capacity typos fixed; ryll's decoder verified as a future-work item (no current decoder, so v2 bump is harmless). |

### Phase 1 sketch — refresh cadence + coverage

Distribute `refresh_digest` calls to the right places so the QR
becomes a continuously-available oracle, remove the bootloader
carve-out, and measure path A's actual cost under the new
cadence. No wire-format changes; the QR payload schema stays
exactly as it is today, only its *value* updates more often.

Sub-steps roughly:

- Measure first: instrument `Scene::refresh_digest` to record
  wall-clock per call (and break down read-back vs encode vs
  paint), boot the existing scene unmodified, and record
  baseline numbers. This is the calibration data the bail-out
  decision below depends on.
- Per-line refresh in the boot transcript runner.
- AWAITING refresh on every cursor blink transition.
- PARKED refresh on the SYSTEM ONLINE line and on every blink
  transition.
- Walk `bootloader.rs` and add the refresh-points listed in the
  *Bootloader refresh-point placement* open question. Remove
  the `assert!` in `Scene::refresh_digest`.
- Update `RepaintState::BootingBootloader`'s repaint flow to
  also refresh on completion (it can no longer be the
  unreachable case it currently models).
- Update `Scene::repaint` if necessary so a mode switch
  mid-bootloader-scene doesn't leave the QR stale.
- Re-measure under the new cadence and record the cumulative
  overhead in the phase closeout. **Bail-out criterion:** if
  the overhead exceeds ~5% of transcript wall-clock, do not
  proceed to phase 2 with this cadence; instead, either thin
  to every-other-line / sub-phase batching (and re-measure) or
  open a follow-up plan for an intent-CRC / shadow-framebuffer
  alternative.
- Re-run `make digest-payload-smoke` after each refresh-site
  addition to confirm no regression in the existing format.

### Phase 2 sketch — multi-channel TLV

Extend the TLV format with rolling-hash records, decide the
capacity strategy, and bump the QR version if (b) wins.

- Add TLV tag constants for per-channel rolling hashes
  (`TAG_HASH_KEYPRESS`, `TAG_HASH_BOOTLOADER_DECISION`, ...)
  in the reserved 0x10–0x1F range — or pick a different range
  now if that conflicts with anticipated SPICE-channel state
  tags. The display channel uses the existing framebuffer-hash
  trailer; no new tag for it.
- Maintain a per-channel CRC32C in `Scene` (or in a new
  `ChannelHashes` struct) updated every time an event is
  pushed to the ring buffer.
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

### Phase 3 sketch — docs, decoder coordination, closeout

- Update `DESIGN.md`'s framing of the on-screen digest.
- Rewrite the `docs/visual-digest-format.md` TLV catalogue.
- Update `ARCHITECTURE.md`'s refresh-cadence paragraph and
  the description of what the digest's hash represents.
- Update the bootloader carve-out note in `AGENTS.md`.
- Verify ryll's decoder ignores unknown TLV records gracefully
  (read ryll-side code; do not push changes there). If it
  doesn't, this becomes a coordination note for the ryll repo
  rather than a blocker.
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
Phase 2 (wire-format changes, possible QR-version bump) is the
best candidate for worktree isolation. Phase 1's coverage
changes and phase 3's doc edits are localised enough to land
in the main tree.

### Planning effort

The master plan itself is at **high effort** (this document).
Phase plans should specify effort per step. Recommended:

- **Phase 1** — medium for planning. Refresh-point placement
  in `bootloader.rs` requires careful walking but the criteria
  are well-defined in this plan. The measurement sub-step is
  mechanical.
- **Phase 2** — high for planning. The capacity decision
  cross-cuts encoder, renderer, decoder, and downstream parsers;
  the channel-ID conventions are an API surface we should not
  re-cut later.
- **Phase 3** — low for planning, medium for execution. The doc
  updates are mostly mechanical but cross-reference several
  source files.

**Model choice:** Phase 2's wire-format design should use
**opus**. Most implementation steps can use **sonnet** with a
detailed brief. No phase warrants haiku.

### Management session review checklist

After each sub-agent completes, verify:

- [ ] The files that were supposed to change actually changed
      (read them, don't trust the summary).
- [ ] No unrelated files were modified.
- [ ] `pre-commit run --all-files` is green.
- [ ] `make digest-payload-smoke` still passes (after phase 2,
      with updated assertions).
- [ ] The QR is visually present in the expected places
      (screenshot via `make screenshot` after phase 1).
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
* The QR's framebuffer hash continues to come from path A
  (`crc32c_framebuffer_excluding_digest`); phase 1's
  measurement confirms the per-line cadence stays under ~5% of
  transcript wall-clock.
* The QR carries per-channel rolling hashes for keypresses,
  bootloader decisions, paste records, and mode switches, in
  addition to the raw recent events it already carries.
* `make digest-payload-smoke` passes against the new format.
* `pre-commit run --all-files` is green.
* `DESIGN.md`, `ARCHITECTURE.md`, `AGENTS.md`, and
  `docs/visual-digest-format.md` reflect the new framing,
  cadence, and TLV record types.

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
* Server-side debug logging via OVMF debugcon (I/O port 0x402)
  + Simple File System Protocol writes to the ESP. The
  QR-mismatch debug story today is "halt CI, dump client log,
  step in for interactive iterative debug against a live
  server" — iterative because the server-side perspective is
  unavailable post-mortem. Two cheap server-side persistence
  paths exist and aren't currently used:
  - **Debugcon (port 0x402).** OVMF's standard debug channel.
    A few `outb`-equivalent lines wired into a `dbg!`-style
    macro give us streaming verbose logging captured by QEMU
    via `-debugcon file:dist/debug.log`. Survives firmware
    death, no protocol opens, no contention with the existing
    serial drain. Covers every cloud we care about
    (OpenStack, Shaken Fist, Proxmox, oVirt are all
    QEMU-based); not portable to bare-metal UEFI or Hyper-V
    but CI-portable is what matters here.
  - **ESP file writes.** Heavier (Simple File System Protocol
    open + file handle lifecycle) but writes survive in the
    boot disk image and can be extracted post-mortem by
    mounting `dist/uncalibrated-sextant.img`. Right shape for
    structured snapshots at the failure moment (compressed
    framebuffer, ring-buffer dump, refresh-stats history).

  The natural split is debugcon for streaming `dbg!`-style
  output and ESP files for structured snapshots. Worth a
  follow-up plan once phase 1 lands — the QR oracle tells the
  client *that* something diverged; this work would tell the
  human investigator *what the server's side of the story was*
  before they start iterating, collapsing the debug loop from
  many round trips to one.

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
