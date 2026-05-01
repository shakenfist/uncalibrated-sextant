# Audit cleanup — phase 3: tests + closeout

Parent plan: [PLAN-audit-cleanup.md](PLAN-audit-cleanup.md).
Previous phases:
[Phase 1](PLAN-audit-cleanup-phase-01-bugs.md),
[Phase 2](PLAN-audit-cleanup-phase-02-structural.md).

## Outcome

**Status: Complete (commits d862ad3, 96720ab, plus this
closeout).**

Both test-coverage gaps the audit found are plugged.
`make screenshot-modes` exercises the dispatcher headlessly
and asserts the resulting `ModeSwitch` event in serial;
`make release-verify` now confirms both the startup banner
and the `available GOP modes:` line in raw + qcow2 release
artifacts.

### What Phase 3 actually delivered

- `scripts/screenshot-modes.sh` modelled on
  `scripts/screenshot.sh`. Drives the binary headless
  through the existing bootloader-Ignore-paste sequence to
  parking, presses `'3'` (1024×768 — the binary's default
  mode), waits 2 s for the toast TTL to clear, sends
  `'space'` to exit parking and drain events. Asserts
  exactly one `type=mode_switch requested=1024x768
  applied=1024x768` line in
  `dist/screenshot-modes-serial.log`. Drain count is 61
  (59 baseline + Keypress for `'3'` + ModeSwitch). Test
  passed on first run. Adds the `make screenshot-modes`
  Makefile target. (commit `d862ad3`)
- `scripts/verify-release.sh` adds a `grep -qF "available
  GOP modes:"` check after the existing banner-found
  branch. Both raw and qcow2 release artifacts PASS the
  combined check. (commit `96720ab`)

### What Phase 3 did NOT deliver, and why

Per master plan's *Future work* (carried forward from the
audit's deferred items):

- **Strict 59-event assertion in `screenshot.sh`.** Forces
  every milestone to update the constant; left as
  documented drift.
- **Headless cycle-mode test.** Impractical at 30 s per
  cycle without a build-flag-configurable
  `CYCLE_DWELL_MS`.
- **`tools/audit/wave1.sh` and `wave2-mechanical.sh`.**
  Worth doing once the operator's PR migration lands; not
  part of this plan.
- **Workspace split for host-side `cargo test`.** Defer
  until two or three pure functions accumulate behind the
  same workspace boundary.

## Prompt

Re-read the master plan's *Phase 3 sketch* and *Future
work*. Skim:

- [`scripts/screenshot.sh`](../../scripts/screenshot.sh) —
  the model for `scripts/screenshot-modes.sh`. Note the
  QMP send-key block driving AWAITING → Booting →
  Bootloader → Parked, and how it asserts drain events at
  the end.
- [`scripts/verify-release.sh`](../../scripts/verify-release.sh) —
  one-line addition to confirm `available GOP modes:`
  appears alongside the existing banner check.
- [`Makefile`](../../Makefile) — for the new `screenshot-modes`
  target.
- [`src/scene.rs`](../../src/scene.rs)'s `run_parked` and
  the `try_handle_mode_key` dispatcher so the test design
  matches actual behaviour: pressing a mode key from
  parked stays in parked (the dispatcher returns `true`
  and the `blink_until_key` helper continues the loop), so
  a second non-mode key is needed to exit parking and
  drain to serial.

This phase is mechanical. **Plan at medium effort
(sonnet)** for the script work; the QMP-timing design is
the only thinking step.

## Goal

Plug the two test-coverage gaps the audit found, then close
out the cleanup milestone:

1. `make screenshot-modes` exercises the
   display-mode-keystroke dispatcher headlessly. The
   audit found the entire `try_handle_mode_key` /
   `cycle_modes` / `Scene::repaint` / `draw_toast` path
   has zero non-interactive coverage today.
2. `verify-release.sh` confirms both the startup banner
   *and* the `available GOP modes:` line in release
   artefacts. After the display-mode-keystrokes milestone
   the GOP-mode dump is an early-boot output worth
   verifying.
3. Master plan and `docs/plans/index.md` mark the
   audit-cleanup milestone *Complete*.

## Scope

**In scope:**

- New script `scripts/screenshot-modes.sh` modelled on
  `screenshot.sh`. Drives the binary through the existing
  bootloader-Ignore-paste sequence to parking, then
  presses `'3'` to trigger a same-resolution mode-switch
  (1024×768 → 1024×768), waits for the toast to settle,
  then sends a non-mode key to exit parking and drain
  events. Asserts `type=mode_switch
  requested=1024x768 applied=1024x768` appears exactly
  once in `dist/screenshot-modes-serial.log`.
- New `make screenshot-modes` Makefile target.
- One-line addition to `scripts/verify-release.sh`
  confirming `available GOP modes:` appears in the
  serial log alongside the existing banner check.
- Phase 3 closeout: phase plan *Outcome* and *Exit
  criteria*; master plan's *Execution* table marked Phase
  3 Complete; `docs/plans/index.md` master-plans row
  updated to *Complete* with the full commit range.

**Out of scope (deferred per master plan's *Future work*):**

- Strict 59-event assertion in `screenshot.sh` (forces
  every milestone to update the constant; left as
  documented drift).
- Headless cycle-mode test (impractical at 30 s
  per cycle without a build-flag-configurable
  `CYCLE_DWELL_MS`).
- `tools/audit/wave1.sh` and `wave2-mechanical.sh` (worth
  doing once the operator's PR migration lands; not part
  of this plan).
- Workspace split for host-side `cargo test` of
  `nearest_mode` / `format_u32` / `RingBuffer` (master
  plan's *Future work*; defer until two or three pure
  functions accumulate).

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 3a   | medium | sonnet | none      | `scripts/screenshot-modes.sh` modelled on `screenshot.sh`: copy QMP key-driving structure through to parking, then press `'3'` to trigger a same-resolution mode switch, wait ~2 s, send `'space'` to exit parking, wait for QEMU exit, assert exactly one `type=mode_switch requested=1024x768 applied=1024x768` line in `dist/screenshot-modes-serial.log`. Add `make screenshot-modes` target. |
| 3b   | low    | sonnet | none      | One-line addition to `scripts/verify-release.sh`: after the existing `BANNER` grep loop succeeds, also `grep -qF "available GOP modes:" "$SERIAL_LOG"` and exit non-zero if missing. Update the success message to mention both checks. |
| 3c   | low    | sonnet | none      | Phase 3 closeout: phase plan *Outcome* + tick *Exit criteria*; master plan *Execution* table → Complete with commit range; `docs/plans/index.md` master-plans row → Complete. |

Three commits expected, one per step. The management
session can do 3c directly without a sub-agent.

## Detailed step briefs

### 3a — `scripts/screenshot-modes.sh` + `make screenshot-modes`

**Files:** `scripts/screenshot-modes.sh` (new),
`Makefile` (new target).

**Design rationale:** the existing `screenshot.sh` drives
the binary through the locked-bootloader scene to parking,
then takes a screenshot and exits. For mode-switch testing
we don't need a screenshot — we need to press a mode key
from a known steady-state and assert the resulting
`ModeSwitch` event arrives in serial. Parking is the
cleanest steady-state to test from because:

- The binary is in a stable polling loop (`run_parked`'s
  `blink_until_key` call).
- The dispatcher's path through `try_handle_mode_key` →
  `set_mode` → `repaint` → `draw_toast` is the same code
  any caller exercises; testing from parked is sufficient
  coverage of the dispatcher itself.

**Why `'3'` (1024×768)?** It's the binary's default mode,
so `set_mode` is a same-resolution apply-and-query-back.
`requested=1024x768` and `applied=1024x768` are both
predictable, and the assertion is exact rather than
"any mode_switch line."

**Script structure (modelled on `screenshot.sh`):**

```bash
#!/usr/bin/env bash
# Headless coverage for the display-mode-keystroke dispatcher.
#
# Boots the already-built ESP image, drives through the locked-
# bootloader scene (Ignore + paste) to parking, presses '3' to
# request a same-resolution mode switch, waits for the toast +
# repaint to settle, then sends 'space' to exit parking and
# drain events. Asserts the resulting serial log carries
# exactly one type=mode_switch line with the expected
# requested / applied dimensions.
#
# Usage: screenshot-modes.sh
# Env:
#   SCREENSHOT_MODES_TIMEOUT  Banner-wait timeout (default 30).
set -euo pipefail
```

Use the same OVMF + q35 + KVM args as `screenshot.sh`, but:

- Serial log: `dist/screenshot-modes-serial.log`.
- QMP socket: `dist/screenshot-modes-qmp.sock`.
- VARS copy: `dist/screenshot-modes-OVMF_VARS.fd`.
- Skip the `screendump` call entirely.

Python QMP block: copy the existing key-driving up through
the paste + Enter, then **insert mode-key beat**:

```python
# Beat 4: BOOT_PAUSE_MS + BOOT_SCRIPT_POST + parking settle.
time.sleep(2.0)

# Beat 5: parking screen is now stable. Press '3' to request
# a 1024x768 mode switch (which is already the active mode,
# so applied == requested and the assertion is exact).
send_key([qcode('3')])

# Beat 6: toast TTL is 1500 ms; mode-switch repaint is fast.
# Wait for the toast to settle and the dispatcher to push the
# ModeSwitch ring-buffer event before exiting.
time.sleep(2.0)

# Beat 7: send space to exit parking — non-mode key, falls
# through try_handle_mode_key, triggers the Parked->Parked
# SceneTransition push, releases the run_parked loop.
send_key([qcode('spc')])
```

**Assertion at end of script:**

```bash
# Confirm the ModeSwitch event made it to serial.
EXPECTED='type=mode_switch requested=1024x768 applied=1024x768'
COUNT=$(grep -cF "$EXPECTED" "$SERIAL_LOG" || true)
if [ "$COUNT" -ne 1 ]; then
    echo "FAIL: expected exactly one ModeSwitch line for 1024x768, got $COUNT" >&2
    echo "--- serial log mode_switch lines ---" >&2
    grep -F 'type=mode_switch' "$SERIAL_LOG" >&2 || true
    exit 1
fi

echo "drain events captured: $(grep -cE '^t=[0-9]+ type=' "$SERIAL_LOG")"
echo "PASS: ModeSwitch line confirmed for 1024x768"
```

**Makefile target:**

```make
screenshot-modes: build
	./scripts/mkesp.sh
	./scripts/screenshot-modes.sh
```

Add `screenshot-modes` to the `.PHONY` line at the top.

**Constraints:**

- shellcheck-clean (the existing scripts pass; mirror
  their style: `set -euo pipefail`, double-quoted
  variable expansions, the same `cleanup() { ... }; trap
  cleanup EXIT` pattern).
- `chmod +x` after creation.
- Don't modify `scripts/screenshot.sh` — the
  display-mode test stands alone.

### 3b — `verify-release.sh` GOP-mode grep

**File:** `scripts/verify-release.sh`.

After the existing `while` loop that waits for the banner
succeeds, add a check for the `available GOP modes:` line.
Both lines are emitted at boot before any keypress is
needed, so when the banner check finishes (~1 s), the
modes line is already present.

Approximate diff:

```bash
# After the existing if [ "$FOUND" -eq 1 ]; then block...

# Existing:
if [ "$FOUND" -eq 1 ]; then
    echo "PASS: banner found in serial log after ${ELAPSED}s."
    exit 0
fi

# After:
if [ "$FOUND" -eq 1 ]; then
    if grep -qF 'available GOP modes:' "$SERIAL_LOG"; then
        echo "PASS: banner and GOP-mode dump found in serial log after ${ELAPSED}s."
        exit 0
    fi
    echo "FAIL: banner found but no 'available GOP modes:' line in serial log."
    echo "--- serial log tail ---"
    tail -20 "$SERIAL_LOG"
    exit 1
fi
```

The kill + cleanup logic stays where it is. shellcheck must
remain clean.

### 3c — closeout

**Files:** this phase plan, the master plan,
`docs/plans/index.md`.

- Populate this phase plan's *Outcome* section with the
  3a + 3b commit SHAs and a one-paragraph headline.
- Tick the *Exit criteria* checklist with per-item
  references.
- Master plan's *Execution* table: mark Phase 3 Complete
  with the commit range.
- Master plan's *Success criteria* checklist: tick all
  items that completed across all three phases.
- `docs/plans/index.md`: change the Audit-cleanup row
  status from *Not started* to *Complete (commits 1482eb0
  through this closeout)*.

## Exit criteria

- [x] `scripts/screenshot-modes.sh` exists, is
      executable, drives the binary headless to parking,
      presses `'3'`, exits cleanly, and asserts exactly
      one `type=mode_switch requested=1024x768
      applied=1024x768` line in
      `dist/screenshot-modes-serial.log`. *(commit
      `d862ad3`.)*
- [x] `make screenshot-modes` Makefile target exists and
      runs the script. *(commit `d862ad3`.)*
- [x] `scripts/verify-release.sh` greps for both the
      startup banner *and* `available GOP modes:`.
      Failure on either produces a non-zero exit.
      *(commit `96720ab`.)*
- [x] `make screenshot` (existing) still passes
      unchanged. *(verified at every step of Phase 1 and
      Phase 2; unchanged in Phase 3.)*
- [x] `make screenshot-modes` (new) passes. *(verified at
      step 3a — drain count 61, ModeSwitch line
      confirmed.)*
- [x] `make release-verify` still passes (raw + qcow2).
      *(verified at step 3b — both PASS the combined
      banner + GOP-mode-dump check.)*
- [x] `pre-commit run --all-files` exits 0 at every
      commit (including shellcheck on the new script).
- [x] Phase plan *Outcome* + *Exit criteria* populated.
      *(this closeout.)*
- [x] Master plan *Execution* table marks Phase 3
      Complete; *Success criteria* checklist ticked.
      *(this closeout.)*
- [x] `docs/plans/index.md` audit-cleanup row marks
      *Complete*. *(this closeout.)*

## Risks

- **`'3'` from parking might fall through the mode
  table.** It is `MODE_KEYS[2]` so the dispatcher
  returns `true` and the loop continues. Verified by
  reading `try_handle_mode_key`. If the test fails with
  zero `type=mode_switch` lines, the failure mode is
  visible in the serial log and the script's `--- serial
  log mode_switch lines ---` dump.
- **Toast TTL might race with the second keypress.**
  `TOAST_MS` is 1500 and the script sleeps 2000 ms before
  sending space, so the toast has cleared (via
  `tick_toast` → `repaint`) by the time the exit key is
  sent. If a future tuning change bumps `TOAST_MS` past
  1500, the script's 2000 ms wait may need bumping too —
  capture as a comment in the script.
- **Toast ASCII assertion is *not* part of this test.**
  Asserting the on-screen toast text would require a
  screendump comparison, which is fragile. The
  ring-buffer event is the testable surface; if the
  event is correct, the toast almost certainly is too
  (they share the same `(applied_w, applied_h)` source).
- **The existing `screenshot.sh` is byte-identical
  unchanged.** Confirm at review time. The new script
  is additive; nothing in `screenshot.sh` should change.

## Back brief

Confirm the test point is the `ModeSwitch` ring-buffer
event (not a screendump comparison), confirm the exact
assertion string `type=mode_switch requested=1024x768
applied=1024x768` matches the formatter in
`src/serial.rs`, and confirm `verify-release.sh`'s new
check fails non-zero when the modes line is missing.
