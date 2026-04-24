# First playable — phase 6: packaging, screenshot, docs

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read the master plan's
*Phase 6 sketch* and *Success criteria* sections — they are the
ceiling for what goes here. Re-skim `ARCHITECTURE.md` and
`AGENTS.md` to see what Phase 5 Step 4 already landed; most of
the documentation surface this phase was originally meant to
produce is already in the tree. Phase 6's remaining work is:
commit a screenshot and wire it into `README.md`, add a small
ring-buffer-to-serial drain so the first-playable milestone
demonstrates end-to-end serial output, and close the
bookkeeping (index, execution table, milestone success
checklist) that declares the first-playable milestone shipped.

This is the smallest phase by code volume. The failure mode is
scope drift: Phase 7-and-beyond material (gRPC framing,
Ryll-facing channel, pointer handshake, real audio, mode walk,
scanline overlay, serial-driven scenes) is explicitly **not**
part of this phase. Capture ideas under *Future work* here or in
the master plan's *Future work* section and move on.

Cross-refs in-repo:

- `src/scene.rs` — `Scene::run` ends with
  `uefi::runtime::reset(ResetType::SHUTDOWN, ...)` today. The
  drain happens just before that call.
- `src/event.rs` — `RingBuffer<256>` with an `iter()` method.
  Already populated through every scene phase.
- `scripts/qemu.sh` — interactive QEMU wrapper. Writes serial
  output to `dist/serial.log` via `-serial file:...`. Same
  pattern is reused for `scripts/verify-release.sh`.
- `scripts/verify-release.sh` — the prior-art headless runner
  that boots each release artifact and greps the serial log
  for a banner string. The screenshot script is structurally
  similar (headless QEMU with a defined timeout).
- `docs/creator-notes/2026-04-concepts.md` — aesthetic notes,
  not player-facing.

All planning documents go in `docs/plans/`. This file is
committed alongside the code it describes, per the master plan's
one-commit-per-logical-change preference.

## Situation

Phase 5 landed (commits documented in the master plan's execution
table once this phase updates it). The binary now plays the full
opening sequence and parks on SYSTEM ONLINE. The ring buffer is
populated throughout but has no exit path — events are recorded
and then discarded on shutdown. `README.md`, `ARCHITECTURE.md`,
and `AGENTS.md` describe the scene/event/cursor/logo modules but
do not yet include a screenshot; the master plan's *Success
criteria* explicitly requires one.

`make release` and `make release-verify` have worked since Phase
2 and produce both `dist/uncalibrated-sextant.img` (raw) and
`dist/uncalibrated-sextant.qcow2`. No additional artifact work is
needed — that bullet of the master plan's Phase 6 sketch is
already satisfied.

The master plan's execution table and `docs/plans/index.md`
still show the whole plan as "Not started", a stale state from
before Phase 1 executed. Both want a pass.

## Mission and problem statement

Close out the first-playable milestone. Three deliverables:

1. A one-shot drain of the event ring buffer to the UEFI serial
   port immediately before ACPI shutdown, emitted as plain-text
   lines (one event per line). This proves the serial path works
   end-to-end and gives Phase 7's transport work something to
   read back. No framing, no gRPC, no structured encoding — just
   human-readable lines.
2. A committed screenshot of the running scene — parking-screen
   state preferred, so the boot transcript, logo, and SYSTEM
   ONLINE prompt are all visible in one frame — embedded in
   `README.md` and produced by a reproducible script so it can
   be regenerated when the scene changes.
3. The milestone's closing paperwork: master plan execution
   table marked Complete, `docs/plans/index.md` status updated,
   Success criteria checklist ticked off, `ARCHITECTURE.md` and
   `AGENTS.md` light-touch updates to mention the serial drain.

The phase is done when an operator can run `make qemu`, watch
the sequence, see the `README.md` screenshot match what they
just saw, and find serial output in `dist/serial.log`
enumerating the events they triggered; when `make release-verify`
still passes; when `pre-commit run --all-files` exits 0.

## Open questions

- **Serial port API.** uefi 0.37 exposes
  `uefi::proto::console::serial::Serial` behind
  `uefi::boot::open_protocol_exclusive`. An alternative is
  treating the `-serial file:...` flag's COM1 output as
  reachable via `log::info!` once `uefi::helpers::init()` has
  installed the stdout logger — but that routes through
  `SimpleTextOutput` to the GOP console, not to COM1. We need
  the real `Serial` protocol handle for the data to hit
  `dist/serial.log`. **Default: acquire `Serial` via
  `open_protocol_exclusive`, write raw bytes, drop.** Confirm
  at Step 1.
- **Timing of the drain.** Two choices: drain on every
  scene-transition event (streaming), or drain once at the end
  just before ACPI shutdown (batched). Streaming is closer to
  what Phase 7 will eventually want but requires owning the
  `Serial` handle across the whole scene. **Default: batched
  one-shot drain immediately before `uefi::runtime::reset`.**
  Simpler, avoids protocol-lifetime entanglement with
  `Scene::run`, and still exercises the serial path.
- **Event serialisation format.** Purely human-readable for now:
  `timestamp_ms=N event=<Keypress|LineRendered|SceneTransition>
  <key=value>...` one per line, CRLF terminated because that's
  what real serial terminals expect and the log file will be
  opened on hosts that may not normalise LF-only. No binary, no
  gRPC varints.
- **Screenshot timing.** When does the script take the shot —
  mid-boot (telemetry lines flowing) or parking-screen (full
  transcript + SYSTEM ONLINE)? **Default: parking screen.**
  It's the richest single frame and the cursor glitch is
  visible there too. The script boots, polls the serial log
  for the "SYSTEM ONLINE" banner, waits a beat for the
  parking-screen cursor to settle in its On phase, then
  screendumps.
- **Screenshot capture technique.** QEMU supports `-display
  none` + QMP `screendump` returning PPM, which `convert`
  turns into PNG. Alternatives: VNC + `vncdotool`; GTK +
  `import`. **Default: QMP screendump.** It is the single
  most reproducible option — no window manager, no vnc
  scraping, no display env — and matches the existing
  headless release-verify pattern.
- **Screenshot location in repo.** `docs/images/boot-
  sequence.png` is consistent with the master plan's "commit
  it into the repo" phrasing and keeps the repo root tidy.
  Referenced from `README.md` via relative path.
- **Regeneration cadence.** The screenshot is a commit artifact
  that will drift when the scene is retouched. **Default: a
  `make screenshot` target drives the capture script;
  operators regenerate when a visual change lands.** Not wired
  into any CI gate.
- **Screen recording.** Master plan's *Mission* mentions "a
  screenshot (and a short screen recording)" but the Phase 6
  sketch only requires the screenshot. **Default: out of
  scope for Phase 6.** Moved to Future work.
- **VHD / VMDK.** Master plan's Phase 6 sketch calls these
  stretch. **Default: deferred.** `qemu-img` supports both but
  we have no consumer for them yet; adding now is ceremony.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | low    | sonnet | none      | Confirm uefi 0.37 `Serial` protocol surface and QMP `screendump` output format. Brief report. See Step 1 below. |
| 2    | medium | sonnet | none      | Implement the ring-buffer serial drain in `src/scene.rs` (or a new `src/serial.rs` module if it grows past ~60 lines). Emit one line per event to COM1 just before `uefi::runtime::reset`. See Step 2 below. |
| 3    | medium | sonnet | none      | Add `scripts/screenshot.sh` and a `make screenshot` target. Run it; commit `docs/images/boot-sequence.png`. See Step 3 below. |
| 4    | low    | sonnet | none      | Embed the screenshot in `README.md`; add a paragraph in `ARCHITECTURE.md` for the serial drain; update `AGENTS.md` Current phase; mark the master plan Complete; update `docs/plans/index.md`. See Step 4 below. |

### Step 1 — confirm the small bits

**Check:**

- uefi 0.37 `Serial` protocol. Look at docs.rs for
  `uefi::proto::console::serial::Serial`. Specifically: is
  `write(&[u8])` the correct method signature, does it return
  a byte count or a unit, and does it need any mode
  configuration (baud, parity) before first write? The
  existing `-serial file:...` QEMU invocation gives us a
  pre-configured COM1 at default 9600 8N1; the firmware-side
  `Serial` handle should map to that directly.
- QMP `screendump` command syntax in QEMU and the output
  format. Since around QEMU 6, `screendump` gained an
  optional `format` argument accepting `ppm` (default) or
  `png`. Native-PNG output removes the ImageMagick dependency.
  **Confirm against the QEMU version available on the host
  (`qemu-system-x86_64 --version`).** If native PNG is
  available use it; otherwise fall back to PPM + `convert`.

**Do NOT:**

- Rewrite the event types to make them `Display` / `Debug` /
  `serde` friendly. The drain code can match-arm-format each
  variant inline; that's six lines of code for three
  variants and keeps the types free of derive churn.

**Output:** a tight paragraph each on the `Serial` write
signature and the QMP screendump syntax on this host. Under
200 words total.

### Step 2 — implement the serial drain

**Files touched:**

- `src/scene.rs` — the drain call site, immediately before
  `uefi::runtime::reset` at the end of `Scene::run`.
- `src/serial.rs` — **only create if Step 1 reveals more than
  ~60 lines of logic are needed** (e.g. if `Serial` needs mode
  configuration before writes, or if write-in-chunks handling
  is needed for large bursts). Otherwise keep the drain
  inline in `Scene::run` as a private method.

**Signature (rough):**

```rust
fn drain_to_serial(&self) {
    // 1. Acquire Serial protocol handle via
    //    uefi::boot::get_handle_for_protocol + open_protocol_exclusive.
    //    If unavailable, silently return — the scene has finished
    //    and an absent serial port should not prevent shutdown.
    // 2. For each event in self.ring.iter(), format a one-line
    //    ASCII summary and write it via serial.write(...).
    //    Terminate each line with "\r\n".
    // 3. Drop the handle; return.
}
```

**Line format (one per event):**

```
t=<ms> type=keypress unicode=<hex> scancode=<hex>
t=<ms> type=line row=<n>
t=<ms> type=transition from=<phase> to=<phase>
```

All numeric fields are decimal except the keypress unicode /
scancode fields which are 4-hex-digit values so non-printable
keys are unambiguous. No trailing whitespace. CRLF terminator.

**Ring-buffer `iter()` consideration.** The existing
`RingBuffer::iter()` method walks from `head` backwards
through `len` entries. Check the current ordering before
calling `iter()` in the drain — we want chronological order
(oldest first). If `iter()` is newest-first, add an
`iter_chronological()` method; do not reverse a collected
`Vec` (`no_std`; no `alloc`-by-default).

**Error handling.** If the Serial protocol is not present
(e.g. someone runs under a QEMU config without a serial port),
`open_protocol_exclusive` returns an error. Log-and-ignore is
correct behaviour — the scene has already delivered its
on-screen payload, and shutdown still needs to happen.

**Compile and lint but do NOT run:**

- `make build` succeeds.
- `pre-commit run --all-files` exits 0.

### Step 3 — screenshot capture

**Add `scripts/screenshot.sh`.** Modelled on
`scripts/verify-release.sh` but for the interactive artifact
(`dist/esp.img`). Shape:

1. `set -euo pipefail`; resolve paths against the repo root.
2. Ensure `dist/esp.img` is fresh: call
   `$REPO_ROOT/scripts/mkesp.sh`.
3. Copy a fresh `OVMF_VARS_4M.fd` to `dist/`.
4. Launch QEMU with:
   - `-display none`
   - `-enable-kvm`, `-machine q35`, `-cpu qemu64`, `-m 256M`
   - `-drive if=pflash,...readonly=on,file=OVMF_CODE_4M.fd`
   - `-drive if=pflash,format=raw,file=dist/OVMF_VARS.fd`
   - `-drive format=raw,file=dist/esp.img`
   - `-serial file:dist/serial.log` (so the drain output is
     also captured alongside the screenshot)
   - `-qmp unix:dist/qmp.sock,server,nowait`
5. Wait up to 30 seconds for the serial log to contain the
   SYSTEM ONLINE banner (reuse the polling pattern in
   `verify-release.sh`).
6. Sleep ~600 ms so the parking-screen cursor has time to hit
   an On phase.
7. Issue QMP `screendump` via a tiny helper — either
   `socat - UNIX-CONNECT:...` piping the `qmp_capabilities` /
   `screendump` command pair, or a short python one-liner if
   socat is not present on the host.
8. Convert to PNG if QEMU emitted PPM (Step 1 confirms
   whether `format=png` is available natively).
9. Move result to `docs/images/boot-sequence.png`.
10. Terminate QEMU via QMP `quit` (or via pidfile and
    `SIGTERM` if QMP is unreachable).

**Add a `screenshot` target to `Makefile`:**

```make
screenshot: build
	./scripts/screenshot.sh
	ls -lh docs/images/boot-sequence.png
```

Declared `.PHONY`. Order: after `release-verify` in the
existing phony list.

**Run the script once, inspect the output visually, commit
`docs/images/boot-sequence.png`.** The operator will confirm
the captured image is the right frame (full boot transcript,
logo visible top-right, SYSTEM ONLINE prompt visible, cursor
present). Re-run and re-capture if the shot arrived before the
cursor blinked on.

**Do NOT:**

- Wire `make screenshot` into `pre-commit` or any CI hook. The
  artifact is committed; regeneration is operator-driven.
- Commit intermediate `.ppm` or `.qcow2` outputs — only the
  final `.png`.
- Add an animated / SVG / recording variant.

### Step 4 — documentation and bookkeeping

**`README.md`:**

- Update **Status** to declare the first-playable milestone
  complete.
- Add an **Example** (or **What it looks like**) section
  directly after **Status** that embeds
  `docs/images/boot-sequence.png` with a one-sentence caption
  noting it is the parking-screen frame and was generated by
  `make screenshot`.
- No other sections need edits — Building / Contributing /
  Why / Sibling projects are all still accurate.

**`ARCHITECTURE.md`:**

- Add one short paragraph after the existing Phase 5 logo
  paragraph describing the Phase 6 serial drain: what it is
  (one-shot plain-text dump of the ring buffer just before
  ACPI shutdown), where it lives
  (`Scene::drain_to_serial`, or `src/serial.rs` if promoted),
  what the line format is, and that it is groundwork for the
  eventual gRPC-over-serial transport from `instar`.
- Move the "Ring buffer drain" bullet from the "remaining
  components" list into the implemented section (or delete
  the bullet — depending on how the paragraph reads).
- Leave narrator-leak + diagnostic-mode wording untouched; that
  is outside this phase.

**`AGENTS.md`:**

- Update **Current phase** to say the first-playable milestone
  has landed, note the serial drain, and point readers at the
  next-milestone open items in the master plan's *Future
  work*.
- Add `src/serial.rs` to **Where to read first** if (and only
  if) Step 2 promoted the drain to its own module.

**`docs/plans/PLAN-first-playable.md`:**

- Update the **Execution** table: all six rows to Complete,
  each with the commit SHAs produced by this milestone's
  commits. (Phase 5's commits are already in hand from the
  prior session — recover from `git log`.)
- Tick off the **Success criteria** checklist at the bottom of
  the file, making it a record of how each criterion was met
  (e.g. "screenshot committed at `docs/images/boot-sequence.png`",
  "`make release-verify` green as of commit X").
- Do not remove the *Future work* or *Open questions* sections
  — they remain relevant for subsequent milestones.

**`docs/plans/index.md`:**

- Change the master-plan row's Status from "Not started" to
  "Complete".

**Pre-commit and commit:**

- `pre-commit run --all-files` must exit 0.
- Commits: one for the serial drain (Step 2), one for the
  screenshot script + committed PNG (Step 3), one for the
  documentation and bookkeeping updates (Step 4). Master plan
  prefers one commit per logical change.

## Agent guidance

Follow the master plan's *Agent guidance* verbatim. Phase-
specific emphases:

- **Scope discipline.** This phase has three small jobs. If
  the sub-agent finds itself redesigning the ring buffer, the
  renderer, or the scene state machine to make the drain
  tidier, stop and ask. The simple version is correct.
- **The screenshot is load-bearing.** The master plan's
  success criteria specifically names it. Do not declare
  Step 3 done until the operator has confirmed the committed
  PNG shows the intended scene — a textual diff cannot speak
  to image quality.
- **No nightly-only features.** The toolchain is pinned to
  Rust 1.88 stable via `rust-toolchain.toml`. If the serial
  drain tempts a feature gate, back out and find the stable
  equivalent.

## Success criteria

Phase 6 is complete when:

- [ ] `dist/serial.log` after a `make qemu` run contains one
      plain-text line per recorded event, in chronological
      order, CRLF-terminated, with the line format described
      in Step 2.
- [ ] `docs/images/boot-sequence.png` is committed and shows
      the parking-screen frame (full boot transcript, logo
      top-right, SYSTEM ONLINE prompt, cursor visible).
- [ ] `README.md` embeds the screenshot and its Status section
      declares the first-playable milestone complete.
- [ ] `make screenshot` regenerates the PNG on demand and
      overwrites it idempotently.
- [ ] `make release-verify` still passes (no regression).
- [ ] `pre-commit run --all-files` exits 0.
- [ ] `docs/plans/PLAN-first-playable.md` execution table,
      success criteria checklist, and `docs/plans/index.md`
      all show the first-playable plan as Complete.

## Bugs fixed during this work

(None yet — populated during execution.)

## Future work

- Streaming serial transmission during the scene rather than a
  one-shot drain — pairs naturally with the gRPC-over-serial
  transport from `instar` when that lands.
- Structured (binary / protobuf / Cap'n Proto) event encoding
  for Ryll-side consumption; the plain-text dump here is for
  human eyeballs only.
- Short screen recording (GIF or WebM) to complement the
  committed screenshot. Optional; screenshot satisfies the
  milestone on its own.
- VHD / VMDK release artifacts via `qemu-img convert`. No
  consumer yet.
- CI-side screenshot regression — render a shot on every PR
  and diff against the committed reference. Premature: the
  scene is under active aesthetic iteration and image diffs
  would churn.
