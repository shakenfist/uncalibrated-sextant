Thanks for your work on this. I appreciate it. Some final checks
before I push.

## How to use this template

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

Report findings as a bullet list grouped by file.

### 2c. Documentation review

| Setting | Value |
|---------|-------|
| Model   | sonnet |
| Effort  | medium |

**Brief for sub-agent:**

Check that documentation matches the current code state.  Read the
diff and verify:

- `README.md` reflects any new features, changed usage, or updated
  project structure
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
