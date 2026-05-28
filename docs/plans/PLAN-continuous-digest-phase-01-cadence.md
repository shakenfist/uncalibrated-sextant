# Continuous multi-channel visual digest — phase 1: refresh cadence and coverage

Parent plan:
[PLAN-continuous-digest.md](PLAN-continuous-digest.md).

## Prompt

Before responding to questions or implementing any step, read
the parent plan in full — particularly the *Situation*
paragraph that justifies keeping path A and the *Bootloader
refresh-point placement* / *AWAITING refresh trigger* /
*Per-refresh cost under per-line cadence* open questions.
Those three questions are the substantive design content for
this phase; the steps below operationalise them.

Read the following source files end-to-end so the steps are
grounded in what actually exists:

- `src/scene.rs` — particularly `Scene::run`, `run_awaiting`,
  `run_booting`, `play_script`, `run_parked`, `repaint`,
  `refresh_digest`, `blink_until_key`. The
  `RepaintState::BootingBootloader` carve-out lives here at
  lines 631–633 (repaint guard) and 657–661 (refresh_digest
  assert).
- `src/bootloader.rs` — the entire sub-state-machine. Phase 1
  walks it end-to-end to identify the refresh-point placement
  the parent plan's open question defaults already sketch:
  `render_telemetry_preamble`, `render_prompt`, `render_nudge`,
  `run_retry_animation`, `run_blob_and_paste`, `capture_paste`
  (Correct / Wrong / Timeout returns), `run_success`,
  `run_timeout`.
- `src/cursor.rs` — `CursorState::tick` returns
  `Option<[u8; 16]>`; phase 1 needs to detect *transitions*
  in that return value (None↔Some, Some(A)↔Some(B)) to decide
  when to refresh, not just refresh on every poll iteration.
- `src/renderer/mod.rs` — `draw_digest`,
  `crc32c_framebuffer_excluding_digest`, the digest geometry
  helpers. Path A stays unchanged at the renderer level;
  phase 1 changes only how often `Scene::refresh_digest` calls
  the existing read-back.
- `src/serial.rs` — the drain pattern. Phase 1 piggybacks
  measurement output onto the existing drain rather than
  inventing a second sink.

Where a question touches `uefi-rs` semantics — particularly
how to get a high-resolution wall-clock reading in `no_std`
UEFI without a real-time clock — research and pick the
lightest viable mechanism (the *Measurement mechanism* open
question below records the default; revisit if it doesn't
build).

External references: the same QR Code 2005 / `qrcodegen`
material as the parent plan. No new external references are
introduced by phase 1.

## Situation

The visual digest's three existing refresh sites (after
AWAITING ends, after BOOTING ends, after PARKED ends — plus a
fourth inside `Scene::repaint` after a mode switch) are
correct but coarse. The parent plan's mission is to add more
refresh sites so the QR becomes a continuous oracle, while
keeping the path-A read-back as the framebuffer hash source.

The risk this phase has to manage is that path A is not free:
the phase-2 measurement note for the original visual-digest
plan estimated ~7 ms per call at 3 GHz under headless OVMF.
Multiplying that by a per-line cadence over a 6 s boot
transcript gives an expected ~210 ms overhead (~3.5%) — well
inside budget, but unverified under interactive SPICE and
under the full set of refresh sites this phase adds. The
phase begins and ends with measurement so the bail-out
criterion in the parent plan (>5% transcript wall-clock →
do not proceed) is grounded in numbers and not assumption.

Coverage additions land at three categories of site:

1. **The boot transcript** (`play_script`) — one refresh after
   each painted step. Three step variants
   (`SceneStep::Telemetry`, `::Line`, `::Probe`) all share the
   same paint + ring-push + repaint-state-update + stall
   sequence; the refresh slots in once per step, between the
   ring push and the stall.
2. **Blink loops** (`blink_until_key`, used by both
   `run_awaiting` and `run_parked`) — refresh on visible
   transitions in the cursor glyph, not on every poll. Polling
   at `POLL_MS = 50` means 20 polls/sec; refreshing every poll
   would be ~140 ms/sec of refresh cost during the blink loop,
   which is wasteful and visible. Refreshing only when
   `cursor.tick`'s `Option<[u8; 16]>` return value differs from
   the previous tick's return matches "the visible state
   actually changed" (blink on/off transitions plus glitch
   substitutions).
3. **The bootloader sub-state-machine** (`src/bootloader.rs`)
   — explicit refresh points named by phase, plus removal of
   the blanket carve-out (`assert!` in `refresh_digest`,
   matching guard in `Scene::repaint`). The bootloader owns
   its own paint pacing; refresh calls slot in after each
   `draw_*` site that completes a visible state transition,
   not from inside the polling loops.

## Mission

By the end of phase 1:

- `Scene::refresh_digest` is called per painted line in the
  boot transcript, on every visible cursor transition in
  AWAITING and PARKED, after the `SYSTEM ONLINE` line in
  PARKED, and at every named refresh-point inside the
  bootloader sub-state-machine. The framebuffer hash source
  is unchanged (still path A:
  `crc32c_framebuffer_excluding_digest`).
- The `RepaintState::BootingBootloader` carve-out is removed.
  The `assert!` in `Scene::refresh_digest` is gone; the
  matching `if !matches!(... BootingBootloader ...)` guard at
  the end of `Scene::repaint` is gone. The bootloader's
  refresh-points handle the screen-update side; `Scene::repaint`
  during the bootloader scene falls back to PRE-only as it
  does today (the bootloader's row content isn't reconstructable
  from `RepaintState` and the parent plan keeps that as-is).
- Wall-clock measurement of refresh cost is in place and runs
  on every boot, dumping a summary line on the existing serial
  drain. The summary records: number of refresh calls, total
  refresh wall-clock, mean / max / p99 per call. Sufficient
  for the bail-out decision and for ongoing regression watch.
- A baseline measurement (pre-change) and a post-change
  measurement are both recorded in the phase closeout. The
  bail-out criterion is evaluated against the post-change
  number; if it fails, phase 2 does not start and a follow-up
  plan is opened.
- `make digest-payload-smoke` still passes — the wire format
  has not changed, only the cadence.
- The QR is visually present in every scene phase, verifiable
  via `make screenshot` for the still-frame phases and via
  `make spice-ryll` for the interactive bootloader scene.

## Open questions

Defaults are strong but worth confirming as the phase
executes. Capture changes inline.

- **Measurement mechanism.** **Default: `core::arch::x86_64::_rdtsc()`
  for the per-call deltas, calibrated once at startup against
  a known `uefi::boot::stall(Duration::from_millis(100))` to
  recover ticks-per-millisecond.** Rationale: zero
  dependencies, works under `no_std` and inside UEFI without a
  protocol open, and the relative measurement is what matters
  for the bail-out criterion. Alternatives — the UEFI 2.5+
  Timestamp Protocol (`uefi::proto::misc::timestamp`) or
  Runtime Services `get_time` — are heavier and either
  require protocol opens or give 1-second resolution. If the
  rdtsc path turns out to require unstable Rust features or
  doesn't compile cleanly under our toolchain pin
  (`rust:1.88-slim`), fall back to the Timestamp Protocol.

- **Measurement output channel.** **Default: append a
  `refresh_digest_stats` summary line to the existing
  `serial::drain` output, formatted as
  `type=refresh_stats count=<n> total_ms=<n> mean_us=<n>
  max_us=<n> p99_us=<n>`.** Same drain, same parse
  conventions as the existing event records. A new
  `RefreshStats` struct lives on `Scene` and is updated by
  `refresh_digest`; the drain reads it after the per-event
  loop. If the parent's separately-tracked p99 turns out to
  need more than a fixed-size sample (e.g. 256 samples), bump
  the sample buffer; do not switch to dynamic allocation.

- **AWAITING transition detector.** **Default: a `last_glyph:
  Option<[u8; 16]>` field on `Scene` (or threaded through
  `blink_until_key`), updated each loop iteration; refresh
  fires when the new return value differs from the previous
  one by `Option`-tag or by byte equality.** Threading
  through is cleaner than a `Scene` field if it can be done
  without making the closure callers awkward; pick whichever
  has a smaller diff. The `Scene` field would also persist
  across `blink_until_key` invocations (AWAITING → PARKED
  reuse the same `CursorState` so the LFSR carries; the same
  `last_glyph` continuity is arguably correct semantically but
  isn't observable since AWAITING ends the loop on a
  keypress, not on a blink boundary).

- **Bootloader refresh-point list, finalised.** Walking
  `src/bootloader.rs` against the parent plan's defaults, the
  concrete sites are:
  - `render_telemetry_preamble`: refresh after each of the
    two `draw_telemetry_line` + `ring.push` blocks. Two
    refresh calls.
  - `render_prompt`: refresh after the prompt + optional
    `(attempt N)` suffix + optional nudge re-render
    completes. One refresh call.
  - `render_nudge`: refresh after the nudge line is drawn.
    One refresh call. (When called via `render_prompt`'s
    re-render branch, the prompt's own refresh covers it; the
    explicit refresh here matters only when `render_nudge` is
    called standalone at the
    `RETRY_NUDGE_AFTER`-threshold transition.)
  - `run_retry_animation`: refresh after each dot. Eight
    refresh calls per animation; refresh-per-dot is the
    cadence semantic for the animation (the dots ARE the
    visible state changes).
  - `run_blob_and_paste`: refresh after the intro line, after
    the encoded blob, after the input prompt, and after each
    wrong-paste re-render block (input prompt + wrong
    indicator). Four-plus refresh calls per pass.
  - `capture_paste`: refresh after each echoed character.
    Refresh-per-keystroke is the cadence semantic for the
    paste; this is the highest-diagnostic-value refresh in the
    whole scene since "did the client deliver the expected
    scan-code sequence" is the entire test.
  - `capture_paste` outcome → `run_blob_and_paste` event push:
    refresh **immediately after** the `PasteReceived` event is
    pushed and **before** the success/wrong branch. This is
    the parent plan's "paste-correctness ordering" requirement
    made concrete: a screenshot at this moment sees the paste
    content and the validation verdict in the same QR.
  - `run_success`: refresh after the region clear and after
    `Booting...` is drawn. Two refresh calls.
  - `run_timeout`: refresh after the region clear, after the
    countdown prefix is drawn, after each countdown tick
    (~31 calls for the 30s countdown — the digits are the
    visible state), and after the halt line. ~34 refresh
    calls per timeout flow.

  This list is what step 1f below codifies; if walking the
  code surfaces a site that doesn't match the criteria
  ("after every visible state change, not from inside polling
  loops"), update the list before implementing.

- **Refresh inside `capture_paste`'s key polling loop.** The
  rule is "no refresh from inside polling loops" — but
  `capture_paste`'s loop is also the only place per-keystroke
  echo happens, and per-keystroke refresh is the bootloader's
  highest-value oracle moment. **Resolution: the refresh
  fires only on the branch that has just drawn a glyph (the
  printable-ASCII echo branch), not on the idle-poll branch
  or the Enter-terminate branch.** That's not "inside the
  polling loop" in the rule's sense — it's "after a visible
  state change", which happens to be detected by the poll.
  The Enter branch's outcome refresh is the post-event one
  above.

- **Bootloader scene's `clock_ms` vs `digest_frame_counter`
  wrap.** The parent plan flagged this. With per-line refresh
  the counter probably reaches the low tens of thousands per
  full boot transcript; even with adversarial timeout loops
  (countdown + retries × paste echoes) the worst case is a
  few hundred thousand. u32 has headroom for ~10⁹ refreshes
  before wrapping. No downstream parser today reads the
  counter as anything narrower than u32. Default: **confirm
  in step 1a, no code change required.**

- **Mode switch mid-bootloader.** Currently impossible (the
  bootloader's poll loops don't dispatch mode keys). The
  parent plan's *Bootloader refresh-point placement* defaults
  don't add a mode-key dispatch to the bootloader, so the
  `RepaintState::BootingBootloader`-as-PRE-only fallback in
  `Scene::repaint` remains the unreachable-but-modelled
  contract. **Default: leave it as-is.** Removing the assert
  in `refresh_digest` doesn't change the reachability of that
  repaint variant.

## Steps

Per the project template's step-table convention. "Brief" is
the per-step prompt for the sub-agent; it should be sufficient
on its own without re-reading this plan in full.

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | Add a measurement scaffold to `src/scene.rs`. Calibrate `core::arch::x86_64::_rdtsc()` against `uefi::boot::stall(Duration::from_millis(100))` once at scene-start to recover `ticks_per_ms` (store on `Scene`). Wrap `Scene::refresh_digest`'s body in a TSC-bracket and accumulate into a new `RefreshStats { count: u32, total_ticks: u64, max_ticks: u64, sample_ring: [u64; 256], sample_head: usize }` on `Scene`. Extend `serial::drain` to emit a single `type=refresh_stats count=<n> total_ms=<n> mean_us=<n> max_us=<n> p99_us=<n>` line after the per-event loop, computing p99 from the sample ring (sort the ring locally; no_std-compatible). If `_rdtsc()` requires nightly features that conflict with the toolchain pin in `rust-toolchain.toml`, fall back to `uefi::proto::misc::timestamp::Timestamp`; document the choice in the closeout. No cadence changes in this commit — just instrumentation. Run `make spice-ryll` once and confirm the new line appears at the end of `dist/serial.log`. Run `make digest-payload-smoke` and confirm it still passes (the smoke parser will see and ignore the new line — verify this; if it doesn't, fix the parser to skip unknown `type=` lines). |
| 1b   | low    | sonnet | none      | Run the instrumented binary against the existing scene (no cadence changes) end-to-end and record the baseline `refresh_stats` line in `docs/plans/PLAN-continuous-digest-phase-01-cadence.md` under a new "## Measurements" section with subheading "### Baseline (3 refresh sites)". Same for `make digest-smoke` if it still has a different scene shape. This is purely measurement + documentation; no code changes. |
| 1c   | medium | sonnet | none      | In `src/scene.rs::play_script`, add a `self.refresh_digest(renderer);` call after the `self.repaint_state = make_state(idx + 1);` line and before `self.stall_with_keys(renderer, PACE_LINE_MS);`, in all three `SceneStep` arms. This is the per-line refresh. Confirm `make digest-payload-smoke` still passes. Confirm via `make screenshot` (BOOTING transcript advance) that the QR is visibly present. |
| 1d   | medium | opus   | none      | In `src/scene.rs::blink_until_key`, add a `last_glyph: Option<[u8; 16]>` local initialised to `None`. After `Self::draw_or_clear_cursor(renderer, glyph, ...)` and before `self.tick_toast(...)`, compare the new `glyph` to `last_glyph`; if different, call `self.refresh_digest(renderer)` and update `last_glyph = glyph`. Equality is `Option`-tag then byte-wise. This gives transition-driven refresh in both AWAITING (which already has a pre-loop refresh) and PARKED. After `run_parked` draws the `SYSTEM_ONLINE_TEXT` line and before calling `blink_until_key`, add an explicit `self.refresh_digest(renderer);` so the parking screen's initial QR reflects the new line. Confirm via `make screenshot` that the parking screen QR updates as expected and via interactive `make spice-ryll` that AWAITING's QR updates on blink (verify by watching two consecutive screenshots differ in the digest region). |
| 1e   | medium | sonnet | none      | Remove the bootloader carve-out from `src/scene.rs`. Two edits: (1) remove the `assert!` block in `Scene::refresh_digest` (lines around 657–661) and the associated explanatory comment block; (2) change `Scene::repaint`'s tail from the conditional `if !matches!(... BootingBootloader ...) { self.refresh_digest(renderer); }` to an unconditional `self.refresh_digest(renderer);`. Update the doc comment on `refresh_digest` to remove the *Carve-out* paragraph and instead document that the bootloader places its own refresh calls (cross-reference `src/bootloader.rs`). Do not change `RepaintState::BootingBootloader`'s repaint behaviour — the PRE-only fallback stays as a documented unreachable case. Confirm `cargo build` and `make digest-payload-smoke` both succeed. |
| 1f   | high   | opus   | none      | Add the bootloader refresh-points enumerated in the parent plan's *Bootloader refresh-point placement* open question and refined in this plan's *Bootloader refresh-point list, finalised* open question. Walk `src/bootloader.rs` top to bottom and insert `self.renderer`-…-equivalent refresh calls — except `refresh_digest` lives on `Scene`, not `BootloaderScene`, and `bootloader::run` takes `&mut Renderer, &mut RingBuffer<256>, &mut u64` rather than `&mut Scene`. Add a fourth shared reference (`&mut RefreshStats` or `&mut Scene` — pick the minimum-coupling shape) so the bootloader can drive refresh, OR add a free-function `refresh_digest_external(renderer: &mut Renderer, ring: &RingBuffer<256>, frame_counter: &mut u32, framebuffer_hash_fn: …)` and route both `Scene::refresh_digest` and the bootloader's calls through it. The latter is preferred because it keeps `BootloaderScene`'s reference set narrow and makes the refresh path testable in isolation; the former is OK if it ends up smaller. Place refresh calls at every site in the finalised list. After implementation, run `make spice-ryll` and exercise the R/I/A path twice (once correct paste, once timeout) and confirm the QR updates visibly between scan-code echoes. Capture serial output to confirm the post-`PasteReceived` refresh appears before the success/wrong branch's downstream events. |
| 1g   | low    | sonnet | none      | Re-run the instrumented binary with all phase-1 changes in place. Record the new `refresh_stats` line under a new "### Post-change (per-line cadence + bootloader + blink)" subheading in this plan's "## Measurements" section. Compute `total_ms / transcript_ms * 100` and record it. If the result is ≤ 5%, write "Bail-out criterion: PASS" and proceed to phase 2. If > 5%, write "Bail-out criterion: FAIL" and stop — open a follow-up plan for cadence thinning or an intent-CRC alternative and do not proceed with phase 2 until that plan resolves. |
| 1h   | low    | sonnet | none      | Update the parent plan's execution table row for phase 1 from "Not started" to "Complete (commits <first>..<last>)" or similar. Update `docs/plans/index.md`'s row for the continuous-digest master plan if the status format wants per-phase progress reflected (consult existing rows for in-progress masters). Add a brief "## Closeout" section to *this* phase plan summarising: commit range, baseline + post-change numbers, bail-out result, anything surprising. If the bootloader refresh-point list deviated from the finalised list in this plan, document the deviation and why. |

Commit granularity: one commit per step is the default. Step 1a
(scaffold + drain extension) is one commit; step 1f
(bootloader refresh-points) is one commit; the small per-site
steps each get their own. Steps 1b, 1g, 1h are documentation-
only and can be folded into adjacent code commits if the doc
update is purely measurement output, but the closeout (1h)
should land separately so it captures the full commit range.

## Verification

After each step:

- `pre-commit run --all-files` is green.
- `cargo build --release` succeeds.
- `make digest-payload-smoke` still passes.

After step 1d:

- `make screenshot` shows a QR in the AWAITING screen position
  (top-left cursor + top-right logo + bottom-right QR).
- A second `make screenshot` invocation 500 ms later, paired
  with an `imagemagick`-side `compare`, shows the digest region
  has changed (the blink transition refresh fired).

After step 1f:

- `make spice-ryll` exercised with a correct paste produces a
  serial log containing, in order: at least one
  `type=refresh_stats` (the final one), and
  `type=paste correct=true` followed by *no* further QR-relevant
  state changes before the scene transitions. (The
  post-`PasteReceived` refresh fires between the paste event
  and the success branch, so its stats are included in the
  cumulative count.)
- `make spice-ryll` exercised with a timeout (silent wait or
  three wrong pastes) produces a serial log containing the
  `type=bootloader_timeout` event followed by the countdown
  ticks; the post-change refresh count should reflect the
  ~30 countdown refreshes.

After step 1g:

- The bail-out PASS/FAIL is recorded in this plan. If FAIL,
  the phase is paused; if PASS, phase 2 may proceed.

## Measurements

To be populated by steps 1b and 1g.

### Baseline (3 refresh sites)

Captured against the existing 3-refresh-site cadence (commit
8814fab) via `make digest-payload-smoke`. The smoke drives a
scripted scene from AWAITING through the bootloader paste flow
to PARKED, with first event at t=1750 and final keypress at
t=15500 (≈13.75 s of scene timeline).

```
type=refresh_stats count=4 total_ms=22 mean_us=5671 max_us=5786 p99_us=5655
```

Notes:
- `count=4` matches the four existing call sites:
  `run_awaiting`'s pre-blink call plus the three
  scene-phase-boundary calls in `Scene::run`.
- Mean ~5.7 ms / max ~5.8 ms is consistent with the parent
  plan's ~7 ms read-back estimate.
- `total_ms=22` over a ~13.75 s scripted transcript is
  ~0.16% overhead — well under the 5% bail-out budget, but
  this is the *baseline* with only 4 refreshes. Step 1g
  re-measures under per-line cadence (~30+ refreshes).
- p99 with N=4 samples is mathematically weak (the formula
  yields ~75th percentile for small N) and is recorded for
  completeness only; the bail-out criterion uses total_ms.
- The smoke script also rushes pacing (keypresses delivered
  faster than a human would); a real interactive boot would
  show a larger transcript denominator and therefore a
  smaller overhead ratio.

### Post-change (per-line cadence + bootloader + blink)

(Populated by step 1g.)

### Bail-out evaluation

(Populated by step 1g.)

## Closeout

(Populated by step 1h. Should include: commit range,
measurement results, bail-out result, any deviations from the
plan, links to follow-up plans if applicable.)

## Back brief

Before executing step 1a, please back brief the operator as to
your understanding of the plan and how the work you intend to
do aligns with it. Particular points worth confirming:

- The measurement mechanism choice (rdtsc vs Timestamp Protocol).
- The intended shape of the bootloader refresh-routing (free
  function vs threaded `&mut Scene` reference vs a new
  reference on `BootloaderScene`).
- That the bail-out criterion in step 1g really halts the
  phase rather than auto-proceeding to phase 2.
