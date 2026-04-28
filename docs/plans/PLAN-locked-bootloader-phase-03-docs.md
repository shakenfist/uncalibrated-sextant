# Locked bootloader — phase 3: polish, docs, inventory closeout

Parent plan: [PLAN-locked-bootloader.md](PLAN-locked-bootloader.md).
Siblings: [phase 1](PLAN-locked-bootloader-phase-01-spice-infra.md)
(SPICE infra, complete);
[phase 2](PLAN-locked-bootloader-phase-02-scene.md) (scene state
machine, complete).

## Prompt

Before working on this phase, re-read:

- The master plan's *Phase 3 sketch*. The original scope was
  "iteration, documentation, inventory closeout". Phase 2's
  step 2e already covered the README / AGENTS / ARCHITECTURE
  updates the master plan flagged for this phase, so the
  remaining doc work is narrower than the master-plan sketch
  implies.
- [PLAN-locked-bootloader-phase-02-scene.md](PLAN-locked-bootloader-phase-02-scene.md)
  — particularly *Bugs fixed during this work* (the
  `Ctrl+Shift+V` and Mac-Option findings, mitigated in
  Phase 2) and *Future work* (the wrong-paste-suffix overlap
  and the partial-paste indecision-hang, both deferred to
  this phase).
- [src/bootloader.rs](../../src/bootloader.rs) — the file you
  will edit. Two narrow changes: the wrong-paste re-prompt
  layout and the silent-wait-timer condition.
- [docs/spice-test-inventory.md](../spice-test-inventory.md)
  — particularly the *Text clipboard client → server* row,
  whose Status column needs the inventory closeout.
- [docs/plans/index.md](index.md) and the master plan's
  *Execution* table — both need their final status flips
  when Phase 3 closes.

This phase is small. Two narrow code changes carry the
operator-experience polish that fell out of Phase 2's smoke
test, plus a mechanical doc commit that closes the
milestone. There is no new scene content, no new event
variants, no new build paths. If a change feels bigger than
that, stop and re-scope — it probably belongs in a separate
plan.

## Situation

Phase 2 landed (commits 4a087dc, 7537897, ec223c5, 93ce17e,
9b3335a, afda0b0, a1cfb1a). The locked-bootloader scene
plays mid-`run_booting`, all four flow paths verified by
operator smoke test under `make spice-ryll`. The
README / AGENTS / ARCHITECTURE coverage and the master plan
execution table flip to "Phase 2 complete" both landed in
step 2e.

Two findings from the smoke test are deferred-to-here:

1. **Wrong-paste suffix overlap.** When the operator pastes
   incorrectly and the bootloader re-renders the input row
   with `(wrong, attempt N of 3)` appended after `Awaiting
   decoded payload> `, the operator's next typed echo
   overwrites the suffix character-by-character starting at
   the column right after the prompt. Functionally fine; the
   echo eventually wins. Visually it reads as scrambled mid-
   paste because `Awaiting decoded payload> s wrong, attempt
   1 of 3)` after the operator types `s`. Cosmetic, but the
   diegetic effect is "system is glitching" rather than
   "previous attempt was wrong, try again", and the latter
   is what the design wants.

2. **Partial-paste indecision-hang.** The
   `PASTE_SILENT_WAIT_MS` (60 s) idle timer fires only when
   `len == 0` — i.e. when the buffer is empty. The Phase 2
   reasoning was: once any character has arrived, the
   wrong-paste counter is the gate, and partial half-typed
   pastes fail by terminator-or-buffer-full. That reasoning
   ignores the operator who types a few characters then
   walks away (or whose ryll session disconnects mid-paste);
   in that case the binary sits forever with no terminator
   in sight, no buffer-fill in sight, and no idle timeout.
   Real bug, not just a Phase 3 polish.

Plus the standing inventory + master-plan-status closeout:

3. `docs/spice-test-inventory.md` row for *Text clipboard
   client → server* still reads `—`; needs to flip to
   `binary: [locked-bootloader](plans/PLAN-locked-bootloader.md)`.
4. The master plan's overall *Status* column in
   `docs/plans/index.md` and its own Execution table caption
   (if any) need flipping to *Complete (commits ...)* once
   Phase 3 lands.

The R/I/A prompt indecision timeout question (raised during
smoke test: should the operator who walks away from the
prompt also see a long-timeout shutdown for unattended-CI
hygiene?) is **deferred to master-plan Future work**, not
addressed here. Phase 2's behaviour at that prompt — wait
forever for human input — is the documented design and
matches the original master plan's "operator may step away
to find the host clipboard tool" framing. A long-timeout
fallback for CI is its own milestone.

## Mission and problem statement

Three deliverables:

### 1. Wrong-paste suffix on a separate row (`src/bootloader.rs`)

Move the `(wrong, attempt N of 3)` indicator off the input
row and onto the row immediately below it. Keep it sticky
across subsequent paste attempts within the same blob screen
(it does not need re-rendering on each paste, only on each
wrong outcome). Mirror the pattern the retry path already
uses for the diegetic nudge (the nudge is on `nudge_row =
prompt_row + 1` and is sticky once rendered).

Concretely:

- Add a `wrong_indicator_row` field to `BootloaderScene`,
  set to `input_row + 1` in `run_blob_and_paste`.
- In the wrong-paste branch:
  - `clear_row(input_row)` — wipes the previous echo.
  - Re-render the bare input prompt at `input_row` (no
    suffix on this row).
  - Render `(wrong, attempt N of 3)` on
    `wrong_indicator_row` via per-glyph blits, replacing the
    inline suffix code that's there today.
  - Note the wrong-indicator row in `highest_row` so the
    success-path region-clear wipes it.
- The existing `WRONG_PASTE_LIMIT = 3` and the
  bypass-silent-wait-on-cap-reached behaviour are unchanged;
  this is purely a layout fix.

After the fix, the visual reads as:

    Awaiting decoded payload> sextant{HELL <-- typed echo
    (wrong, attempt 1 of 3)                <-- sticky indicator
                                             below

### 2. Silent-wait timer always applies (`src/bootloader.rs`)

In `capture_paste`, change the idle-timeout condition to
fire whenever `idle_ms >= PASTE_SILENT_WAIT_MS`, regardless
of `len`. The 60 s window is a no-key-activity timer, not a
no-progress timer.

Reset `idle_ms` to zero on each successful `poll_key()`
(already happens). The semantics become: "60 seconds without
any key arriving triggers timeout, whether the buffer is
empty or partly-filled."

Justification: ryll's paste-as-keystrokes types at 16 ms per
character, so a 23-character placeholder paste finishes in
~370 ms total — far inside the 60 s window even if the
operator is mid-keystroke. A human typing the placeholder by
hand would also be far inside the window per keystroke. The
new condition catches the disconnected-mid-paste and
operator-walked-away-mid-typing cases without compromising
the active-paste cases.

The Phase 2 commentary in the source ("Once any printable
has been captured, the wrong-paste counter is the gate;
partial half-typed pastes do not silently time out") needs
revising or removing to match the new behaviour.

### 3. Inventory + master-plan-status closeout

`docs/spice-test-inventory.md` — change the *Text clipboard
client → server* row's Status column from `—` to
`binary: [locked-bootloader](plans/PLAN-locked-bootloader.md)`.
Match the formatting of any existing rows that already cite
plan links (the *first-playable* binary cites them in some
of the eight `binary:` rows; pick whichever format is
prevalent).

`docs/plans/PLAN-locked-bootloader.md` — flip the master
plan's Execution table row 3 from "Not started" to
*Complete (commits ...)*. Update the *Success criteria*
checklist (every item checked, with notes referencing the
commit that landed it).

`docs/plans/index.md` — flip the master plan's Status
column from *Phase 1-2 complete, Phase 3 in progress* to
*Complete*.

`docs/images/boot-sequence.png` — re-run `make screenshot`
and commit any pixel-level diff that falls out of the layout
fix in §1 (the wrong-indicator row only renders on a wrong
paste; the screenshot drives the success path, so the
rendered frame should be unchanged. If `make screenshot`
produces a byte-identical PNG, no commit needed for the
image. If it changes, commit the new file.)

## Open questions

Defaults below are strong but worth confirming during
execution.

- **Wrong-indicator stickiness vs cleared-on-next-paste**.
  **Default: sticky.** Once the operator has seen the
  indicator, it stays visible until the next outcome — same
  pattern as the retry-path nudge. Re-rendering or clearing
  it on each new paste attempt is more code for no operator
  benefit. The indicator's text updates in place when the
  count increments (1 → 2 → 3); when 3 wrong pastes hit, the
  flow transitions to the timeout countdown, which clears
  the whole region anyway.
- **Should the wrong-indicator suffix include the
  placeholder hint** (e.g. `(wrong, attempt 1 of 3 — paste
  the decoded payload)`)? **Default: no.** The diegetic
  framing is "system tells you the previous attempt was
  wrong"; a hint would lean too far toward
  "system tells you what to do next", which defeats the
  paste-and-decode metaphor. The intro line above the blob
  already covers what the operator is meant to do.
- **Timer semantics for `idle_ms` after a wrong paste**.
  **Default: reset to zero.** A wrong paste is key activity
  even if the validation fails; the operator was clearly at
  the keyboard. Each new `capture_paste` call instantiates
  its own `idle_ms = 0` (existing behaviour), so this is
  free.
- **Whether to also tighten the silent-wait window**.
  **Default: keep 60 s.** The original 60 s is generous so
  the operator can switch to a terminal, decode, and switch
  back. Once typing has started, the same 60 s is plenty of
  margin. Tightening it would surprise operators who get
  interrupted mid-paste.
- **`make screenshot` regeneration**. **Default:
  conditional.** Run `make screenshot` after the layout fix;
  if the captured PNG is byte-identical, skip the image
  commit. The rendered parking screen is unaffected by the
  wrong-paste row layout in any successful traversal.
- **Should this phase tune any of the master-plan timing
  defaults** (retry sleep, countdown duration, error halt
  duration)? **Default: no.** The smoke test passed without
  iteration; "all looks good to me" is the operator signoff.
  Tuning is welcome later in a small follow-up if a feature
  reveals a need.

## Execution

Three steps, each its own commit.

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 3a   | low    | sonnet | none      | Two narrow `src/bootloader.rs` changes: wrong-paste suffix moves to its own row (`input_row + 1`), and the `capture_paste` silent-wait timer applies regardless of buffer length. Verify via `make build` and `pre-commit`; spot-check the wrong-paste path under `make spice-ryll` before reporting done. See *Brief for step 3a*. |
| 3b   | low    | sonnet | none      | Update `docs/spice-test-inventory.md` row for *Text clipboard client → server* from `—` to `binary: [locked-bootloader](plans/PLAN-locked-bootloader.md)`. Match the existing `binary:` row format. See *Brief for step 3b*. |
| 3c   | low    | sonnet | none      | Closeout commit: master plan execution table row 3 -> *Complete*; success criteria checklist all ticked; `docs/plans/index.md` master plan row -> *Complete*; phase 3 plan's own success criteria ticked. Regenerate `docs/images/boot-sequence.png` only if `make screenshot` produces a non-trivially-different PNG. See *Brief for step 3c*. |

### Brief for step 3a

Edit `src/bootloader.rs`. Two narrow changes:

**Change 1 — wrong-paste suffix on its own row.**

Add a `wrong_indicator_row: usize` field to
`BootloaderScene` (same row used as `input_row + 1` in the
`run_blob_and_paste` setup; initialised in `run` alongside
the other `*_row` fields, default to `start_row + 7` to
match the input row's `start_row + 6`).

In `run_blob_and_paste`, after computing `input_row =
start_row + 6`, also set `self.wrong_indicator_row =
input_row + 1`.

In the `PasteOutcome::Wrong(len)` branch of the outer paste
loop, replace the existing inline suffix-rendering block
(everything between `self.renderer.clear_row(input_row);
self.renderer.draw_text_at(INPUT_PROMPT, 0, input_row); let
mut col = INPUT_PROMPT.len(); ...` and the closing
`Event::LineRendered`) with:

- `clear_row(input_row)` — wipes the previous echo.
- `draw_text_at(INPUT_PROMPT, 0, input_row)` — bare prompt,
  no suffix. The next `capture_paste` call's echo starts
  fresh at `input_col_start` with no row contention.
- `clear_row(self.wrong_indicator_row)` — clears the prior
  indicator (so the count updates in place from "1 of 3" to
  "2 of 3" without ghost digits).
- Render `(wrong, attempt N of 3)` on
  `self.wrong_indicator_row` via the existing per-glyph
  pattern (use `format_u32` for the digits; literal
  `(wrong, attempt `, ` of `, `)` framing as today).
- `note_row(self.wrong_indicator_row)` so the success-path
  region-clear wipes it.
- Push `Event::LineRendered { row:
  self.wrong_indicator_row, ... }` to keep the serial drain's
  row-by-row narrative complete.

**Change 2 — silent-wait fires regardless of buffer length.**

In `capture_paste`, change:

    if len == 0 && idle_ms >= PASTE_SILENT_WAIT_MS {
        return PasteOutcome::Timeout;
    }

to:

    if idle_ms >= PASTE_SILENT_WAIT_MS {
        return PasteOutcome::Timeout;
    }

and update the surrounding comment to reflect the new
semantics: "60 s without any key arriving triggers timeout
regardless of buffer state — covers the operator-walked-
away-mid-paste and ryll-disconnected-mid-paste cases as well
as the never-started case."

Both changes preserve all other behaviour (event emission
order, modifier state machine, wrong-paste counter, timeout
flow). Re-read the existing comments around these branches
and edit any that contradict the new behaviour.

Verification:
1. `make build` clean.
2. `pre-commit run --all-files` clean.
3. **Operator-driven sanity check** (in management session):
   `make spice-ryll`. Walk the wrong-then-correct path —
   paste `wrong`, observe the `(wrong, attempt 1 of 3)` line
   on the row below the input prompt rather than appended to
   it. Paste correct, observe boot continues. (Testing
   change 2 manually requires walking away from a partly-
   typed paste for 60 s; not strictly required for sign-off
   if the smoke test of change 1 also passes; the change-2
   logic is small and well-bounded.)

Do NOT commit. Leave the changes in the working tree for
the management session to review.

Commit message subject: `Polish bootloader paste-prompt
layout and timeout.`

### Brief for step 3b

Edit `docs/spice-test-inventory.md`. Find the row for *Text
clipboard client → server*. Change its Status column from
`—` to:

    binary: [locked-bootloader](plans/PLAN-locked-bootloader.md)

Match the format of existing `binary:` rows (some of the
eight `binary:` rows in the file already have plan links;
copy whichever convention is prevalent — most likely
`binary: [link-text](path)`). The *Text clipboard server →
client* row stays at `—` (still stubbed; that's master-plan
Future work for the vdagent / virtio-serial milestone, not
this milestone).

Verification: `pre-commit run --all-files` clean (markdown
files have no automated checks today, but the
trim-trailing-whitespace and end-of-file hooks still run).

Do NOT commit. Leave the change in the working tree for
review.

Commit message subject: `Mark client→server clipboard row
in inventory.`

### Brief for step 3c

Closeout commit. Mechanical edits:

1. `docs/plans/PLAN-locked-bootloader.md`:
   - Execution table row 3: status column from `Not started`
     to `Complete (commits 3aSHA, 3bSHA, 3cSHA)`. Use the
     actual short SHAs of the three commits this phase
     produced — you can collect them from `git log --oneline
     -5` after the prior two commits land. (3c's own SHA is
     the one this commit creates; reference it
     consistently with the existing phase 1 / phase 2 rows
     even though it is the commit-being-made.)
   - *Success criteria* checklist at the bottom: tick every
     item, with a one-line note per item citing the commit
     that landed it (or the verification step that
     confirmed it). Items already verified by Phase 2
     smoke-test stay ticked.

2. `docs/plans/PLAN-locked-bootloader-phase-03-docs.md`
   (this file): tick the *Success criteria* below.

3. `docs/plans/index.md`: master plan row's Status column
   from `Phase 1-2 complete, Phase 3 in progress` to
   `Complete (commits ...)` matching the format of the
   *First playable* row's `Complete` entry. The phases-list
   column gets `[3. Closeout](PLAN-locked-bootloader-phase-03-docs.md)`
   in place of `3. Docs (phase plan pending)` (the file
   landed when this phase plan was created, so the link can
   be updated; if it was already updated, leave it alone).

4. `docs/images/boot-sequence.png`: run `make screenshot`.
   Compare the new PNG against the committed one (`git diff
   docs/images/boot-sequence.png` will report binary-changed
   or no-change). If no change: do not include the PNG in
   this commit. If changed: include the PNG with a one-line
   note in the commit body explaining what changed about the
   captured frame.

Verification:
1. `pre-commit run --all-files` clean.
2. Visual sanity-check on the phase 3 plan's own *Success
   criteria* — every box ticked, no orphan unfinished work
   in the commit history.

Do NOT commit. Leave the changes in the working tree for
review.

Commit message subject: `Close out locked-bootloader
milestone.`

## Agent guidance

Inherit the master plan's *Agent guidance* and the phase 2
plan's *Agent guidance* phase-2-specifics where relevant.
Phase-3-specific:

- **Both code changes in step 3a are small and well-
  bounded.** The wrong-paste-row change is mechanical
  (introduce a row field, move the suffix render block);
  the silent-wait change is a single condition edit plus a
  comment revision. Sonnet at low effort with a thorough
  brief succeeds; opus is overkill.
- **Steps 3b and 3c are documentation only** and must not
  modify any source file. If a sub-agent edits a `.rs` file
  during 3b or 3c, that is a brief-mismatch — review and
  reject.
- **Operator smoke test for step 3a** (wrong-paste path) is
  worth running, but is not a sign-off blocker for change
  2 (silent-wait); change 2's logic is small enough that
  inspection plus the existing four-flow-paths confidence
  from Phase 2 is sufficient.

## Administration and logistics

### Success criteria

This phase is complete when:

- [x] `src/bootloader.rs` renders the wrong-paste suffix on
      its own row (`input_row + 1`), not appended to the
      input prompt row. The success-path region-clear wipes
      it via `note_row`. *(Step 3a, commit ffa84f4.)*
- [x] `src/bootloader.rs::capture_paste` fires the
      silent-wait timeout on `idle_ms >= PASTE_SILENT_WAIT_MS`
      regardless of buffer length. Comment around the
      branch reflects the new semantics. *(Step 3a, commit
      ffa84f4.)*
- [x] An operator-driven `make spice-ryll` smoke test of
      the wrong-then-correct flow path confirms the suffix
      no longer overlaps the operator's typed echo.
      *(Operator-confirmed at the end of step 3a, before
      this closeout commit.)*
- [x] `docs/spice-test-inventory.md` *Text clipboard client
      → server* row has a `binary: [...]` plan link.
      *(Step 3b, commit 3a4f7aa.)*
- [x] `docs/plans/PLAN-locked-bootloader.md` Execution
      table row 3 is *Complete (commits ...)* and every
      *Success criteria* item in that file is ticked.
      *(This closeout commit.)*
- [x] `docs/plans/index.md` master plan row Status is
      `Complete (commits ...)`. *(This closeout commit.)*
- [x] `make build`, `make release-verify`, and `make
      screenshot` all continue to pass. `docs/images/boot-
      sequence.png` regenerated only if the captured frame
      changed. *(`make screenshot` ran during step 3a; PNG
      was byte-identical, no regeneration committed.)*
- [x] `pre-commit run --all-files` exits 0 across all
      three commits. *(Verified at each commit.)*

### Future work

Items deliberately deferred from this phase (and from the
master plan, restated for visibility):

- **R/I/A prompt indecision timeout for unattended-CI
  hygiene.** Current design polls forever; a 5-minute (or
  user-configurable) fallback to ACPI shutdown would be a
  kindness for unattended runs. Master-plan-future-work.
- **Cosmetic timing tuning** of retry sleep, countdown
  cadence, and prompt responsiveness if a future scene
  reveals friction. None reported in the Phase 2 smoke test;
  no iteration needed here.
- **A "secure" hidden-input variant** of the awaiting-paste
  step for any future scene that simulates a real password
  prompt. Master-plan-future-work.
- **Host-side state-machine tests.** Extracting the clock
  and renderer behind a trait so the state machine can be
  exercised in `cargo test`. Worth doing before scene #3
  lands; not load-bearing for closing out scene #2.
- **Real cryptographic content.** Replacing plain base64
  with an actual encrypted payload requiring a key.
  Master-plan-future-work.
- **vdagent / virtio-serial.** The on-screen blob in step 5
  becoming a real SPICE clipboard server → client write.
  Master-plan-future-work.

### Bugs fixed during this work

(Populated during execution.)

### Documentation index maintenance

On creation of this phase plan: `docs/plans/index.md` row
for the master plan should already say "Phase 1-2 complete,
Phase 3 in progress" and link `[3. Closeout](PLAN-locked-
bootloader-phase-03-docs.md)`. If the index still says
"3. Docs (phase plan pending)", update the link in the same
commit that creates this file.

On completion: bump the master plan row to *Complete
(commits ...)* and link the phase 3 plan as `[3.
Closeout](...)`. The master plan's own Execution table row 3
also flips at this point.

### Back brief

Before executing any step of this plan, please back brief
the operator as to your understanding of the plan and how
the work you intend to do aligns with that plan.
