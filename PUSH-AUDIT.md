Thanks for your work on this. I appreciate it. Some final checks
before I push.

## How to use this runbook

The pre-push audit splits into two waves:

**Wave 1 — mechanical.** Build verification, lint, test suite, and
the parts of style conformance that grep can answer.  Wrapped in a
single shell script so it runs as one tool approval.  Always run
wave 1 first; wave 2 is only worth spending on if wave 1 passes.

**Wave 2 — judgment.** Code-quality, test-coverage, documentation,
and security review.  Some of this is mechanical (TODO/FIXME/dead-
code grep, unsafe block list) and is wrapped in a second script;
the rest needs sub-agents to read code and apply judgment.  The
four judgment agents are independent and can be spawned in
parallel.

The management session reviews all findings, fixes any issues, and
confirms the push.

**Note:** The `tools/audit/wave1.sh` and
`tools/audit/wave2-mechanical.sh` scripts referenced below need to
be created for this repo. Adapt them from
`shakenfist/ryll/tools/audit/` as a starting point; the mechanical
portions are mostly reusable. The UEFI-specific additions are
called out inline.

## Wave 1: Mechanical checks

Run the consolidated script (one approval):

```
tools/audit/wave1.sh
```

It should perform (and exit non-zero on any failure):

- `pre-commit run --all-files`
- `./scripts/check-rust.sh check` (rustfmt + clippy in the
  `no_std`-aware build environment)
- Host-side `cargo test --workspace` for anything testable off-target
- Target-side smoke test: build the EFI binary and run it under
  QEMU/OVMF with a short scripted serial interaction, asserting a
  known log marker appears
- Mechanical style checks: no raw `println!`/`eprintln!` (we have
  no host stdout in UEFI anyway — any such call is a bug), advisory
  long-line check on Rust files in the diff vs the base branch

Exit codes:

| Code | Meaning                          |
|------|----------------------------------|
| 0    | all wave 1 checks passed         |
| 1    | pre-commit failed                |
| 2    | rustfmt or clippy failed         |
| 3    | cargo test failed                |
| 4    | QEMU/OVMF smoke test failed      |
| 5    | raw `println!`/`eprintln!` found |

If wave 1 fails, fix the cause and re-run before spending on
wave 2.

### Style conformance — judgment portion

The script covers what grep can prove.  The remaining style
questions need a sub-agent to read code:

| Setting | Value |
|---------|-------|
| Model   | sonnet |
| Effort  | low    |

**Brief for sub-agent (only if wave 1 passes):**

Check `git diff <base>...HEAD` for adherence to project conventions
in `AGENTS.md`:

- `no_std` discipline: no accidental `std::` imports, no allocations
  outside the documented allocator region, no floating-point use
  without explicit justification
- UEFI protocol access: all protocol opens go through the documented
  helper, no raw `uefi::table::Boot` accesses sprinkled through the
  code
- Ring buffer discipline: event insertion uses the documented
  typed API; no direct writes to the underlying storage
- Serial transport: messages use the generated gRPC-over-serial
  types, not hand-rolled byte packing
- Scene code: scene transitions go through the scene manager, not
  by mutating global framebuffer state directly
- Naming: feature names and message identifiers agree with the
  SPICE channel mapping table in `DESIGN.md`

Report a short list of any violations found.  If none, say
"Style checks passed."

## Wave 2: Deeper review

Only run wave 2 after wave 1 passes.

Start with the consolidated mechanical script (one approval):

```
tools/audit/wave2-mechanical.sh
```

It reports (does not block; never exits non-zero on findings):

- TODO / FIXME / HACK / XXX in changed source files
- Newly added `#[allow(dead_code)]` annotations
- Count of new `#[test]` functions vs Rust files changed
- Documentation files touched (warns if none — the diff may have
  merited doc updates)
- New `unsafe {}` blocks — UEFI code has these legitimately; the
  point is to surface them for review, not ban them
- New `.unwrap()` / `.expect()` in changed files (raw list — review
  whether each is in test code or panic-safe in production; UEFI
  panics are particularly unkind)

Then spawn the judgment agents below.  They can run in parallel.

### 2a. Code quality

| Setting | Value |
|---------|-------|
| Model   | sonnet |
| Effort  | medium |

**Brief for sub-agent:**

The mechanical script already extracted TODO/FIXME comments, new
`#[allow(dead_code)]`, `unsafe{}` blocks, and unwrap/expect lists.
Take that report as input.

Add the judgment-level review on the diff (`git diff <base>...HEAD`):

- **Duplicated code:** Are there significant blocks of duplicated
  logic? Look for copy-paste patterns across scene handlers or
  message parsers.
- **Missed abstractions:** Should any new code be extracted into a
  shared module? Look for logic a second scene or a second
  channel-exercise would likely need.
- **Memory layout hazards:** Do any new statics land in addresses
  we care about? Large `.bss` arrays in `no_std` UEFI builds have
  bitten the instar project before; flag any new large statics.
- **Triage the script's raw findings:** for each TODO/unwrap/unsafe
  the mechanical script flagged, say blocking or advisory and why.

<!-- shared-block: comment-proportion v1 -->
Comment proportion (shared block; do not edit -- the canonical
copy lives in shakenfist/development at
`templates/shared-blocks/comment-proportion.md`):

- A comment or docstring earns its length by saying what the code
  cannot: the contract, the units, the failure modes, the reason a
  surprising choice is correct. Restating the code in prose is not
  documentation.
- Treat as candidates any added comment or docstring that is longer
  than the code it documents, and any comment block over roughly
  fifteen lines attached to a body under ten. These are candidates,
  not verdicts -- a subtle algorithm, a public API contract, or a
  hard-won bug explanation can justify the length.
- Where the length is not justified the finding is advisory, and
  the fix is to cut the restatement rather than delete the comment:
  keep the why, drop the line-by-line narration of the what.
- Prose that documents user-visible behaviour rather than the
  implementation usually belongs in `docs/`, with the comment
  reduced to a pointer.
<!-- shared-block-end -->

<!-- shared-block: source-file-size v1 -->
Source file size (shared block; do not edit -- the canonical
copy lives in shakenfist/development at
`templates/shared-blocks/source-file-size.md`):

- Where a repository tracks whole-file human review, a file's cost
  is its length times how often it is touched: every change
  discards the review of the whole file, and the next session
  re-reads all of it. That, rather than taste, is why length is
  worth raising in review at all.
- Treat a source file over roughly 800 lines as a candidate to
  split, and one over roughly 1,500 as wanting a stated reason to
  stay whole. These hold whether or not a repository tracks review
  per file: tracking is what makes the cost repeat and become
  measurable, not what makes a long file expensive to read. Both
  are advisory. Neither is a gate, there is no hard cap, and a
  reviewer who raises one is opening a question, not recording a
  defect.
- Generated files, vendored trees and protocol or data tables are
  exempt: they are not read the way source is, and a tool that
  counts them is measuring the wrong thing.
- Split along a seam that already exists -- one module's public
  entry point, one check, one subcommand, one endpoint -- so that
  a later change touches one of the pieces rather than all of
  them. A file split at a line number rather than at a seam is
  worse than the long file it replaced.
- Length is never reduced by deleting the comments and docstrings
  that explain why the code is the way it is. Those are what make
  a long file reviewable, and trading them for a line count makes
  the review worse while making the number better. Cut duplicated
  scaffolding first; see `comment-proportion` for what earns its
  length.
<!-- shared-block-end -->

<!-- shared-block: plan-references-in-code v1 -->
Plan references in code (shared block; do not edit -- the
canonical copy lives in shakenfist/development at
`templates/shared-blocks/plan-references-in-code.md`):

- Code, comments, docstrings, test names, fixture descriptions and
  configuration describe the software as it is now. Which plan,
  phase, step or decision produced a line is history, and the plan
  and the commit log already keep it. Do not write "added in phase
  5", "per decision 3", "pending step 5f" or "the phase-4 leaks
  pass": a reader of the code has not read the plan, and the
  number tells them nothing.
- Where a comment cites a plan to explain why the code is the way
  it is, the explanation belongs in the comment. Write the reason
  -- the constraint, the measurement, the failure it prevents --
  and drop the citation. A pointer standing in for the reasoning
  costs every reader a detour, and rots when the plan is archived
  or renumbered.
- A plan link is acceptable only for work that is not built yet: a
  deliberate gap or refusal whose lifting is planned, where
  "deferred; see `PLAN-foo.md`" tells the reader the gap is known.
  The link comes out when the work lands. Prefer an issue link
  where one exists, and write a plan in another repository as an
  absolute URL; the `plan-source-references` audit checks that
  these links resolve.
- Plan documents and commit messages may cite phases and decisions
  freely; recording that history is their job.
- "Phase" in its ordinary sense -- a two-phase commit, a compiler's
  link phase -- is not a plan reference.
- A plan reference a diff adds to code is a finding to fix before
  pushing. References on lines the diff does not touch are backlog,
  not findings against the change.
<!-- shared-block-end -->

<!-- shared-block: python-version-discipline v1 -->
Python version and typing (shared block; do not edit -- the
canonical copy lives in shakenfist/development at
`templates/shared-blocks/python-version-discipline.md`):

- No syntax or standard library API newer than the floor in
  `requires-python`. Structural pattern matching, `X | Y` unions in
  annotations evaluated at runtime, `tomllib`, and
  `datetime.UTC` each raise on an interpreter the package still
  claims to support, and none of them fail in CI when CI runs only
  the newest version. This is the finding to look for first: it is
  a real break on a real user's machine, not a style point.
- New and modified code carries type hints, and mypy is expected to
  be clean over it. A project part way through a staged rollout is
  held to the new code, not to the whole tree.
- Prefer the walrus operator and f-strings where they make the code
  read better, subject to the floor above.
- Raising the floor in `requires-python` is a supported-platforms
  decision, not a convenience: it drops users. If it is genuinely
  right, the platforms table, `requires-python` and
  `constraints.python` in `renovate.json` all move together.
<!-- shared-block-end -->

!!! note "In this project"
    There is no `pyproject.toml` and no `requires-python` here, so the
    floor the block speaks of is not written down anywhere. Python is
    three vendoring scripts under `scripts/` which run on a
    developer's machine, never in the firmware and never in CI, so the
    floor in practice is whatever Debian 13 ships. Hold new code to
    that, and to the typing and readability points, which apply
    unchanged.

Report findings as a bullet list with file, line, and
blocking/advisory tag.

### 2b. Test review

| Setting | Value |
|---------|-------|
| Model   | sonnet |
| Effort  | medium |

**Brief for sub-agent:**

Review the diff (`git diff <base>...HEAD`) for test coverage:

- Does every new host-testable function have test coverage?
- For target-side code (UEFI-only), is there a scripted QEMU/OVMF
  interaction that exercises the new behaviour end-to-end?
- Do the tests include adversarial cases (malformed serial input,
  framebuffer edge cases, input events at scene boundaries)?
- Are there any assertions that test implementation details
  rather than behaviour (fragile tests)?

<!-- shared-block: functional-test-coverage v1 -->
Functional test coverage (shared block; do not edit -- the
canonical copy lives in shakenfist/development at
`templates/shared-blocks/functional-test-coverage.md`):

- The standard is "do we run the code to do the real thing, and
  does it work as intended". Every subcommand exposed on the command
  line, and every endpoint exposed by an API, should have a test
  that exercises it for real rather than against a mock of itself.
- For a change that adds or alters user-visible behaviour, the
  question to answer is which functional test would have failed
  before it and passes after. If there is none, that is the finding,
  and it is a finding about this change rather than a note for
  later.
- Unit tests are held to no coverage percentage, but a branch that
  is reachable from outside the process and has no test is worth
  naming. Error paths and argument validation are where this bites:
  they are the code most often written once and never run again.
- Mocking the system under test proves nothing. Mock the boundary --
  the network, the clock, the hypervisor -- and let the code being
  tested actually run.
- Where a gap is real but out of scope for the change in hand, say
  so plainly and record it, rather than silently widening the
  change or silently leaving it unsaid.
<!-- shared-block-end -->

!!! note "In this project"
    There is no command line and no API to exercise: the deliverable
    is a UEFI binary. The functional tests are the scripted QEMU/OVMF
    runs -- `scripts/test.sh`, `scripts/digest-payload-smoke.sh`,
    `scripts/screenshot-modes.sh` -- which boot the real firmware
    under the real OVMF and read the result back off the framebuffer
    or the serial link. "Mock the boundary" here means QEMU is the
    boundary; a test that reimplements the scene state machine on the
    host instead of booting it proves nothing.

Report findings as a bullet list grouped by file.

### 2c. Documentation review

| Setting | Value |
|---------|-------|
| Model   | sonnet |
| Effort  | medium |

**Brief for sub-agent:**

Check that documentation matches the current code state.  Read the
diff and verify:

<!-- shared-block: readme-discipline v1 -->
README discipline (shared block; do not edit -- the canonical
copy lives in shakenfist/development at
`templates/shared-blocks/readme-discipline.md`):

- New user-visible features are documented in `docs/` (and
  `ARCHITECTURE.md` / `AGENTS.md` where appropriate), not by
  adding bullets to `README.md`.
- `README.md` is a pitch: what the project is, who it is for,
  minimal installation instructions, a small number of usage
  examples, and curated absolute links into `docs/`. It only
  changes when the pitch, the install story, or the
  documentation links change.
- README growth is itself a finding: if the diff adds README
  content that belongs in `docs/`, flag it as blocking and
  move it.
<!-- shared-block-end -->

<!-- shared-block: llm-doc-discipline v1 -->
AGENTS.md and ARCHITECTURE.md discipline (shared block; do not
edit -- the canonical copy lives in shakenfist/development at
`templates/shared-blocks/llm-doc-discipline.md`):

- `AGENTS.md` is a working guide: the conventions, invariants and
  gotchas an agent cannot infer by reading the code, plus curated
  links into `docs/`. It is loaded into every session, so every
  line costs context on every task.
- `ARCHITECTURE.md` is a map: the component inventory, how data
  moves between components, and why the shape is the way it is.
  A deep dive on one subsystem belongs in `docs/`, where humans
  benefit from it too.
- One canonical home per fact. If `docs/` covers it, link to it
  instead of restating it -- and the same rule applies between
  `AGENTS.md` and `ARCHITECTURE.md`.
- Neither file is a reference manual, a runbook, or a changelog.
  CLI flags, configuration keys, wire protocols, step-by-step
  procedures and plan history go to `docs/`.
- Growth in either file is itself a finding: if the diff adds
  content that belongs in `docs/`, flag it as blocking and move
  it.
<!-- shared-block-end -->

<!-- shared-block: diagram-discipline v1 -->
Diagram discipline (shared block; do not edit -- the canonical
copy lives in shakenfist/development at
`templates/shared-blocks/diagram-discipline.md`):

- A diagram of *structure or flow* -- components and the arrows
  between them, an ordered exchange of messages, a state machine
  -- is written as a fenced `mermaid` block, not drawn in ASCII.
  GitHub renders those natively and the mkdocs sites render them
  through `pymdownx.superfences`, so the same source is a picture
  in both places.
- Not every box of characters is a diagram. These stay as plain
  code fences, because mermaid cannot express them and would lose
  what they show: directory and file trees; memory maps, address
  space layouts and register or bit-field diagrams, where column
  alignment carries the meaning; wire-format and on-disk byte
  layouts; captured terminal output; and tables. The test is
  whether the picture is nodes and edges. Something that is a
  table with lines drawn on it is a table.
- Pick the diagram type that matches the claim: `flowchart` for
  components and data flow, `sequenceDiagram` for an ordered
  exchange between parties, `stateDiagram-v2` for a state
  machine, `erDiagram` for data relationships. A sequence drawn
  as a flowchart has thrown away the ordering it existed to show.
- A new ASCII box-and-arrow diagram in the diff is a finding.
  Converting one the diff already touches is in scope; converting
  every other diagram in the file is not, because a sweep is its
  own change and its own review.
<!-- shared-block-end -->

- `DESIGN.md` reflects any change to the SPICE channel mapping
  table, the two-channel test architecture, or the aesthetic
  direction
- `ARCHITECTURE.md` reflects any new modules, scenes, serial
  message types, or build-tooling layout
- `AGENTS.md` reflects any new dependencies, build commands, or
  conventions
- Plan files in `docs/plans/` are up to date — completed phases
  marked complete, deferred items listed
- If the changes affect how ryll drives the harness, note whether
  `shakenfist/ryll/docs/` needs a matching update

<!-- shared-block: plan-phase-references v1 -->
Plan phase references (shared block; do not edit -- the canonical
copy lives in shakenfist/development at
`templates/shared-blocks/plan-phase-references.md`):

- Documentation outside plans directories describes the current
  state of the software, not the history of how it was built. Do
  not write "implemented in phase 5" or "since phase 3 of the
  two-tier CI plan": a reader wants to know whether a feature
  exists, not which phase of which plan delivered it.
- If a documented behaviour is implemented, describe it plainly.
  If it is planned but not yet implemented, link to the master
  plan in `docs/plans/` instead of citing a phase number.
- Reserve the word "phase" for plan documents. A procedural
  document describing a live multi-stage process (a release
  runbook, say) should call its stages "steps" or "stages", so
  that a phase reference in `docs/` is always a plan smell.
- The consistency audit greps `README.md` and `docs/` (excluding
  plans directories) for "phase <number>". Append
  `<!-- audit-ok: phase-reference -->` to a line only when the
  reference is genuinely not about an implementation plan.
<!-- shared-block-end -->

Report findings as a bullet list. "No documentation gaps found"
is a valid answer.

### 2d. Security review

| Setting | Value |
|---------|-------|
| Model   | opus |
| Effort  | high |

**Brief for sub-agent:**

Security review of the diff (`git diff <base>...HEAD`). This
requires careful judgment — read the actual code, not just the
diff summary.

Our threat model is narrower than a normal networked service:
uncalibrated-sextant runs in a guest VM, under OVMF, and
communicates with the host via QEMU-emulated devices and a serial
link to Ryll. Still, the following classes matter:

- **Untrusted serial input:** Ryll is treated as trusted, but the
  gRPC-over-serial parser still needs to validate lengths, refuse
  unbounded allocations, and not panic on malformed frames.
- **Untrusted device input:** Simple Pointer / Simple Text Input
  events come from whatever QEMU/SPICE deliver. Don't panic on
  unexpected event shapes.
- **`unsafe` in UEFI code:** Every `unsafe {}` should have a safety
  comment justifying the invariant, especially around raw pointer
  access to the framebuffer or protocol tables.
- **Arithmetic overflow:** Framebuffer coordinate maths, ring
  buffer indices, and scene timing counters are all arithmetic
  that can overflow on hostile input. Check for wrapping vs
  checked arithmetic.
- **Resource exhaustion:** Could a malicious or misbehaving Ryll
  cause unbounded memory growth or CPU spin? UEFI has very limited
  memory; this matters.
- **Static memory layout:** Large `.bss` arrays can overlap with
  firmware or guest memory regions. Document and review where new
  large statics land.

<!-- shared-block: path-traversal-review v1 -->
Path construction from outside data (shared block; do not edit --
the canonical copy lives in shakenfist/development at
`templates/shared-blocks/path-traversal-review.md`):

- Treat as a candidate any filesystem path built from a value the
  process did not choose: a request parameter, an image name, tag or
  digest, a layer path, an archive member name, a filename out of a
  configuration file or a database row.
- The question is not whether the value looks dangerous but whether
  the resulting path is *proved* to stay inside its intended base
  directory. Resolve the joined path with `os.path.realpath()` and
  verify it still starts with the base; a check on the untrusted
  component alone is defeated by symlinks and by encodings the
  check did not anticipate.
- Prefer a helper that cannot be forgotten at a call site --
  `safe_path_join()` in occystrap, or the framework's own
  (`send_from_directory` in Flask) -- over an inline guard repeated
  at each join.
- Archive extraction is the case most often missed: a member name
  inside a tarball or zip is attacker-controlled in exactly the same
  way as a request parameter.
- Where a bare join is correct because every component is
  process-chosen, say so in a comment rather than leaving the
  reader to re-derive it.
<!-- shared-block-end -->

!!! note "In this project"
    The firmware builds no filesystem paths at all -- it has no
    filesystem beyond the ESP that OVMF loaded it from. The block
    applies to the host side: the vendoring scripts under `scripts/`
    which write generated Rust into `src/`, and the QEMU and
    screenshot wrappers which build paths from arguments and
    environment variables. A path assembled from `$1` in a shell
    script is the same defect as one assembled from a request
    parameter.

Report findings with severity (critical / high / medium / low /
informational). For each finding, state the file, line, the
vulnerability class, and a recommended fix.

## Management session checklist

After all agents complete, the management session should:

- [ ] Wave 1 passed (build, style, QEMU/OVMF smoke).
- [ ] Wave 2 findings reviewed.
- [ ] Any blocking findings from 2a/2b/2c have been fixed and
      re-verified.
- [ ] Any security findings from 2d have been assessed — critical
      and high must be fixed before push.
- [ ] The commit history is clean (no fixup commits that should be
      squashed, no accidental files).
- [ ] The branch is up to date with the target branch (rebase if
      needed).
- [ ] Ready to push.
