# Continuous multi-channel visual digest — phase 2: multi-channel TLV + capacity decision

Parent plan:
[PLAN-continuous-digest.md](PLAN-continuous-digest.md).

## Prompt

Before responding to questions or implementing any step, read
the parent plan's *Mission and problem statement* (the
multi-channel TLV bullet) and the four *Open questions* most
relevant to this phase: *TLV capacity strategy*, *Why QR and
not a denser code*, *What channels exist on day one*, and
*Per-channel hash domain*. The capacity strategy default in
the parent plan ("bump to V10/L") is the headline decision
this phase has to either confirm or overturn; **see this
plan's *Open questions* below for the 640×480-fit concern
that may force overturning it.**

Read these source files end-to-end so the steps are grounded
in code that actually exists:

- `src/digest.rs` — the TLV encoder. Particularly `encode`
  (around line 195), the `size_of_record` function (line 165),
  the `TAG_*` constants (lines 37–58), `DIGEST_PAYLOAD_CAPACITY`
  (line 89), `DIGEST_SCHEMA_VERSION` (line 33), and the
  `_: () = assert!(...)` capacity guard (line 98).
- `src/renderer/mod.rs` — particularly the digest geometry
  constants at lines 102–116 (`DIGEST_QR_VERSION`,
  `DIGEST_QR_MODULES`, `DIGEST_MODULE_PX`, `DIGEST_REGION_PX`,
  `DIGEST_REGION_X`, `DIGEST_REGION_Y`) and the compile-time
  `assert!` block at lines 126–141 that pins the 640×480
  baseline fit. `Renderer::draw_digest` (around line 478) uses
  `QrCodeEcc::Low` and the `qrcodegen_no_heap::Version` pinned
  via `DIGEST_QR_VERSION_V`.
- `src/event.rs` — the `Event` enum and `RingBuffer::push`.
  The plan adds per-channel rolling hash accumulators that
  update on every push; the push call sites need to route
  through whatever wrapper this phase introduces.
- `src/scene.rs` — every `self.ring.push(Event::...)` call
  site. There are many. The `DigestRefresher` introduced in
  phase 1 is the precedent for "small struct holding refresh-
  adjacent state on `Scene`"; `ChannelHashes` (or whatever
  this phase calls it) will follow the same pattern.
- `src/bootloader.rs` — also pushes events; needs to route
  through the wrapper.
- `scripts/digest-payload-smoke.sh` — the host-side decoder
  harness. Lines 1–60 document the assertions it makes today.
  This phase extends those to recognise the new TLV tags and
  to validate the channel hashes round-trip-decode correctly.
- `docs/visual-digest-format.md` — the wire-format spec.
  Phase 3 (docs closeout) rewrites it; phase 2 should at
  least sketch what the new records look like so the spec
  rewrite has a complete description to work from.

External: QR Code 2005 spec Table 7 (byte-mode capacity by
version × ECC level — V5/L=106, V10/L=213, V15/L=412, V40/L
max=2953). The `qrcodegen_no_heap::Version` type accepts
1..=40; bumping is a constant change plus the buffer-size
re-derivation that already lives in
`DIGEST_QR_BUFFER_LEN = DIGEST_QR_VERSION_V.buffer_len()`.

All planning documents live in `docs/plans/`. Phase plans are
in this directory and tracked in the parent plan's *Execution*
table. One commit per logical change; each commit builds,
passes `pre-commit run --all-files` and `make
digest-payload-smoke`, and has a clear message in the
project's existing style.

## Situation

Phase 1 made the QR digest a continuously-valid client-comparison
oracle by adding refresh sites everywhere the scene's visible
state changes. The TLV *content* did not change: the payload is
still 10-byte header + raw event records (up to ~92 bytes worth,
newest-first selection) + 4-byte CRC32C trailer over the
non-digest framebuffer pixels.

The headline gap that remains: **raw event records that scroll
out of the QR's per-frame budget are invisible to the oracle**.
At V5/L's 92-byte body, a single bootloader paste flow already
fills the budget with Keypress records (one per echoed character
× 23 characters × 14 bytes/record = 322 bytes wanted vs. 92
bytes available); the oldest events get evicted. A test driver
that wants to assert "the server received keypress X" has to
catch the QR frame where X is still in the window, which is
racy and contradicts the "snapshot at any time is meaningful"
goal of phase 1.

The fix is per-channel rolling hashes: a small fixed-size
record per channel (event variant) that CRC32Cs the channel's
event stream from boot. The hash diverges on the first missed
event and stays diverged, so a test driver can confirm "the
server saw every keypress I sent" by comparing the channel
hash to its locally-computed expectation, regardless of which
raw records survived the budget cut.

The parent plan's *Capacity strategy* default was "bump V5/L
→ V10/L (213 bytes) so rolling hashes plus a comfortable
window of raw events both fit." That default was made under
the assumption that the larger QR region (228×228 px at
4-px-per-module) would fit the bottom-right corner across all
supported GOP modes. **A back-of-envelope check during phase 2
planning surfaces that V10/L at 4 px/module does not fit at
640×480 without overlapping bootloader content** — the
`Decryption of next-stage bootloader failed. (R)etry, (I)gnore,
or (A)bort?` prompt is 74 chars × 8 px = 592 px wide,
extending well into where the V10 QR's right-anchored region
would sit. Smaller module scales (3 px, 2 px) shrink the QR
but a 2 px module is at the edge of `zbarimg`'s reliable
decode range and a 3 px V10 still overlaps the prompt row's Y
range. See step 2a for the formal evaluation.

The likely outcome, pending step 2a: stay at V5/L, accept that
rolling hashes consume ~30–50 bytes of the 92-byte body, and
truncate raw events accordingly. The parent plan's *Operator
framing* ("we just need to flag that an error occurred") covers
this — the rolling hashes are the primary diagnostic; raw
events become the secondary context window.

## Mission

By the end of phase 2:

- A `ChannelHashes` (or similarly-named) struct on `Scene`
  carries one CRC32C accumulator per hashed event channel.
  Every `Event` push that affects a hashed channel updates
  its accumulator in lock-step. The accumulators are updated
  via a centralised push wrapper, not by scattering update
  calls across every push site.
- The TLV format gains rolling-hash records using tag values
  in the parent plan's reserved 0x10–0x1F "boot-time core
  channels" range. Each record is fixed-size: tag (1) + len
  (1) + channel CRC32C (4) = 6 bytes per channel. The
  per-channel rolling-hash records appear in the encoded
  payload *before* the raw event records, so they survive
  capacity truncation.
- `DIGEST_SCHEMA_VERSION` is bumped from 1 to 2. The bump
  signals the new tag conventions; raw event tag values
  (0x01–0x08) are unchanged and the header / trailer formats
  are unchanged, so the bump is informational rather than a
  hard break — a v1-only decoder reading a v2 payload sees
  the new tag types but doesn't know what they mean and
  should ignore them. (Verify ryll's decoder ignores unknown
  tags gracefully in phase 3.)
- Either the QR version is bumped (V5/L → V10/L or
  intermediate) and the renderer geometry / compile-time
  asserts updated to match, OR raw event truncation is
  implemented so the encoder gracefully drops raw event
  records when the rolling-hash records would otherwise
  push the payload past V5/L's capacity. The decision lives
  in step 2a; either action lives in step 2d.
- `make digest-payload-smoke` validates the new TLV records:
  recognises the new tag values, parses out per-channel
  hashes, and (where the harness has enough information)
  cross-checks the hashes against the events it expects from
  the scripted scene. Existing assertions stay green.
- The `_: () = assert!(...)` capacity guard in `src/digest.rs`
  is updated to reflect whichever capacity strategy step 2a
  picks. If V5/L stays, the guard stays. If V10/L wins, the
  guard's allowed maximum rises in step with
  `DIGEST_PAYLOAD_CAPACITY`.
- Phase 3 (docs + closeout) has a phase-2 outcome to write
  about and a phase-2 commit range to link from the parent
  plan's execution table.

Out of scope for phase 2:

- ryll-side decoder changes (phase 3 verifies; coordination
  with the ryll repo is independently scheduled).
- Future channels (USB redir, pointer, audio, smartcard).
  Phase 2 reserves the 0x10–0x1F range for boot-time core
  channels and the 0x10–0x1F range for SPICE-channel-derived
  future channels per the parent plan; the actual records
  for those channels land with the work that introduces them.
- Display-channel hash (still the existing trailer; no
  duplication as a per-channel rolling-hash record).

## Open questions

Defaults are strong but worth confirming. Capture changes
inline.

- **Capacity strategy (the headline decision).** **Decided
  in step 2a (see *Step 2a — Capacity feasibility study*
  below): stay at V5/L at 4 px/module and truncate raw
  events. Eight rolling-hash records (48 bytes) plus 14
  bytes header/trailer leave a 44-byte raw-event budget
  (~3 records per frame); the rolling hashes are the
  primary diagnostic and raw events are secondary
  localisation context.** This overturns the parent plan's
  V10/L default.

  Reasoning summary (full analysis in the step 2a section):
  every QR version+scale that fits the 640×480 corner at
  ≥ 3 px/module overlaps the bootloader content rows at
  least as badly as V5@4 already does, and the larger
  bumps (V6+) additionally eat into the per-line refresh
  region of the PRE-script transcript. The rolling-hash
  mechanism makes a larger raw-event window unnecessary.

  Alternatives kept on the record:
  - **V10/L at 4 px/module with a 640×480 content-layout
    change** (shorter prompt, narrower echo, moved
    countdown). Achievable but invasive; not justified
    while the rolling hashes carry the diagnostic load.
  - **V10/L at 3 px/module.** Triples the raw-event
    budget (~14 events) at a 15 px region-size increase,
    but trims decoder safety margin against capture noise.
    Worth revisiting if rolling hashes prove insufficient
    and a content-layout change is on the table anyway.
  - **Intermediate V6–V9 at 4 px/module.** Each step up
    obscures more of the PRE-script transcript without
    proportionally improving the raw-event budget.
  - **Mode-aware QR version (V5 on small modes, V10 on
    large)** is **rejected up-front**: payload size that
    varies by mode means downstream decoders have to handle
    two budgets and the cross-frame consistency story gets
    ugly.

- **Which `Event` variants get per-channel rolling hashes?**
  **Default: hash all eight existing event tags** (Keypress,
  LineRendered, SceneTransition, BootloaderDecision,
  PasteReceived, BootloaderTimeout, ModeSwitch, ModeCycle).
  Tag assignments in the 0x10–0x1F reserved range, paralleling
  the raw tag numbering:

  | Channel              | Raw tag | Hash tag |
  |----------------------|---------|----------|
  | Keypress             | 0x01    | 0x11     |
  | LineRendered         | 0x02    | 0x12     |
  | SceneTransition      | 0x03    | 0x13     |
  | BootloaderDecision   | 0x04    | 0x14     |
  | PasteReceived        | 0x05    | 0x15     |
  | BootloaderTimeout    | 0x06    | 0x16     |
  | ModeSwitch           | 0x07    | 0x17     |
  | ModeCycle            | 0x08    | 0x18     |

  Eight hash records × 6 bytes = 48 bytes. Plus 14 bytes
  fixed overhead = 62 bytes used; V5/L's 106-byte payload
  leaves 44 bytes for raw events (~3 records at typical
  ~14 bytes apiece).

  Alternative: hash only the "I/O ingress" channels (drop
  LineRendered and SceneTransition since their content is
  scene-internal and the display channel's framebuffer hash
  already covers the visible-state effect). Six hashes × 6
  bytes = 36 bytes, leaving 56 bytes for raw events (~4
  records). **Worth considering in step 2a as a trade against
  raw-event window size.** Don't over-optimise though — the
  drop from "8 hashes, 3 raw" to "6 hashes, 4 raw" is one
  extra raw event per frame and two fewer diagnostic
  channels.

- **Encoded ordering and raw-event truncation policy.**
  **Default: rolling-hash records first (immediately after
  header, before raw events), raw events second (newest-first
  selection, oldest-first eviction when budget exhausted),
  trailer last.** This guarantees the channel hashes always
  fit — they're the primary diagnostic and the parent plan's
  reason for the format extension.

  Raw event eviction policy: keep the existing
  newest-to-oldest walk in `digest::encode` (line 216-ish);
  reduce the available `RECORD_BUDGET` by the size of the
  rolling-hash block (48 bytes if hashing all 8 channels).
  No per-tag prioritisation — the newest events are the most
  diagnostic regardless of tag.

  An alternative — **drop LineRendered records from the raw
  payload entirely** since the framebuffer hash already
  covers their visible effect — would buy back ~20 bytes
  per smoke scene at typical density. **Consider in step 2c
  if the raw-event window feels too tight after 2a's
  capacity decision lands.** Trade is loss of "which row was
  rendered when" diagnostic detail in the QR, which is
  mostly available via the framebuffer hash anyway.

- **Per-channel hash semantics: from-boot vs sliding window?**
  **Default: from boot.** CRC32C is updated for every event
  push of that channel, monotonically, never reset. Hash
  diverges on the first missed event and stays diverged,
  giving the test driver a sticky "things went wrong here"
  signal. A sliding-window hash (e.g., last N events) would
  silently re-converge once the divergent events scroll
  out, masking transient bugs.

  Decoder consumer (ryll) compares the hash between
  consecutive QR frames: same hash → no new events of that
  channel arrived; changed hash → new events arrived. Test
  driver knows what it sent; reasons about whether the hash
  changed as expected.

- **Where `ChannelHashes` lives and how push routes through
  it.** **Default: a `ChannelHashes` struct on `Scene`,
  initialised in `Scene::new`, updated by a centralised
  `push_event` wrapper that does both `self.ring.push(event)`
  and the appropriate channel-hash update.** The wrapper lives
  on `Scene`; the bootloader gets a `&mut ChannelHashes`
  threaded into `bootloader::run` (paralleling phase 1's
  `&mut DigestRefresher` plumbing) plus access to the ring
  buffer (which it already has) — or, more cleanly, the
  bootloader stops calling `self.ring.push` directly and
  routes through a `&mut EventStream` (or equivalent) that
  owns both the ring and the channel hashes.

  Step 2b decides the exact shape; the borrow-checker may
  prefer one organisation over another. Phase 1's
  `DigestRefresher` is the working precedent.

- **Per-channel record format on the wire.** **Default: tag
  (1 byte, 0x11..=0x18) + len (1 byte, = 4) + CRC32C value
  (4 bytes, LE) = 6 bytes per channel record.** No explicit
  channel-id field — the tag IS the channel identifier, and
  the tag-to-channel mapping is part of the schema. Mirrors
  the existing tag+len+value layout of raw event records;
  decoders that already walk the TLV body need no special
  parsing — just a new tag-value handler that pulls 4 bytes
  as a u32 LE.

- **Schema version bump: 1 → 2.** **Default: bump to 2.**
  Mechanically a no-break addition (new tags in a reserved
  range; existing tags unchanged), but the new tag values
  carry semantics (channel-hash conventions) that a v1
  decoder cannot interpret. A bump signals "new conventions
  apply" and lets a strict decoder refuse v1 if it expects
  v2 records to be present. Update the `DIGEST_SCHEMA_VERSION`
  constant and any spec references.

- **Smoke harness validation depth.** **Default: parse and
  surface the per-channel hash values in the smoke output;
  do *not* attempt to recompute and verify them against the
  events the scripted scene emitted.** Recomputation would
  require the smoke to know the exact event ordering the
  scripted boot produces, including paste-loop keystroke
  cadence — fragile. Parse-and-surface gives a smoke that
  fails loudly on tag misordering or record size errors;
  recomputation can be added later if a regression escapes
  parse-and-surface.

  Beyond the smoke: a separate `make digest-multichannel-
  smoke` (or extension to the existing smoke) that drives a
  known-deterministic scene and verifies the channel hashes
  exactly would be a useful regression net — but it's a
  *next-phase* item, not phase-2 core.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 2a   | high   | opus   | none      | Capacity feasibility study. Compute V6/L, V7/L, V8/L, V9/L, V10/L region geometry at 4 px/module and at 3 px/module; for each, check overlap against (i) the top-right logo (y ≤ 144 at 640×480), (ii) the bottom toast row (y ≥ 464 at 640×480), and (iii) the widest bootloader-scene content (the 74-char R/I/A prompt at row 21+2 = ~y 384–400, plus the 64-char paste echo at row 21+6 = ~y 432–448). Compute decoded payload capacity for each. Recommend a capacity strategy (likely "stay V5/L + truncate raw events", possibly "bump to V8/L or V9/L if a smaller module scale + slight content-layout change can be made to fit", possibly "V10/L if a content-layout change at 640×480 is acceptable"). Update this plan's *Open questions* entry for capacity strategy with the final decision and reasoning. No code changes in this step — it is research that conditions step 2d. |
| 2b   | medium | sonnet | none      | Add a `ChannelHashes` struct (or similar) in `src/scene.rs` carrying eight CRC32C accumulators (one per event tag) — use the same `Crc::<u32>::new(&CRC_32_ISCSI)` constructor that `src/digest.rs::CRC32C` already exposes. Add as a `Scene` field, initialised in `Scene::new`. Introduce a centralised push helper — either a method on `Scene` (`fn push_event(&mut self, event: Event)`) or a small `EventStream` struct that owns both the ring and the hashes (mirroring phase 1's `DigestRefresher` shape). Route every `self.ring.push(Event::...)` site in `src/scene.rs` and `src/bootloader.rs` through the new helper. The bootloader needs the helper passed in via `bootloader::run`'s signature (same pattern as phase 1's `&mut DigestRefresher` threading). No wire-format changes in this commit — the hashes accumulate but the encoder doesn't read them yet. Verify `pre-commit` + `make digest-payload-smoke` stay green; the smoke output's existing event records should be unchanged. |
| 2c   | medium | sonnet | none      | Extend `src/digest.rs`: add `TAG_HASH_*` constants (0x11..=0x18, matching the raw tag numbering table in this plan's open questions). Add a `RECORD_HASH_SIZE = 6` constant. Bump `DIGEST_SCHEMA_VERSION` from 1 to 2. Extend `encode` to accept a `&ChannelHashes` argument and emit one 6-byte record per hashed channel immediately after the header (in tag-numeric order — 0x11, 0x12, ..., 0x18), then the raw-event records in the remaining budget, then the trailer. Reduce the `RECORD_BUDGET` constant by `8 * RECORD_HASH_SIZE = 48` bytes so raw-event selection respects the new overhead. Thread `&ChannelHashes` through `DigestRefresher::refresh`. Update `scripts/digest-payload-smoke.sh` to recognise the new tag values (0x11..=0x18, len=4 each), surface the decoded hashes in the success line, and validate the version field is 2. **Do not** attempt to cross-check the hashes against the scripted scene's events. Update the per-tag check in the smoke from "0x01..=0x08" to "0x01..=0x08 \|\| 0x11..=0x18" or equivalent. Verify smoke passes; verify `count` of records in the smoke output reflects the eight new hash records. |
| 2d   | medium | sonnet | worktree  | Capacity action per step 2a's outcome. **If 2a recommends staying V5/L:** confirm the encoder's truncation behaviour drops raw events gracefully when the rolling-hash records consume their share of the budget. Add a defensive test or smoke assertion that the encoder does not panic at full ring occupancy. No constant changes needed. **If 2a recommends a bump:** update `DIGEST_QR_VERSION` (renderer), `DIGEST_QR_MODULES` (renderer), `DIGEST_PAYLOAD_CAPACITY` (digest), the `_: () = assert!(...)` capacity guard in `src/digest.rs`, the `DIGEST_QR_BUFFER_LEN` derivation (already auto-resolves from `Version::buffer_len()`), and the compile-time `assert!` block in `src/renderer/mod.rs` (lines 126–141) that pins the 640×480 fit. Update `scripts/digest-payload-smoke.sh`'s zbarimg invocation if module-pixel scale changed. Re-run `make digest-payload-smoke` and the visual `make screenshot` to confirm the QR renders cleanly at the chosen GOP mode. The worktree isolation is precautionary — wire-format constant changes that propagate wrong are unkind to debug. |
| 2e   | low    | sonnet | none      | Phase closeout. Update the parent plan's execution table row for phase 2 from "Not started" to "Complete (commits <first>..<last>). <Capacity decision summary in one sentence>." Update `docs/plans/index.md`'s row for the continuous-digest master plan from "Phase 1 complete; phases 2–3 pending" to "Phases 1–2 complete; phase 3 pending." Add a *Closeout* section to this phase plan covering commit map, capacity decision + reasoning, any deviations, follow-ups for phase 3. |

Commit granularity: one commit per step. Step 2a is research +
documentation only (no code) — it commits as a plan amendment.
Step 2b is one commit (struct + plumbing). Step 2c is one
commit (encoder + smoke). Step 2d is one commit (capacity
action — small if V5/L stays, larger if a bump is taken).
Step 2e is one commit (closeout).

## Verification

After each step:

- `pre-commit run --all-files` is green.
- `cargo build --release` succeeds (via `make build`).
- `make digest-payload-smoke` passes.

Additionally:

- **After step 2b:** the smoke's event-record output is
  identical to phase 1's (count, tag distribution, hashes
  match). The new accumulators are running but invisible
  on the wire.
- **After step 2c:** the smoke output includes eight new
  tag=0x11..0x18 records, each with len=4. The smoke's
  `count` field (encoded record count) reflects the new
  records.
- **After step 2d (if bump):** the visual `make screenshot`
  shows the new QR size in the bottom-right corner without
  overlapping content at every supported GOP mode (640×480
  is the critical one). zbarimg decodes the larger QR
  reliably.
- **After step 2d (if V5/L stays):** the smoke at full ring
  occupancy (the bootloader paste flow exercises this — 23
  keypress echoes alone exceed the post-overhead raw-event
  budget) does not panic; the encoder cleanly drops the
  oldest raw records.

## Step 2a — Capacity feasibility study

Formal evaluation of the capacity strategy raised in the
parent plan's *TLV capacity strategy* open question and
inherited as the headline decision for this phase. Numbers
computed from QR Code 2005 / ISO/IEC 18004 Table 7 (ECC L,
byte-mode), the `qrcodegen-no-heap` default 4-module quiet
zone, and the renderer constants in `src/renderer/mod.rs`
(`MARGIN_X = MARGIN_Y = 16`, `CELL_W = 8`, `CELL_H = 16`)
combined with `src/bootloader.rs`'s row math.

### Byte-mode capacities (Table 7, ECC L)

| Version | Modules | Bytes |
|---------|---------|-------|
| V5      | 37      | 106   |
| V6      | 41      | 134   |
| V7      | 45      | 154   |
| V8      | 49      | 192   |
| V9      | 53      | 230   |
| V10     | 57      | 271   |

The parent plan quotes V10/L as 213 bytes. That figure is
wrong. Table 7 gives V10/L = 271 bytes (and V15/L = 412,
V20/L = 858, not 666 — the parent plan also has the V20/L
figure off). `qrcodegen-no-heap`'s `Version::new(10)
.buffer_len()` returns 196, but that is the encoder's
codeword scratch buffer, not the user-visible payload
capacity. Use 271 here; phase 3's docs sweep can fix the
parent plan.

### Geometry table

Module count per version is `4*V + 17`. Region px is
`(modules + 8) * px_per_module` (8 = 2 × 4-module quiet
zone). At 640×480 right-anchored the origin is
`region_x = 640 - 16 - region_px` (MARGIN_X = 16) and
`region_y = 480 - 16 - 16 - region_px = 448 - region_px`
(MARGIN_Y + CELL_H = 32, reserving the bottom toast row).
Logo bottom edge sits at y = 144; `region_y >= 144` keeps
the QR clear of it.

| V | px/mod | modules | region_px | region_x | region_y | y ≥ 144 | x ≥ 0 | Decode-OK |
|---|--------|---------|-----------|----------|----------|---------|-------|-----------|
| 5  | 4 | 37 | 180 | 444 | 268 | yes | yes | yes |
| 5  | 3 | 37 | 135 | 489 | 313 | yes | yes | yes |
| 6  | 4 | 41 | 196 | 428 | 252 | yes | yes | yes |
| 6  | 3 | 41 | 147 | 477 | 301 | yes | yes | yes |
| 7  | 4 | 45 | 212 | 412 | 236 | yes | yes | yes |
| 7  | 3 | 45 | 159 | 465 | 289 | yes | yes | yes |
| 8  | 4 | 49 | 228 | 396 | 220 | yes | yes | yes |
| 8  | 3 | 49 | 171 | 453 | 277 | yes | yes | yes |
| 9  | 4 | 53 | 244 | 380 | 204 | yes | yes | yes |
| 9  | 3 | 53 | 183 | 441 | 265 | yes | yes | yes |
| 10 | 4 | 57 | 260 | 364 | 188 | yes | yes | yes |
| 10 | 3 | 57 | 195 | 429 | 253 | yes | yes | yes |

All twelve combinations clear the logo (y ≥ 144) and fit
horizontally (x ≥ 0) at 640×480. The fit story is not the
discriminator — content overlap is.

### Decode-reliability note

`zbarimg` and the rest of the QR decoder ecosystem need
roughly three pixels per module to ride out scan-line noise
and any host-side resampling between QEMU's framebuffer
capture and decode. The 2 px/module scale that would
otherwise let V10 fit comfortably is on the edge of that
budget and is rejected up-front; the table above starts at
3 px/module for that reason. 3 px/module is the floor for
this analysis. 4 px/module is the current production scale
and the safer default; 3 px/module is acceptable but worth
re-validating with a screenshot smoke if it is chosen.

### Bootloader content rows at 640×480

Row math uses `start_row = 19` (the 19 PRE-script lines of
`run_booting` advance `row` by 1 each), giving
`prompt_row = countdown_row = start_row + 2 = 21`,
`input_row = start_row + 6 = 25`, and
`wrong_indicator_row = input_row + 1 = 26`. Pixel y range
of row `R` is `[16 + R*16, 16 + (R+1)*16)` = a 16 px tall
band.

| Content                              | row | y range     | x range (content) |
|--------------------------------------|-----|-------------|-------------------|
| R/I/A prompt (74 chars)              | 21  | [352, 368)  | [16, 608)         |
| Countdown header (same row as prompt)| 21  | [352, 368)  | [16, 608)         |
| Paste echo, target len (23 chars)    | 25  | [416, 432)  | [224, 408)        |
| Paste echo, buffer full (64 chars)   | 25  | [416, 432)  | [224, 736) (clipped at 640) |
| Wrong-indicator                      | 26  | [432, 448)  | [16, ~352)        |

(The 76-column terminal at 640×480 — `(640 - 32) / 8` —
truncates the 64-char echo at ~50 chars visible, so the
echo's effective right edge is ~640 px even though the
buffer holds 64 chars.)

### Overlap matrix (3 px and 4 px per module)

For each fit-eligible (V, scale) the QR occupies
`x ∈ [region_x, region_x + region_px)` and
`y ∈ [region_y, region_y + region_px)`. An OVERLAP requires
both x and y ranges to overlap the content's box.

| V | px/mod | QR x range | QR y range | Prompt row 21 | Echo row 25 | Wrong row 26 |
|---|--------|-----------|-----------|---------------|-------------|--------------|
| 5  | 4 | [444, 624) | [268, 448) | OVERLAP (x [444, 608), y [352, 368)) | OVERLAP (x [444, 624), y [416, 432)) | OVERLAP (x [444, 624) ∩ [16, ~640), y [432, 448)) |
| 5  | 3 | [489, 624) | [313, 448) | OVERLAP (x [489, 608), y [352, 368)) | OVERLAP (x [489, 624), y [416, 432)) | OVERLAP (x [489, 624), y [432, 448)) |
| 6  | 4 | [428, 624) | [252, 448) | OVERLAP (x [428, 608), y [352, 368)) | OVERLAP                                    | OVERLAP                                    |
| 6  | 3 | [477, 624) | [301, 448) | OVERLAP (x [477, 608), y [352, 368)) | OVERLAP                                    | OVERLAP                                    |
| 7  | 4 | [412, 624) | [236, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 7  | 3 | [465, 624) | [289, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 8  | 4 | [396, 624) | [220, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 8  | 3 | [453, 624) | [277, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 9  | 4 | [380, 624) | [204, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 9  | 3 | [441, 624) | [265, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 10 | 4 | [364, 624) | [188, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |
| 10 | 3 | [429, 624) | [253, 448) | OVERLAP                                    | OVERLAP                                    | OVERLAP                                    |

Every fit-eligible combination overlaps every bootloader
content row at 640×480. The current production V5/L at
4 px/module is no exception: it sits at x ∈ [444, 624),
overlapping the right ~164 px of the prompt row and the
last ~50 chars of the echo row. The renderer paints the QR
*after* the bootloader's text, so the QR overpaints
whatever was there — at 640×480, the right portion of the
R/I/A prompt and the trailing characters of any long paste
echo are visually obscured by the QR. **This is the
established baseline.** Phase 1 shipped per-line and
per-bootloader-state refresh against this geometry and the
operator accepted the partial occlusion at 640×480 as the
cost of right-anchored placement.

The relevant criterion for step 2a is therefore not "does
the QR overlap content?" (V5/L already does) but "does a
larger QR overlap *more* content than the operator has
already accepted?" Concretely:

- **V5@4 (baseline)** obscures: right 164 px of prompt
  row, last ~50 chars of full-buffer echo, right portion of
  wrong-indicator. Y range starts at 268 (row 16) — well
  below any of the 19 PRE-script lines.
- **V6@4** drops the left edge to x = 428, obscures
  another 16 px of prompt content (still within the 74-char
  prompt). Y range starts at 252 (row ~15) — encroaches on
  the last PRE-script lines (`SENSORIUM: nominal` etc.).
- **V8@4** drops to x = 396, y = 220 (row ~13). Eats
  another four PRE-script lines and obscures most of the
  prompt's `(I)gnore, or (A)bort?` tail.
- **V10@4** drops to x = 364, y = 188 (row ~11). Obscures
  half the PRE-script boot transcript and most of the
  prompt — the situation paragraph's "doesn't fit"
  conclusion.
- **3 px/module scales** all clear the PRE-script row
  region (y ≥ 265 at V9, ≥ 253 at V10) but every one of
  them still clips the prompt and echo rows because the
  prompt occupies x ∈ [16, 608) — even the smallest
  3 px/module QR (V5 at x = 489) intrudes 119 px into the
  prompt content x-range.

The 3 px/module column does not unlock anything qualitatively
new: the QR shrinks back to roughly V5@4's footprint
(V10@3 = 195 px ≈ V5@4 = 180 px), so we trade decode
reliability headroom for capacity headroom that we could
have gotten without changing the module scale.

### Decoded byte-capacity budget

Subtract from the raw capacity:
- Header + trailer: **14 bytes** (10 header + 4 trailer).
- Eight rolling-hash records × 6 bytes: **48 bytes**
  (phase-2 default per the *Open questions* table).
- Total fixed overhead: **62 bytes**.

Average raw-event record size is ~14 bytes (per
`src/digest.rs::size_of_record`: Keypress=14, LineRendered=12,
SceneTransition=12, BootloaderDecision=15, PasteReceived=13,
BootloaderTimeout=10, ModeSwitch=18, ModeCycle=15 — mean
13.6). The budget then divides cleanly:

| V    | Raw bytes | Post-overhead | Raw events (~14 B each) |
|------|-----------|---------------|--------------------------|
| V5   | 106       | 44            | ~3                       |
| V6   | 134       | 72            | ~5                       |
| V7   | 154       | 92            | ~6                       |
| V8   | 192       | 130           | ~9                       |
| V9   | 230       | 168           | ~12                      |
| V10  | 271       | 209           | ~14                      |

V5/L's 44-byte raw-event budget after eight hash records
is tight: a single bootloader paste flow generates 23
Keypress events × 14 B = 322 B of demand against 44 B of
supply, so ~3 of the last 23 echoes survive in each frame.
The rolling hashes are the safety net that makes this
acceptable (they record *every* echo, just not the raw
content).

### Recommendation

**Stay at V5/L at 4 px/module. Truncate raw events to the
44-byte post-overhead budget. Treat rolling hashes as the
primary diagnostic; raw events are secondary context.**

Reasoning:

1. **No fit-eligible bump qualitatively improves the
   overlap story at 640×480.** Every QR larger than V5@4
   obscures more content than the operator has already
   accepted; every 3 px/module choice still clips the
   prompt and echo rows. The smallest "intermediate" bump
   (V6@4) eats into the PRE-script transcript without
   buying a useful raw-event budget increase (~5 events
   vs ~3).
2. **The raw-event budget loss is what the rolling hashes
   exist to compensate for.** The parent plan's *Operator
   framing* is explicit: the rolling hashes are the
   primary "did the server see this" channel; raw events
   are the localisation aid. A 44-byte raw window covering
   the most recent ~3 events is sufficient for that
   localisation role given that the hashes flag *that*
   divergence happened.
3. **The decode-reliability floor and the content-layout
   constraint together kill the V10 direction.** The only
   way V10 fits without overlapping more than baseline is
   2 px/module, which `zbarimg` does not reliably decode.
   Re-laying out the bootloader scene at 640×480 to make
   room for V10 (shorter prompt, narrower echo, moved
   countdown) is a much larger scope-creep change than this
   phase warrants and the rolling-hash payload structure
   does the diagnostic job without it.
4. **Mode-aware QR version is rejected up-front by this
   plan's *Open questions* entry** — payload size varying
   by mode forces downstream decoders to handle two
   budgets. Not on the table.

Rejected alternatives (kept here for future re-evaluation):

- **V10/L + 640×480 content-layout change.** Would require
  splitting the 74-char prompt across two lines, narrowing
  the echo's column count, and moving the countdown out of
  the prompt row's Y range. Achievable but invasive; the
  rolling-hash mechanism makes the larger raw-event window
  unnecessary.
- **V8/L or V9/L at 4 px/module.** Buys 9–12 raw events
  vs V5's ~3, but eats four to eight PRE-script transcript
  rows (which carry the per-line refresh diagnostic value
  phase 1 just paid for). Net diagnostic value is negative.
- **V10/L at 3 px/module.** 195 px region is only 15 px
  larger than V5@4; the raw-event budget triples (~3 → ~14
  events) but the smaller modules reduce decode safety
  margin against capture noise. Worth revisiting if rolling
  hashes prove insufficient and a content-layout change is
  on the table anyway.

Consequence for the rest of phase 2: step 2c reduces
`RECORD_BUDGET` by 48 bytes (8 channels × 6 B) and trusts
the existing newest-first / oldest-first eviction in
`encode` to handle truncation. Step 2d's "if V5/L stays"
branch is the one we are taking — no `DIGEST_QR_VERSION`,
`DIGEST_PAYLOAD_CAPACITY`, or geometry-assert changes are
required.

## Closeout

(Populated by step 2e. Should include: commit range, capacity
decision and final reasoning, any deviations from the plan,
links to follow-up work if applicable.

Cross-plan note for the phase 3 docs sweep: the parent
plan's *TLV capacity strategy* and *Why QR and not a denser
code* sections quote V10/L = 213 bytes and V20/L = 666
bytes. Table 7 gives V10/L = 271 bytes and V20/L = 858
bytes. Fix in phase 3.)

## Back brief

Before executing step 2a, please back brief the operator as
to your understanding of the plan and how the work you intend
to do aligns with it. Particular points worth confirming:

- The capacity-strategy default flip vs. the parent plan
  (this plan's default is V5/L + truncation; the parent
  defaulted to V10/L). If the operator wants to keep the
  parent's V10/L direction, step 2a needs to investigate
  what content-layout changes would be required at 640×480
  to make it fit.
- The "hash all 8 channels" default vs. "hash 6 channels
  (drop LineRendered + SceneTransition)" alternative.
- The smoke validation depth (parse-and-surface vs.
  recompute-and-verify).
