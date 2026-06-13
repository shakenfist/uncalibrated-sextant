# Title for the plan

## Prompt

Before responding to questions or discussion points in this
document, explore the uncalibrated-sextant codebase thoroughly.
Read relevant source files, understand existing patterns (UEFI
`no_std` Rust via the `uefi-rs` crate, GOP framebuffer rendering,
Simple Text Input Ex and Simple Pointer Protocol for input, the
gRPC-over-serial transport lifted from `shakenfist/instar`, scene
state and the ring buffer, the on-screen QR digest). Ground your
answers in what the code actually does today. Do not speculate
about the codebase when you could read it instead. Where a
question touches on external concepts (UEFI Boot Services, GOP,
OVMF firmware behaviour, the SPICE protocol and its channels,
QEMU's serial/display/audio/USB emulation, the SPICE client
surface exposed by ryll or virt-viewer), research as needed to
give a confident answer. Flag any uncertainty explicitly rather
than guessing.

All planning documents should go into `docs/plans/`.

Consult `DESIGN.md` for the intent of the project, the SPICE
channel → in-game mechanic mapping table, the two-channel test
architecture (gRPC-over-serial plus on-screen QR digest, both
fed from one ring buffer), and the aesthetic direction. Consult
`ARCHITECTURE.md` for the implementation shape once it is
fleshed out, and `AGENTS.md` for build commands, project
conventions, and code organisation. Key cross-repo references:

- `shakenfist/ryll` — drives this harness over serial, parses
  events, makes assertions; look here for the test-driver side
- `shakenfist/kerbside` and `shakenfist/kerbside-patches` — the
  SPICE proxy layer being exercised
- `shakenfist/uefi-latency-guest` — the minimal UEFI probe this
  project complements; look here for prior art on UEFI Rust
  build conventions
- `shakenfist/instar` — source of the gRPC-over-serial transport
  pattern intended for this project
- External: `uefi-rs` crate documentation, the UEFI spec
  (Graphics Output, Simple Text Input Ex, Simple Pointer, Serial
  I/O protocols), OVMF (the firmware we boot under), and the
  SPICE protocol documentation vendored in `kerbside/docs/`

When we get to detailed planning, I prefer a separate plan file
per detailed phase. These separate files should be named for
the master plan, in the same directory as the master plan, and
simply have `-phase-NN-descriptive` appended before the `.md`
file extension. Tracking of these sub-phases should be done via
a table like this in this master plan under the Execution
section:

```
| Phase | Plan | Status |
|-------|------|--------|
| 1. Cargo skeleton | PLAN-thing-phase-01-skeleton.md | Not started |
| 2. GOP hello      | PLAN-thing-phase-02-gop-hello.md | Not started |
| ...   | ...  | ...    |
```

I prefer one commit per logical change, and at minimum one
commit per phase. Do not batch unrelated changes into a single
commit. Each commit should be self-contained: it should build,
pass tests, and have a clear commit message explaining what
changed and why.

## Situation

...

## Mission and problem statement

...

## Open questions

...

## Execution

...

## Agent guidance

### Execution model

All implementation work is done by sub-agents, never in the
management session. The management session (this conversation)
is reserved for planning, review, and decision-making. This
keeps the management context lean and avoids drowning it in
implementation diffs.

The workflow is:

1. **Plan** at high effort in the management session.
2. **Spawn a sub-agent** for each implementation step with the
   brief from the plan, at the recommended effort level and
   model.
3. **Review** the sub-agent's output in the management session.
   Check the actual files — the sub-agent's summary describes
   what it intended, not necessarily what it did.
4. **Fix or retry** if the output is wrong. Diagnose whether
   the brief was insufficient (improve it) or the model was too
   light (upgrade it), then re-run.
5. **Commit** once the management session is satisfied with the
   result.

This applies to all steps, including high-effort ones. If a
sub-agent can't succeed even with a detailed brief and the
right model, that's a signal the brief needs improving, not
that the management session should do the implementation
itself.

Use `isolation: "worktree"` for sub-agents when the change is
risky or experimental. The worktree is discarded if the output
is unsatisfactory. For safe, well-understood changes,
sub-agents can work directly in the main tree.

### Planning effort

The master plan itself should always be created at **high
effort** — it requires broad codebase understanding,
cross-referencing multiple source files, and making judgment
calls about scope and sequencing.

Each phase plan should specify the recommended effort level
for planning that phase. Phases involving deep protocol
research (SPICE channel semantics, UEFI spec corners,
OVMF-specific behaviour), algorithm understanding, or
architectural decisions should be planned at high effort.
Phases that are mechanical or follow well-established
patterns can be planned at medium effort.

### Step-level guidance

Each phase plan should include a table like this:

```
| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1a   | medium | sonnet | none      | One-sentence summary of what to do and which files to touch |
| 1b   | high   | opus   | worktree  | Why this needs high effort: requires understanding X to do Y |
```

**Effort levels:**
- **high** — Requires reading multiple files, making judgment
  calls, understanding non-obvious invariants (UEFI memory
  layout, firmware quirks, SPICE channel semantics), or
  researching external references.
- **medium** — The plan provides enough context that the
  sub-agent can follow a clear brief. May need to read a few
  files but the approach is well-defined.
- **low** — Purely mechanical changes (rename, reformat, add a
  log line). The brief is a complete instruction.

**Model choice:** The planner should recommend which model is
best suited for each step.

- **opus** — Best for deep reasoning, cross-file architectural
  understanding, subtle correctness judgment, UEFI/SPICE
  research, or intricate implementation where getting it wrong
  would be costly to debug (memory layout bugs in `no_std` code
  are particularly unkind to debug).
- **sonnet** — Good default for well-briefed implementation
  work. Faster and cheaper than opus.
- **haiku** — Suitable for purely mechanical tasks.

**When in doubt, skew to the more capable model.** Saving money
only matters if the outcome is still acceptable.

**Brief for sub-agent:** Write it as if briefing a colleague
who has never seen the codebase. Include: what to change, which
files to touch, what patterns to follow, and any non-obvious
constraints (UEFI memory limits, `no_std` restrictions, fixed
load address, SPICE channel invariants the guest must
maintain). The better the brief, the lower the effort level
needed and the lighter the model that can succeed.

### Management session review checklist

After a sub-agent completes, the management session should
verify:

- [ ] The files that were supposed to change actually changed
      (read them, don't trust the summary).
- [ ] No unrelated files were modified.
- [ ] The code builds (`pre-commit run --all-files` or
      equivalent).
- [ ] Tests pass (host-side `cargo test` and, where applicable,
      the QEMU/OVMF integration harness).
- [ ] The changes match the intent of the brief — not just
      syntactically correct but semantically right.
- [ ] Commit message follows project conventions (including the
      `Co-Authored-By` line with model, context window, effort
      level, and other settings).

## Administration and logistics

### Success criteria

We will know when this plan has been successfully implemented
because the following statements will be true:

* The code passes `pre-commit run --all-files` (rustfmt, clippy
  with `-D warnings`, shellcheck).
* New code follows existing patterns for `no_std` UEFI Rust,
  ring-buffer event flow, and the gRPC-over-serial transport.
* There are unit tests for new logic where host-side testing is
  practical, and the existing tests still pass. Integration
  behaviour in QEMU/OVMF has been smoke-tested.
* Lines are wrapped at 120 characters, single quotes for Rust
  strings where applicable.
* `README.md`, `DESIGN.md`, `ARCHITECTURE.md`, and `AGENTS.md`
  have been updated if the change adds or modifies channels,
  scenes, serial messages, or build tooling.
* Documentation in `docs/` has been updated to describe any new
  features or configuration options.
* If the changes affect how we exercise a SPICE channel, the
  relevant entries in the channel mapping table in `DESIGN.md`
  have been reviewed and updated if needed.

### Future work

We should list obvious extensions, known issues, unrelated bugs
we encountered, and anything else we should one day do but have
chosen to defer to here so that we don't forget them.

...

### Bugs fixed during this work

This section should list any bugs we encounter during
development that we fixed. You should also scan the relevant
github bug tracker to see if there are any directly related
bugs that we should either resolve as part of this master
plan, or at least be aware of when planning.

### Documentation index maintenance

When creating a new master plan from this template, update the
following files in `docs/plans/`:

* **`index.md`** — add a row to the *Master plans* table with
  the creation date, a link to the plan, a one-line intent
  summary, the initial status, and links to each phase plan
  file. Keep the table in chronological order.
* **`order.yml`** — add an entry for the new master plan so it
  appears in the documentation navigation bar. Phase files
  should *not* be added to `order.yml`.

When all phases of a plan are complete, update the status
column in `index.md` to *Complete*.

### Back brief

Before executing any step of this plan, please back brief the
operator as to your understanding of the plan and how the work
you intend to do aligns with that plan.
