# Locked bootloader — first SPICE-channel test as a scene

## Prompt

Before responding to questions or discussion points in this
document, read [DESIGN.md](../../DESIGN.md) (especially the
*SPICE channel mapping* and *Voice: unreliable narration leaks*
sections), [docs/spice-test-inventory.md](../spice-test-inventory.md)
(especially the Clipboard section and the transport note
immediately after it), and the prior master plan
[PLAN-first-playable.md](PLAN-first-playable.md) for tone,
phase structure, and execution conventions.

The locked-bootloader scene is the first content beat that
turns the existing first-playable scaffold into a real
SPICE-channel test — clipboard paste, in this case — with
diegetic framing as a bootloader-decryption flow. It is also
the first scene that *requires* a SPICE client (rather than
QEMU's bare GTK display) to play, and so introduces SPICE-
client testing infrastructure as a side effect.

Cross-repo references, in order of likely usefulness:

- `shakenfist/instar` — eventual home for the gRPC-over-serial
  transport that will replace the current plain-text drain.
  Out of scope for this milestone; worth understanding because
  Future work will lean on it.
- `spice-vdagent` (Linux daemon) — reference implementation
  of the protocol a UEFI port would eventually need to speak,
  so the server → client clipboard direction can be a real
  channel test rather than the on-screen-blob stub this
  milestone ships.
- `remote-viewer` (`virt-viewer` package on Debian / Fedora) —
  the SPICE client we will use to play this scene during
  development. Confirms paste-as-keystrokes fallback when no
  vdagent is detected on the guest.

All planning documents go in `docs/plans/`. Phase plans will
be separate files named `PLAN-locked-bootloader-phase-NN-...md`
and tracked in the *Execution* table below.

One commit per logical change as before; minimum one commit
per phase. Each commit should build, pass `pre-commit run
--all-files`, and have a clear message.

## Situation

The first-playable milestone (commits `862b638` → `a30bc38`)
landed a UEFI binary that boots, plays an opening scene, parks
on `SYSTEM ONLINE`, drains an event ring buffer to serial, and
ACPI-shuts-down. Nothing about that flow tests SPICE on the
wire. The binary writes to GOP and reads keystrokes via Simple
Text Input Ex; the harness runs against QEMU with `-display
gtk`, which is not a SPICE display at all.

The SPICE channel test inventory at
[docs/spice-test-inventory.md](../spice-test-inventory.md) now
catalogues the operations Ryll will eventually drive. Of ~85
rows, eight carry `binary:` markers indicating the first-
playable binary already exercises some shape of the operation
incidentally; none of the remaining ~77 rows have any
implementation.

This milestone takes the first row whose metaphor is strong
enough to earn screen time — *Text clipboard client → server*
("A message dropped in from outside — content the instrument
did not generate") — and builds the first scene that is a real
channel test, dressed as a locked-bootloader unlock.

## Mission and problem statement

Build a scene, inserted into the existing boot sequence
between `SENSORIUM: nominal` and `EMERGENCY SAFE BOOT
COMPLETE`, that exercises clipboard paste from operator → guest
as a real SPICE-channel test, and which an operator can play
through using a SPICE client (`remote-viewer` for now, `ryll`
eventually). The test passes when the operator pastes the
correctly-decoded base64 payload back into the SPICE session;
fails (with operator-visible error and ACPI-shutdown) when the
paste does not arrive within the paste timeout; restarts the
entire run when the operator selects Abort at the prompt.

Shape of the new scene, embedded mid-boot-sequence:

1. New telemetry lines establish the failure: an
   `Advanced b64 cryptographic coprocessor: OFFLINE` line and
   a `NIST 800-53 SC-28(1) Secret hardening: DISABLED BY
   CONFIGURATION` line. (The b64 reference is intentional —
   it is the default "encryption" of Kubernetes secrets and
   the joke is at the Kubernetes operator's expense.)
2. A DOS-style three-option prompt appears:
   `Decryption of next-stage bootloader failed. (R)etry,
   (I)gnore, or (A)bort?`.
3. **R(etry)** → animated `Retrying decryption...` with a dot
   leader, a short delay, then the prompt re-renders **in
   place** with an attempt counter (`(attempt N)`). After
   N ≥ 5 retries, a diegetic nudge appears below: `Continued
   retry will not change the outcome.` No scrolling — the
   prompt updates the same screen rows.
4. **A(bort)** → `uefi::runtime::reset(ResetType::COLD, ...)`.
   The entire run replays from the firmware boot manager.
5. **I(gnore)** → message: `Cryptographic co-processor offline.
   Encrypted bootloader payload follows. Decode externally and
   paste back to continue.` Then the encoded blob, formatted
   as a CTF flag (`sextant{...}` base64-encoded), with copy
   framing.
6. The binary then awaits keystrokes assembled into a buffer.
   On a known terminator (newline, or full-buffer match), it
   validates against the expected decoded value. Wrong paste
   → re-prompt with attempt counter (limited). Correct paste
   → `Booting...` followed by the existing `EMERGENCY SAFE
   BOOT COMPLETE` line and parking screen.
7. If no paste arrives within the silent-wait timeout, the
   binary shows a visible countdown
   (`Awaiting decoded payload. Aborting in NN...`), then an
   error halt screen for a CI-recordable beat, then ACPI-
   shuts-down. The countdown number must be unmistakeable in
   a session recording so a CI failure is diagnosable from
   the video alone.

Under the surface, paste capture works via the Phase 5 keyboard
polling — `remote-viewer` and `virt-viewer` detect the absence
of vdagent on the guest and fall back to replaying clipboard
paste as Inputs-channel keystrokes, character by character. The
on-screen blob in step 5 is a stub for the real SPICE clipboard
server → client write, which requires virtio-serial + vdagent
in UEFI and is explicitly *Future work*.

The milestone is done when an operator can launch a
`make spice` target, walk all four flow paths
(paste-correct, paste-wrong-then-correct, abort-and-replay,
timeout-and-halt), and have each behave as described.
Pre-commit clean; existing `make qemu`, `make release-verify`,
and `make screenshot` continue to work.

## Open questions

Defaults below are strong but worth confirming or iterating at
the relevant phase. Capture changes inline rather than letting
them drift.

- **SPICE client.** **Default: `remote-viewer`** (from the
  `virt-viewer` package; ubiquitous on Debian / Fedora). Ryll
  is already a SPICE client and already speaks the host side
  of vdagent clipboard
  ([ryll/src/channels/main_channel.rs:28-38](../../../ryll/ryll/src/channels/main_channel.rs)
  for the protocol constants; `arboard` for host clipboard
  access), but it expects the *guest* to speak vdagent — when
  the guest declares no `CAP_CLIPBOARD_*` capabilities, ryll
  has no paste-as-keystrokes fallback and clipboard paste is
  silently dropped. Our UEFI binary advertises no vdagent
  today, so ryll cannot drive this scene. `remote-viewer`'s
  paste-as-keystrokes fallback is what makes the scene
  playable now. Switch to ryll once *either* (a) ryll learns a
  paste-as-keystrokes fallback for guests without vdagent
  (small ryll-side feature; tracked in *Future work* below),
  or (b) the UEFI binary gains a real vdagent implementation
  (the larger Future work milestone) — at which point the
  fallback is unnecessary.
- **CTF flag prefix.** **Default: `sextant{...}`**. Short,
  project-specific, recognisable as a flag.
- **Placeholder decoded value.** Until later milestone content
  exists, we need a decoded value to validate against.
  **Default: `sextant{HELLO_OPERATOR}`**. Easy to type, easy
  to verify by eye, no plot dependency to update later.
- **Wrong-paste behaviour.** **Default: re-prompt with attempt
  counter, up to 3 attempts**, then halt with the same
  countdown / error / shutdown path as the timeout. Gives the
  operator a chance to fix typos without enabling indefinite
  brute-force.
- **Retry sleep duration.** **Default: 1.2 s** with an
  animated `Retrying decryption` dot leader (each dot a
  separate per-glyph blit per principle 6).
- **Retry attempts before diegetic nudge.** **Default: 5.**
- **Paste silent-wait timeout.** **Default: 60 s.** Generous
  enough for an operator to switch to a terminal, decode the
  blob with `base64 -d`, and switch back.
- **Visible countdown duration.** **Default: 30 s.** Counted
  down on screen one second at a time. Each tick is an
  in-place update of a single row.
- **Error halt duration before ACPI shutdown.** **Default:
  5 s.** Long enough that a CI session recording catches the
  error message clearly.
- **Abort reset type.** **Default: `ResetType::COLD`** — the
  full-VM-reboot effect the operator expects from "Abort".
- **Position in boot sequence.** **Default: insert between
  `SENSORIUM: nominal` and `EMERGENCY SAFE BOOT COMPLETE`** —
  the bootloader unlock is part of boot, not a post-boot
  decision. The existing `CLIPBOARD RELAY: handshake OK`
  diagnostic line stays where it is and reads as ironic
  foreshadowing once the locked-bootloader content lands.
- **`make qemu` (GTK) backwards compatibility.** **Default:
  keep working.** The new scene is reachable via `make qemu`
  but its paste step times out cleanly when no SPICE client
  is connected. Documented limitation; `make spice` is the
  canonical path for this scene.
- **Whether the existing screenshot flow needs to change.**
  **Default: no.** `make screenshot` continues to capture the
  parking-screen frame after a synthetic Ignore + correct
  paste sequence. Phase 1 confirms this works; if the QMP
  send-key path cannot drive the new scene, the screenshot
  stays the previous milestone's frame and we add a separate
  `make screenshot-bootloader` later.

## Execution

| Phase | Plan | Status |
|-------|------|--------|
| 1. SPICE-client testing infrastructure | [PLAN-locked-bootloader-phase-01-spice-infra.md](PLAN-locked-bootloader-phase-01-spice-infra.md) | Complete (commits a7b261d + docs commit; remote-viewer paste-as-keystrokes finding documented in phase plan's *Outcome* section) |
| 2. Locked-bootloader scene (state machine, content, paste capture, validation, timeout, abort) | PLAN-locked-bootloader-phase-02-scene.md | Blocked on ryll paste-as-keystrokes (see *Prerequisites* below) |
| 3. Iteration, documentation, inventory closeout | PLAN-locked-bootloader-phase-03-docs.md | Blocked on Phase 2 |

### Phase 1 sketch — SPICE-client testing infrastructure

Add `scripts/spice.sh` and a `make spice` target that:

- Launches QEMU with SPICE configured (`-spice
  port=5900,disable-ticketing=on,addr=127.0.0.1`, a SPICE-
  capable VGA device, and `-display none` so the SPICE client
  provides the display).
- Spawns `remote-viewer spice://127.0.0.1:5900` in a separate
  process so the operator gets a window naturally.
- Tears down both on Ctrl-C cleanly.

Confirm against the existing Phase 5 keyboard polling that
paste-as-keystrokes works: paste a multi-character string in
the SPICE client and observe the binary's `read_key` loop
receive each character in order. Document the path
peculiarities (does paste arrive with a trailing newline? is
there any character normalisation? are there delays between
characters?).

Document the operator UX briefly in `README.md`: how to
install `remote-viewer`, how to launch the scene, how to
paste, what to expect.

Keep `make qemu` (GTK) working unchanged. End of phase.

### Phase 2 sketch — locked-bootloader scene

Insert the scene into `BOOT_SCRIPT` and `Scene::run`. Add
new ring-buffer event variants for the new in-scene actions
(`BootloaderDecision { choice }`, `PasteReceived { len }`,
`BootloaderTimeout`) so the existing serial drain captures
machine-checkable test outcomes for free.

The full state machine:

- `Booting` plays the new b64 / NIST telemetry lines.
- `BootloaderPrompt` renders the R/I/A prompt and polls for a
  matching keypress. R loops to `BootloaderRetrying`; I
  advances to `BootloaderShowingBlob`; A cold-resets.
- `BootloaderRetrying` runs the 1.2 s animated retry, then
  re-renders the prompt in place with the attempt counter
  incremented. After 5 attempts, also renders the diegetic
  nudge below.
- `BootloaderShowingBlob` renders the message and the
  `sextant{...}`-base64 blob, then transitions.
- `BootloaderAwaitingPaste` polls keystrokes into a
  fixed-size buffer (`[u8; 64]` is plenty for the placeholder
  value), validates on terminator, and either advances to
  `Booting` (the existing post-bootloader path) or loops back
  to a re-prompt with attempt counter. After 3 wrong attempts
  or `silent-wait + countdown` of idle time, transitions to
  `BootloaderTimeout`.
- `BootloaderTimeout` runs the 30 s visible countdown with
  in-place row updates, then renders the error halt screen
  for 5 s, then ACPI-shuts-down.

Per-glyph rendering for all new content (principle 6). In-
place row updates for the retry counter, attempt counter, and
countdown — no scrolling.

End of phase: the scene plays end-to-end against `make spice`
through every path (correct paste, wrong paste retry, abort,
timeout). `make qemu` still launches the original-flow scene
to the b64 line, then times out cleanly at the paste prompt.

### Phase 3 sketch — iteration, docs, inventory closeout

Operator-driven iteration via `make spice`. Tune timing
constants, validate aesthetics (especially the countdown
visibility for CI recordings). Then:

- Update `README.md` Status with the new milestone landed and
  the `make spice` requirement called out for this scene.
- Update `ARCHITECTURE.md` with the bootloader scene's state
  machine and the SPICE-client testing path.
- Update `AGENTS.md` Current phase and add `scripts/spice.sh`
  to the where-to-read-first list.
- Update `docs/spice-test-inventory.md`: change the Status
  column for *Text clipboard client → server* to
  `binary: [locked-bootloader](plans/PLAN-locked-bootloader.md)`.
  The server → client row stays `—` (still stubbed).
- Mark the master plan Complete in this file's Execution
  table and in `docs/plans/index.md`.
- Tick the *Success criteria* checklist below with notes on
  how each criterion was met.

## Agent guidance

Follow the master `PLAN-first-playable.md` *Agent guidance*
verbatim. Phase-specific emphases:

- **Scope discipline.** This milestone is one scene plus one
  piece of testing infrastructure. Resist any temptation to
  ship vdagent / virtio-serial here — that work is bigger
  than this whole milestone and is captured under *Future
  work*.
- **CI-recording-friendliness is load-bearing.** The timeout
  countdown, error message, and abort behaviour will be the
  first parts of the project that need to fail visibly under
  automation. Bias toward generous on-screen pauses and
  clear language.
- **Principle 6 still binds.** Animated dots, ticking
  countdown numbers, and in-place re-rendered prompts all go
  through per-glyph or per-cell BltOps.
- **No cute encoding choices.** Plain base64; recognisable
  CTF flag format; nothing puzzly. The point of the round-
  trip is to test the channel, not to be hard.

## Administration and logistics

### Success criteria

This milestone is complete when:

- [ ] `make spice` opens a `remote-viewer` window that
      displays the binary's GOP output.
- [ ] The new scene renders the b64 / NIST telemetry lines,
      the R/I/A prompt, the encoded blob, and (depending on
      path) the success message, the wrong-paste re-prompt,
      the timeout countdown, or the cold-reset Abort.
- [ ] Pasting `sextant{HELLO_OPERATOR}` (the decoded form)
      into the SPICE client window after Ignore continues
      boot through `EMERGENCY SAFE BOOT COMPLETE` to the
      parking screen.
- [ ] Pasting incorrect content re-prompts up to three times,
      then enters the timeout / error / shutdown path.
- [ ] Selecting Abort cold-resets the VM (the firmware boot
      manager runs again and the scene replays from the
      start).
- [ ] Idling at the paste prompt for `60 + 30 + 5` seconds
      produces a clearly visible error halt and clean ACPI
      shutdown.
- [ ] `make qemu` (GTK path) still launches the original
      scene flow; the new scene is reachable but its paste
      step times out cleanly when no SPICE client is
      connected.
- [ ] `make release-verify` and `make screenshot` still pass.
- [ ] `pre-commit run --all-files` exits 0.
- [ ] Inventory's *Text clipboard client → server* row has a
      `binary:` plan link to this plan.
- [ ] `README.md`, `AGENTS.md`, `ARCHITECTURE.md` describe
      the new scene and the `make spice` path.

### Future work

Items deliberately deferred out of this milestone:

- **Ryll-side paste-as-keystrokes fallback.** Small ryll
  feature: when the guest never advertises `CAP_CLIPBOARD_*`
  (or vdagent never connects), replay clipboard paste as
  Inputs-channel keystrokes. Matches `remote-viewer`'s
  behaviour and lets uncalibrated-sextant switch off
  `remote-viewer` and onto ryll for this scene without
  waiting for a full UEFI vdagent implementation. Cheap and
  directly improves ryll, which is a primary purpose of this
  whole project.
- **virtio-serial driver in `no_std` UEFI.** Required for any
  real (non-keystroke) SPICE clipboard interaction. Likely
  its own milestone; the in-binary `Serial` protocol does
  *not* speak virtio-serial and our binary has no virtio
  drivers at all today.
- **vdagent protocol implementation.** Builds on the
  virtio-serial driver. Once landed, the on-screen blob in
  step 5 of the scene above is replaced with a real SPICE
  clipboard write, and the inventory's *Text clipboard
  server → client* row becomes a real channel test.
- **gRPC-over-serial transport (per `instar`).** The plain-
  text drain becomes structured, machine-readable, and
  bidirectional. Lets Ryll drive scenes rather than only
  observe.
- **Ryll-driven scene selection.** Ryll asks the binary to
  play *the locked-bootloader scene* (vs. some other scene),
  then validates the result. Currently the binary picks one
  scene unilaterally.
- **Multi-format clipboard** (image, RTF). Once vdagent
  exists, test more than text.
- **Real cryptographic content.** Replace base64 with an
  actual encrypted payload requiring a key, once a key
  exchange story exists.
- **Subsequent scenes that consume the breadcrumb.** The
  decoded payload can carry a hint for a later scene; that
  pays off only once later scenes exist.

### Bugs fixed during this work

(None yet — populated during execution.)

### Documentation index maintenance

On creation of this plan, `docs/plans/index.md` and
`docs/plans/order.yml` get a new master-plan entry. As phases
complete, update the status column in the Execution table
above and in `index.md`.

### Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the
work you intend to do aligns with that plan.
