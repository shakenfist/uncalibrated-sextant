# First playable — phase 1: Cargo / `no_std` UEFI skeleton

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read `DESIGN.md` (especially
*First milestone scope*) and `docs/plans/PLAN-first-playable.md`.

Phase 1 is the bootstrap: turn this design-only repository into a
Rust project that produces a buildable UEFI application binary. No
interesting rendering yet, no `make qemu` yet — just a `.efi` file
that `file(1)` agrees is a valid EFI application.

This phase is load-bearing because the Cargo / toolchain /
linker configuration has to be *exactly* right to produce a working
EFI binary. Errors here fail silently or with obscure messages;
errors in later phases are much more diagnosable. It is worth being
careful, and worth verifying the output byte-for-byte at the end.

## Situation

The repository currently contains design documents only (commits
`649b8c8` and `862b638`): `README.md`, `DESIGN.md`,
`ARCHITECTURE.md`, `AGENTS.md`, `PLAN-TEMPLATE.md`,
`PUSH-TEMPLATE.md`, and `docs/plans/`. There is no Rust code, no
Cargo manifest, no toolchain pin, and no `.gitignore`.

**Relevant prior art, and what we cannot reuse from it.** Two
neighbouring projects look like they might provide a template, but
neither actually does for Phase 1:

- `shakenfist/instar` is bare-metal, not UEFI. It loads its guest
  binaries at a fixed address (0x20000) via a custom linker script
  and a bespoke VMM, and builds with nightly Rust and Docker. Its
  Cargo layout is consequently very un-UEFI-like, and copying it
  would be wrong. Its gRPC-over-serial transport (under
  `crates/guest-protocol`) is real prior art for a *later*
  milestone, not this one.
- `shakenfist/uefi-latency-guest` is C, not Rust. Its `Makefile`
  is useful reference for Phase 2 (ESP image assembly,
  `qemu-img convert`), but has nothing to offer Phase 1.

We are therefore establishing the first Rust+UEFI pattern for this
workspace, drawing on the `uefi-rs` community conventions directly
rather than from local precedent.

## Mission and problem statement

Produce a minimal, buildable UEFI application that:

- compiles cleanly on stable Rust targeting
  `x86_64-unknown-uefi`;
- depends on the `uefi` crate (from the rust-osdev/uefi-rs
  project) at a pinned recent version;
- clears the screen via the UEFI Simple Text Output protocol,
  prints `Hello from Uncalibrated Sextant`, waits for a key, and
  exits with `Status::SUCCESS`;
- produces a `.efi` output file that `file(1)` identifies as
  `PE32+ executable (EFI application) x86-64`.

The binary does not yet need to *run* under QEMU — Phase 2 wires
up `make qemu`. Phase 1 ends at "builds and is the right file
type".

The master plan calls for "GOP text output" in its Phase 1 sketch;
to clarify: this phase uses the **Simple Text Output Protocol**
(`SIMPLE_TEXT_OUTPUT_PROTOCOL`), which is the trivial text
protocol every UEFI firmware provides. The Graphics Output
Protocol (GOP) — framebuffer graphics — is not introduced until
Phase 4.

## Open questions

These should be resolved by Step 1 below and recorded in the
phase plan's *Bugs fixed during this work* or as a short note in
the commit message.

- **`uefi` crate version.** Pin a specific current release rather
  than a floating range. Check `https://crates.io/crates/uefi`
  for the latest stable. The API between 0.2x and 0.3x had
  significant churn (notably around `SystemTable<Boot>` vs the
  globals-based API and the `#[uefi::entry]` signature); pick one
  version and stick with it for the whole project until there's a
  reason to upgrade. Record the decision and the API generation
  in a short comment at the top of `src/main.rs`.
- **Rust toolchain.** Confirm stable Rust can target
  `x86_64-unknown-uefi` without any `-Z` flags. The target has
  been Tier 2 and stable-accessible for a while; the `build-std`
  feature (which needs nightly) should *not* be required. If it
  turns out to be needed for the `uefi` crate version we pick,
  revisit the version choice rather than adopting nightly.
- **Workspace vs single crate.** Single crate is sufficient for
  Phase 1. Later phases may introduce a workspace if we extract
  the renderer or scene code into separate crates, but we do not
  need to over-structure the project upfront. Default: single
  crate named `uncalibrated-sextant`.
- **`Cargo.lock` commit policy.** This is a binary project, not a
  library, so `Cargo.lock` should be committed for reproducible
  builds. Record in `.gitignore` accordingly (do *not* ignore
  `Cargo.lock`).

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | medium | opus   | none      | See Step 1 below. Research and record, no file changes. |
| 2    | medium | sonnet | none      | See Step 2 below. Create Cargo scaffolding and source. |
| 3    | low    | sonnet | none      | See Step 3 below. Build, verify `file(1)` output, commit. |
| 4    | low    | sonnet | none      | See Step 4 below. Update docs (ARCHITECTURE.md, AGENTS.md). |

Note on effort and models: Step 1 is opus/medium because the
decisions made here shape every later phase and involve cross-
referencing crates.io, release notes, and the target's current
stability status. Steps 2-4 are well-briefed enough for sonnet.

### Step 1 — research and decide versions

**What to produce:** a short findings note (in the commit message
for Step 2, or as an inline comment in `Cargo.toml`) that records:

- The pinned `uefi` crate version (e.g. `0.35`, whatever current is)
- The chosen Rust toolchain version (e.g. `1.85.0` — pick a recent
  stable, but not the absolute latest, to give a bit of CI runway)
- Confirmation that `x86_64-unknown-uefi` is a stable Tier 2 target
  and does not require nightly or `build-std`
- Whether the `uefi` crate's current release uses the older
  `SystemTable<Boot>` entry-point signature or the newer
  globals-based API, since `src/main.rs` will be shaped by that

**How:** `curl -s https://crates.io/api/v1/crates/uefi | jq
.crate.newest_version` (and/or WebFetch the crates.io page),
then skim the crate README on docs.rs for the current API
idiom. Also check the Rust target list:
`rustc --print target-list | grep uefi` confirms stable presence.

**Output:** a short paragraph in the management session (not
committed anywhere) with the pinned versions and any surprises.
No file changes in this step.

### Step 2 — scaffold Cargo and source

Create the following files. Exact contents are starting templates;
adjust to whatever the Step 1 research determined is current API.

**`Cargo.toml`**

```toml
[package]
name = "uncalibrated-sextant"
version = "0.0.1"
edition = "2021"
description = "SPICE channel exercise harness for shakenfist/ryll"
license = "Apache-2.0"
publish = false

[dependencies]
uefi = { version = "<pinned-from-step-1>", features = ["panic_handler", "alloc"] }

[profile.release]
panic = "abort"
lto = true
codegen-units = 1
opt-level = "s"

[profile.dev]
panic = "abort"
```

**`.cargo/config.toml`**

```toml
[build]
target = "x86_64-unknown-uefi"
```

**`rust-toolchain.toml`**

```toml
[toolchain]
channel = "<pinned-from-step-1>"
components = ["rustfmt", "clippy"]
targets = ["x86_64-unknown-uefi"]
profile = "minimal"
```

**`.gitignore`**

```
/target
```

(Do not ignore `Cargo.lock` — this is a binary project.)

**`src/main.rs`** (shape; adjust to current `uefi` crate API)

```rust
// uncalibrated-sextant: SPICE channel exercise harness.
//
// Phase 1 entry point. See DESIGN.md and
// docs/plans/PLAN-first-playable-phase-01-skeleton.md for context.
// Uses uefi crate <version> with the <old|new> entry-point API.

#![no_main]
#![no_std]

use uefi::prelude::*;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    // Clear screen and print a single line of text.
    // Wait for a keypress before returning.

    // ... implementation here ...

    Status::SUCCESS
}
```

The exact lines inside `main` depend on whether the current `uefi`
crate gives you `uefi::system::with_stdout` (newer API) or an
explicit `SystemTable<Boot>` argument (older API). Either way:

1. Clear the screen (`ST::stdout().clear()` or equivalent).
2. Write `"Hello from Uncalibrated Sextant\r\n"`.
3. Block on a keypress via `stdin().read_key()` in a loop, calling
   `boot::stall(10_000)` (10 ms) between polls, until a key is
   returned.
4. Return `Status::SUCCESS`.

### Step 3 — build and verify

Run:

```
cargo build --release
file target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi
```

Expected output from `file`:

```
target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi:
    PE32+ executable (EFI application) x86-64, for MS Windows
```

The "for MS Windows" tail is correct — EFI applications share the
PE32+ format with Windows binaries.

If `file` reports anything else (ELF, shared object, plain PE32
without the +64), the build configuration is wrong; debug before
proceeding.

Once verified, commit the work. Commit message should:

- be at most 50 chars on the first line, ending in a period
  (CLAUDE.md convention);
- mention the pinned `uefi` crate and Rust toolchain versions;
- include the original `Prompt:` block from this phase's
  invocation;
- include `Co-Authored-By:` with model, context, effort, and any
  other active quality-affecting settings;
- include `Signed-off-by: Michael Still <mikal@stillhq.com>`.

### Step 4 — documentation updates

Update:

- **`ARCHITECTURE.md`** — replace the "stub" note with a brief
  description of the Cargo layout now in place (single crate,
  `src/main.rs` entry, `x86_64-unknown-uefi` target, pinned `uefi`
  crate). Keep it short; later phases will expand.
- **`AGENTS.md`** — add a *Build commands* section with
  `cargo build --release` as the current build command, and note
  that `make qemu` does not yet exist (coming in Phase 2).
- **`README.md`** — add a terse *Building* section pointing at
  `cargo build --release` and noting OVMF/QEMU are not yet
  required (Phase 2).

Commit as a separate logical change from Step 3, or squash into
Step 3's commit if the diff is genuinely trivial.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. One
phase-specific emphasis: **do not skip the `file(1)` verification
in Step 3.** A misconfigured linker can produce a `.efi` file that
looks plausible to `ls` but is not actually an EFI application.
Running `file` is the cheapest way to catch that and it must be
part of the review checklist before the phase is marked complete.

## Success criteria

Phase 1 is complete when:

- [ ] `cargo build --release` succeeds on a fresh clone with the
      pinned toolchain.
- [ ] `file` reports `PE32+ executable (EFI application) x86-64`
      on the produced binary.
- [ ] `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`,
      `rust-toolchain.toml`, `.gitignore`, and `src/main.rs` are
      committed.
- [ ] `ARCHITECTURE.md`, `AGENTS.md`, and `README.md` reference
      the new build command.
- [ ] `pre-commit run --all-files` is **not** expected to pass
      yet — pre-commit is Phase 3 work. Do not add a
      pre-commit config in this phase.

## Future work

None specific to this phase. Items deferred out of the whole
milestone are listed in the master plan.
