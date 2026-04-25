# Locked bootloader — phase 1: SPICE-client testing infrastructure

Parent plan: [PLAN-locked-bootloader.md](PLAN-locked-bootloader.md).

## Prompt

Before working on this phase, re-read the master plan's *Phase 1
sketch* and the *Open questions* entries it points to (especially
*SPICE client*, *`make qemu` (GTK) backwards compatibility*, and
*Whether the existing screenshot flow needs to change*). Skim
[scripts/qemu.sh](../../scripts/qemu.sh) — `scripts/spice.sh`
should mirror its structure (host-native bash, OVMF VARS
freshened per run, serial log to `dist/serial.log`, repo-root
relative paths) so the two scripts are comparable side-by-side.

This phase is pure testing infrastructure: a new launch path that
attaches a real SPICE client to the existing first-playable
binary. It adds **no Rust source**, **no new scenes**, **no new
ring-buffer events**, and **no behaviour change** to the binary
itself. The locked-bootloader scene is Phase 2; this phase is the
chair you have to sit in before you can play that scene.

The failure mode is scope creep — adding a "while we're here"
keypress logger, a mock paste scene, or virtio-serial groundwork.
Resist all of it. The only `*.rs` file this phase touches is, at
most, a one-line cargo features tweak if the SPICE backend
unexpectedly demands one (it should not).

Cross-refs in-repo:

- [scripts/qemu.sh](../../scripts/qemu.sh) — the structural
  template. `scripts/spice.sh` should be diff-comparable.
- [scripts/mkesp.sh](../../scripts/mkesp.sh) — ESP image
  assembly; reused unchanged.
- [scripts/screenshot.sh](../../scripts/screenshot.sh) — uses
  `-display none` + a QMP socket. Same `-display none` discipline
  applies to `scripts/spice.sh` (the SPICE client *is* the
  display).
- [Makefile](../../Makefile) — `make qemu` target as the model;
  the new `make spice` target sits beside it.
- [src/scene.rs](../../src/scene.rs) — `run_awaiting` and
  `run_parked` both return on the *first* keypress. This is
  load-bearing context for what Phase 1 can and cannot verify
  about paste; see *Mission* below.

External references:

- `remote-viewer(1)` — `virt-viewer` package. Already installed
  on this host (`virt-viewer 11.0`, verified via
  `dpkg -l virt-viewer`). No install gate is strictly needed,
  but the script should print a clean error if the binary is
  missing on a future host.
- QEMU's SPICE invocation: the canonical line is
  `-spice port=5900,disable-ticketing=on,addr=127.0.0.1` paired
  with `-vga qxl` (or `-device qxl-vga`) and `-display none`.
  The `qxl` adapter publishes its framebuffer via the SPICE
  Display channel rather than rendering locally.
- SPICE Inputs channel: how `remote-viewer` delivers
  paste-as-keystrokes when the guest does not advertise
  `CAP_CLIPBOARD_*` via vdagent. Our UEFI binary advertises
  nothing of the sort, so the fallback path is what we are
  exercising.

All planning documents go in `docs/plans/`. This file is
committed alongside the script + Makefile + docs it describes.

## Situation

The first-playable milestone shipped a UEFI binary that boots
under `make qemu`'s GTK display path. GTK is convenient for
local visual debugging but is *not* a SPICE display — no SPICE
channels exist in that configuration, so by definition no SPICE
channel test can run against it. The binary's keystrokes arrive
via QEMU's GTK input layer directly into the firmware's Simple
Text Input Ex; no SPICE Inputs channel is involved.

The locked-bootloader milestone (this plan's parent) needs an
operator to paste base64 content from their workstation
clipboard into the running guest. That paste must traverse a
real SPICE channel for the test to mean anything. Today there is
no `make` target that launches the binary in a SPICE-capable
configuration, and no documented developer path for "open the
binary in a SPICE client and play it".

`remote-viewer` (the SPICE client packaged with `virt-viewer`)
is already installed on the development host. It detects the
absence of a guest-side vdagent and falls back to delivering
clipboard paste as Inputs-channel keystrokes — this is the
fallback path the master plan relies on. Ryll, the eventual
test driver, is also a SPICE client but lacks this fallback
today (master plan *Open questions / SPICE client* discusses
this at length); switching off `remote-viewer` is *Future work*.

`scripts/qemu.sh` is the prior-art launch wrapper. It runs
QEMU directly on the host (no Docker), copies a fresh
`OVMF_VARS_4M.fd` per run for a clean EFI variable slate, and
writes serial output to `dist/serial.log`. `scripts/screenshot.sh`
already demonstrates the `-display none` + headless invocation
pattern that `scripts/spice.sh` will adapt.

The existing observable surfaces — `dist/serial.log`, the QMP
screenshot path, `make release-verify` — must continue to
function unchanged after this phase.

## Mission and problem statement

Add a SPICE-client launch path for the existing binary, and
verify single-char keystroke delivery from clipboard paste lands
in the binary's keyboard polling loop. Two deliverables, plus
the documentation surface.

### 1. `scripts/spice.sh` + `make spice` target

Structurally a sibling of `scripts/qemu.sh`. The script:

- Builds the ESP image via `make build && scripts/mkesp.sh`
  (same as `make qemu`), so `make spice` is a one-stop target.
- Confirms `remote-viewer` is on `PATH`; aborts with a clear
  install hint (`apt install virt-viewer`) if not.
- Resolves a free TCP port if `5900` is in use (or, simpler:
  uses `5900` and lets the operator override via env). **Default
  for Phase 1: hard-coded `5900`** — keep the script short;
  port-probing logic is *Future work* if it becomes a friction
  point. Document the conflict mode in the script's header.
- Copies a fresh `OVMF_VARS_4M.fd` per run (same pattern as
  `qemu.sh`).
- Launches `qemu-system-x86_64` with:
  - `-enable-kvm -machine q35 -cpu qemu64 -m 256M`
    (matches `qemu.sh`)
  - OVMF pflash drives (matches `qemu.sh`)
  - The ESP image as a raw drive (matches `qemu.sh`)
  - `-vga qxl` (the SPICE-aware framebuffer adapter)
  - `-spice port=5900,disable-ticketing=on,addr=127.0.0.1`
  - `-display none`
  - `-serial file:$REPO_ROOT/dist/serial.log` (matches
    `qemu.sh`; lets the existing serial-watching workflow
    continue working)
- Waits briefly for QEMU to bind the SPICE port (a short retry
  loop probing `127.0.0.1:5900` with `bash`'s `</dev/tcp/...`
  redirection), then spawns
  `remote-viewer spice://127.0.0.1:5900` as a backgrounded
  child process.
- Installs a `trap` on EXIT/INT/TERM that kills both QEMU and
  `remote-viewer` so Ctrl-C in the parent shell tears the whole
  setup down cleanly.
- Foregrounds (`wait`) on QEMU; closing the `remote-viewer`
  window does *not* kill QEMU on its own (no GTK window to
  close), so the operator's exit gesture is Ctrl-C in the
  terminal. Document this clearly in the script header and
  README.

`make spice` invokes the script with the standard ESP path. It
does not replace `make qemu` — both targets coexist. `make qemu`
remains the recommended local-debugging target; `make spice` is
the canonical path for any scene that requires a real SPICE
client.

### 2. Single-keystroke smoke test (manual, operator-driven)

Verify that the SPICE Display + Inputs channels are doing what
they are advertised to do, against the *unchanged* first-
playable binary:

- Launch `make spice`. Observe the framebuffer in
  `remote-viewer`: the wordless cursor-only AWAITING screen
  should render, then the boot transcript after the first key,
  then the parking screen.
- At the parking screen, **paste a single character** (not a
  string) from the host clipboard via the SPICE client's paste
  shortcut. The character is delivered via the SPICE Inputs
  channel as a synthetic keypress, which the binary's
  `read_key` loop catches; it advances past Parked to ACPI
  shutdown.
- Inspect `dist/serial.log` after shutdown: the serial drain
  should contain a keypress event whose `unicode=` field
  matches the pasted character.

That confirms (a) framebuffer reaches the SPICE client, and
(b) keystroke from SPICE Inputs reaches the binary. Together
these are sufficient to know Phase 2 has a working substrate.

**Why a *single* character, not a multi-character paste?** The
current binary's `run_awaiting` and `run_parked` both return on
the first keypress. A multi-char paste produces one observable
keypress event in the serial drain — the rest are lost to ACPI
shutdown before the binary polls again. Characterising
multi-char paste behaviour (does it arrive with a trailing
newline? are there inter-char delays? any normalisation?) needs
a scene that *holds* through multiple keystrokes, which Phase 2
provides natively (the bootloader paste capture loop). Doing
the multi-char characterisation in Phase 2 is honest — doing it
in Phase 1 would require adding a throwaway capture scene that
gets deleted in Phase 2, which is the kind of scope creep the
prompt warns against.

### 3. Documentation surface

- `README.md`: add a short subsection under *Building and
  running* describing the SPICE path — install hint, `make
  spice` invocation, the "close with Ctrl-C in the terminal"
  exit gesture, and that the SPICE path is required for
  scenes that exercise SPICE channels (forward-references the
  yet-to-land Phase 2 scene without committing to its detail).
  Update the Status section if any existing claim becomes
  stale.
- `AGENTS.md`: add `make spice` to the *Build commands* list
  alongside `make qemu`. Add `scripts/spice.sh` to the
  *Where to read first* list.
- `ARCHITECTURE.md`: **no change** in this phase. The
  architecture of the binary does not change. Phase 2's plan
  will add the bootloader scene's state-machine paragraph.

The screenshot at `docs/images/boot-sequence.png` does **not**
need regeneration — the rendered scene is identical, only the
display channel changed. Confirm by eye after the smoke test
that the rendered frame in `remote-viewer` matches the
committed screenshot; capture is unchanged.

## Open questions

Defaults below are strong but worth confirming during execution.
Capture changes inline rather than letting them drift.

- **Hard-coded SPICE port vs port-probing.** **Default: hard-
  code `5900`.** Reasons: the script is shorter, port `5900` is
  the SPICE convention and rarely conflicts on a developer
  workstation, and an env-variable override
  (`SPICE_PORT=5901 make spice`) is a one-line escape hatch.
  Add port-probing only if the host turns out to have something
  squatting on `5900`. Future work.
- **VGA model: `qxl` vs `virtio-gpu`.** **Default: `qxl`.**
  `qxl` is the canonical SPICE display adapter, has the
  longest track record with `remote-viewer`, and matches what
  Linux distros' SPICE-enabled VMs ship by default. `virtio-
  gpu` works under SPICE too but with weaker historical
  guarantees for paste-as-keystrokes fallbacks; pick the
  conservative path for a testing harness.
- **`-machine q35` vs alternative.** **Default: `q35`** to
  match `qemu.sh` exactly. Same firmware (OVMF), same chipset,
  same KVM acceleration, same memory size. Behaviour parity
  between `make qemu` and `make spice` is more valuable than
  any micro-optimisation.
- **Auto-spawn `remote-viewer` vs print-and-wait.** **Default:
  auto-spawn.** Matches `make qemu`'s "open a window for me"
  UX. The cost is one extra trap target; the benefit is that
  `make spice` is a single command for the operator, not a
  two-step ritual. Document the manual `remote-viewer
  spice://127.0.0.1:5900` invocation as a fallback for
  operators who want to use a non-default client.
- **Exit gesture clarity.** **Default: Ctrl-C in the terminal
  is the only documented exit path.** Closing `remote-viewer`'s
  window does not stop QEMU (there is no GTK QEMU window). The
  script header and README should both spell this out so an
  operator does not leave a stray QEMU running.
- **`-monitor` / QMP socket on `make spice`.** **Default: not
  added.** The interactive operator session needs neither;
  `scripts/screenshot.sh` already opens its own QMP socket
  when it needs one. Adding QMP to `make spice` is *Future
  work* if Phase 2's iteration loop wants programmatic
  control.
- **Audio / USB / smartcard / folder-share channels.**
  **Default: not enabled.** Phase 1 only needs Display +
  Inputs. Adding more channels here would couple Phase 1 to
  scenes that don't exist yet. Each channel gets enabled as
  the corresponding scene lands.
- **`make screenshot` interaction with `make spice`.**
  **Default: no change.** `make screenshot` continues to use
  its own headless QEMU + QMP path, independent of SPICE.
  Confirmed by reading `scripts/screenshot.sh`; the SPICE
  invocation is additive, not a replacement.
- **What happens to `dist/serial.log` between runs?** **Default:
  it gets overwritten** (same as `make qemu`, which uses
  `-serial file:` not `append`). Operators who want to preserve
  a log copy it themselves between runs.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | Write `scripts/spice.sh` mirroring `scripts/qemu.sh`'s structure, add `make spice` target, ensure `make qemu` and `make release-verify` still pass. See "Brief for step 1a" below for the full spec. |
| 1b   | medium | sonnet | none      | Operator-driven smoke test of the SPICE path against the unchanged binary, then commit the doc updates (README + AGENTS) capturing what was confirmed. See "Brief for step 1b" below. |

### Brief for step 1a

Write a host-native bash script `scripts/spice.sh` that launches
the existing UEFI binary under QEMU configured for SPICE, and
auto-spawns `remote-viewer` to attach. Mirror `scripts/qemu.sh`
in structure: `set -euo pipefail`, `REPO_ROOT` resolution, OVMF
VARS freshened per run, serial log to `dist/serial.log`,
repo-root-relative paths.

Required QEMU flags (additions or substitutions vs `qemu.sh`):

- Replace `-display gtk` with `-display none`.
- Add `-vga qxl`.
- Add `-spice port=5900,disable-ticketing=on,addr=127.0.0.1`.
- Keep all other flags identical to `qemu.sh` so behaviour
  parity is obvious by diff.

After spawning QEMU in the background, poll `127.0.0.1:5900`
until the port accepts a TCP connection (use bash's
`</dev/tcp/127.0.0.1/5900` test, with a short retry budget,
e.g. 20×0.25s). Then spawn `remote-viewer
spice://127.0.0.1:5900` in the background.

Install a `trap '...' EXIT INT TERM` that kills both child PIDs
(`qemu_pid` and `viewer_pid`) so Ctrl-C tears everything down.
Foreground via `wait $qemu_pid`. Document in the script header
that closing the `remote-viewer` window does not exit QEMU; the
operator must Ctrl-C in the terminal.

Add an early `command -v remote-viewer >/dev/null` check that
exits with a clear `apt install virt-viewer` hint if the binary
is missing.

Add a `make spice` target in the `Makefile` alongside `make
qemu` (same dependency on `make build` then `scripts/mkesp.sh`,
but invoking `scripts/spice.sh` instead of `scripts/qemu.sh`).
Add `spice` to `.PHONY`.

Verification before declaring step done:
- `pre-commit run --all-files` passes (notably shellcheck on
  the new script).
- `make release-verify` still passes (proves the binary build
  path is unchanged).
- `make qemu` still launches the existing GTK window unchanged
  (a quick spot-check is enough; do not commit any change to
  `qemu.sh`).

Do *not* run an interactive `make spice` smoke test from the
sub-agent — that requires operator interaction with the
clipboard. The smoke test belongs to step 1b in the management
session.

Commit message convention follows the existing pattern
(50-char subject ending in `.`, body wrapped at 75 chars,
`Co-Authored-By` and `Signed-off-by` lines per CLAUDE.md).

### Brief for step 1b

Operator-driven verification, then docs. Sequence:

1. From the management session, run `make spice`. Observe the
   `remote-viewer` window: AWAITING cursor-only screen, then
   advance via host keystroke (or paste a single character).
2. Walk through to the parking screen. From the host
   clipboard, paste a single distinctive character (e.g.
   `Z`). Confirm ACPI shutdown.
3. Inspect `dist/serial.log` for a keypress event with
   `unicode=` matching the pasted character.
4. Spot-check that the rendered output in `remote-viewer`
   matches the committed `docs/images/boot-sequence.png` — no
   pixel-perfect comparison needed, just "looks the same".
5. Tear down (script's trap should handle this on Ctrl-C).
6. Spawn a sub-agent to update `README.md` (add a *SPICE
   client path* subsection under *Building and running*) and
   `AGENTS.md` (add `make spice` to *Build commands*, add
   `scripts/spice.sh` to *Where to read first*) per the
   *Documentation surface* section above. The sub-agent does
   *not* need to run the smoke test — pass it the verified
   results.
7. Verify `pre-commit run --all-files` still passes.
8. Commit docs as a separate commit from the script.

The smoke test result becomes a single bullet in the commit
body for the docs commit ("Manually verified: paste of single
character at parking screen reaches the binary's serial drain
as a keypress event with matching `unicode=` field.").

## Agent guidance

Follow the master `PLAN-locked-bootloader.md` *Agent guidance*
section (which inherits from `PLAN-first-playable.md`).
Phase-specific emphases:

- **Behaviour parity with `make qemu`.** The new script and
  the existing one should differ only where SPICE *requires*
  difference (display, VGA, SPICE port). Anything else
  diverging is a Phase 1 bug.
- **No Rust changes.** This phase touches `scripts/`,
  `Makefile`, and docs only. If a Rust change feels necessary,
  stop and re-scope — it almost certainly belongs in Phase 2.
- **Document the exit gesture.** "Close the window" is the
  default operator instinct from `make qemu`. SPICE breaks
  that intuition because there is no QEMU-owned window.
  Mention this in the script header *and* the README.

## Administration and logistics

### Success criteria

This phase is complete when:

- [ ] `scripts/spice.sh` exists, is executable, passes
      shellcheck, and launches QEMU + `remote-viewer` together.
- [ ] `make spice` is a working `Makefile` target listed in
      `.PHONY`.
- [ ] Closing the operator session via Ctrl-C in the terminal
      kills both QEMU and `remote-viewer` cleanly (no orphan
      processes).
- [ ] The first-playable binary, launched via `make spice`,
      renders its scene in the `remote-viewer` window
      indistinguishably from the existing `make qemu` GTK
      output.
- [ ] A single character pasted from the host clipboard
      reaches the binary's `read_key` loop and appears in
      `dist/serial.log` as a keypress event with the matching
      `unicode=` field.
- [ ] `make qemu`, `make release-verify`, and `make screenshot`
      all continue to work unchanged.
- [ ] `README.md` documents the SPICE path under *Building
      and running*.
- [ ] `AGENTS.md` lists `make spice` and `scripts/spice.sh`.
- [ ] `pre-commit run --all-files` exits 0.

### Future work

Items deliberately deferred from this phase:

- **Multi-character paste characterisation.** Trailing newline
  semantics, inter-character delays, any normalisation
  performed by `remote-viewer` or QEMU's SPICE Inputs channel.
  Belongs in Phase 2 where the locked-bootloader scene
  natively holds for multi-char input and can observe a
  buffered paste end-to-end.
- **Port-probing for SPICE.** If a developer's `5900` is
  occupied, the `SPICE_PORT=5901 make spice` env-var override
  is the documented escape hatch for now. Auto-probing for a
  free port is *Future work* if friction emerges.
- **QMP socket on `make spice`.** Add only if Phase 2's
  iteration loop wants programmatic control of the SPICE-
  attached QEMU. Phase 2 will judge.
- **Switching from `remote-viewer` to ryll.** Master plan
  *Open questions / SPICE client* covers this in detail.
  Requires either a Ryll-side paste-as-keystrokes fallback or
  a UEFI vdagent implementation; both are larger than this
  whole milestone.
- **Channels beyond Display + Inputs.** Audio / USB /
  smartcard / folder share each get enabled as the
  corresponding scene lands.
- **CI gating on `make spice`.** No automation today; SPICE
  paste is a manual operator path. Headless SPICE-channel
  driving is *Future work* and probably depends on Ryll
  picking up the ability to drive the UEFI binary as a real
  SPICE client.

### Bugs fixed during this work

(None yet — populated during execution.)

### Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the work
you intend to do aligns with that plan.
