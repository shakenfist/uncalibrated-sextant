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

**Build runs inside Docker from the outset.** This plan was
amended after Step 1 research established that developer hosts
should not be required to install Rust natively (the project
owner maintains several Rust projects at different toolchain
versions and keeps hosts clean). This matches the workspace's
existing ryll / instar / imago pattern. Phase 1 therefore
introduces a minimal Docker image carrying the pinned toolchain
and the `x86_64-unknown-uefi` target, plus a tiny Makefile with
`build` and `clean` targets. Phase 2 extends both the image
(QEMU, OVMF, ESP image tools) and the Makefile (`qemu`,
`release`). No host Rust toolchain is required at any step.

The master plan calls for "GOP text output" in its Phase 1 sketch;
to clarify: this phase uses the **Simple Text Output Protocol**
(`SIMPLE_TEXT_OUTPUT_PROTOCOL`), which is the trivial text
protocol every UEFI firmware provides. The Graphics Output
Protocol (GOP) — framebuffer graphics — is not introduced until
Phase 4.

## Open questions

- **`uefi` crate version.** *Resolved by Step 1.* Pin exactly to
  `0.37.0` (published 2026-03-23; MSRV 1.88 per the crate's
  `Cargo.toml`, despite what Step 1 initially reported). Not
  `^0.37` — the
  crate releases frequently and we want the version stable for
  the whole project until there's a reason to bump. Entry-point
  API is the *new* globals-based form: argumentless
  `#[entry] fn main() -> Status` with `uefi::helpers::init()` and
  services accessed via `uefi::system::with_stdout(...)`,
  `uefi::boot::...`, `uefi::runtime::...`. The legacy
  `SystemTable<Boot>` two-argument signature is gone and many
  older tutorials are stale on this point — check the top of
  `src/main.rs` in review to make sure the new form is used.
- **`uefi` crate features.** *Resolved by Step 1.* Use
  `features = ["panic_handler", "alloc", "global_allocator"]`.
  The plan previously sketched `["panic_handler", "alloc"]`;
  that's incomplete. `alloc` gives the crate surface for heap
  collections, but `global_allocator` is now a separate opt-in
  feature that actually installs a UEFI-boot-services allocator
  so `Box` / `Vec` / `String` work at runtime.
- **Rust toolchain.** *Resolved by Step 1.* Pin `1.88.0` (stable,
  released 2025-04-03) in `rust-toolchain.toml`, with
  `targets = ["x86_64-unknown-uefi"]` and
  `components = ["rust-src", "rustfmt", "clippy"]`. A few months
  old, matches the crate's 1.88 MSRV exactly (the Step 1 research
  report was wrong about the MSRV — the crate actually requires
  1.88), and avoids the bleeding edge
  that sometimes regresses. `x86_64-unknown-uefi` is Tier 2
  without host tools but builds cleanly on stable with just
  `rustup target add x86_64-unknown-uefi` — no `-Z build-std`, no
  nightly.
- **Docker base image.** Two plausible bases: the official
  `rust:1.88-slim` image (rustup is already present; just run
  `rustup component add` / `rustup target add` at build time) or
  a Debian/Alpine base with rustup installed by hand. Prefer
  `rust:1.88-slim` for simplicity unless a concrete reason rules
  it out. Decide at Step 2. The image should be as small as
  reasonably possible since Phase 2 will extend it with QEMU and
  OVMF.
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
| 1    | medium | opus   | none      | *Complete.* See research report summarised above in *Open questions*. |
| 2    | medium | sonnet | none      | See Step 2 below. Create Cargo scaffolding, Docker wrapper, and a minimal Makefile. |
| 3    | low    | sonnet | none      | See Step 3 below. Run `make build` inside Docker, verify `file(1)` output, commit. |
| 4    | low    | sonnet | none      | See Step 4 below. Update docs (ARCHITECTURE.md, AGENTS.md, README.md). |

Note on effort and models: Step 1 was opus/medium because the
decisions made there shape every later phase and involved cross-
referencing crates.io, release notes, and the target's current
stability status. Steps 2-4 are well-briefed enough for sonnet.

### Step 1 — research and decide versions (complete)

Executed 2026-04-24 by an opus/medium sub-agent. Findings are
folded into *Open questions* above. Summary:

- `uefi` crate pinned to `0.37.0`
- API generation: new / argumentless / globals-based
- Features: `["panic_handler", "alloc", "global_allocator"]`
- Rust toolchain pinned to `1.88.0` stable
- `x86_64-unknown-uefi` is Tier 2 without host tools; builds on
  stable without `-Z build-std`
- No host Rust toolchain should be required — Docker wraps the
  build from Phase 1 onward

### Step 2 — scaffold Cargo, Docker wrapper, Makefile, and source

Create the following files. The versions below reflect the Step 1
research; use them as-is unless the sub-agent discovers a specific
reason to deviate.

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
uefi = { version = "=0.37.0", features = ["panic_handler", "alloc", "global_allocator"] }

[profile.release]
panic = "abort"
lto = true
codegen-units = 1
opt-level = "s"

[profile.dev]
panic = "abort"
```

Note the `=0.37.0` (exact) rather than `0.37.0` (caret by default)
— the `uefi` crate releases frequently and we want the version
stable for the whole project until there's a reason to bump.

**`.cargo/config.toml`**

```toml
[build]
target = "x86_64-unknown-uefi"
```

**`rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.88.0"
components = ["rust-src", "rustfmt", "clippy"]
targets = ["x86_64-unknown-uefi"]
profile = "minimal"
```

**`.gitignore`**

```
/target
```

(Do not ignore `Cargo.lock` — this is a binary project.)

**`Dockerfile`** (at repo root)

```dockerfile
FROM rust:1.88-slim AS build

RUN rustup component add rust-src rustfmt clippy \
 && rustup target add x86_64-unknown-uefi

WORKDIR /work
```

Keep it minimal. Phase 2 will extend this image with QEMU, OVMF,
and ESP image tooling; do not anticipate those here. `rust:1.88-slim`
is a small Debian-based official image that already carries rustup
and the 1.88.0 toolchain.

**`scripts/build.sh`** (make executable)

```bash
#!/usr/bin/env bash
# Build the uncalibrated-sextant UEFI binary inside Docker.
set -euo pipefail

IMAGE_TAG="uncalibrated-sextant-build:1.88.0"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build the image if it doesn't exist. Cheap to re-run; docker
# build is a no-op if the layer cache is already warm.
docker build -t "$IMAGE_TAG" "$REPO_ROOT"

# Run cargo build inside the container. Mount the source in, mount
# a named volume for target/ so host file ownership stays clean
# and incremental builds are preserved across invocations.
docker run --rm \
    -v "$REPO_ROOT":/work \
    -v uncalibrated-sextant-target:/work/target \
    -w /work \
    "$IMAGE_TAG" \
    cargo build --release
```

Bind-mounting the source read-write and using a named volume for
`target/` avoids the "files owned by root" trap that catches
naive Docker-wrapped Rust builds.

**`Makefile`** (at repo root — minimal in Phase 1; Phase 2 extends)

```makefile
.PHONY: build clean

BINARY := target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi

build:
	./scripts/build.sh

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target
```

The `clean` target removes both the host-side `target/` (in case
anything lands there) and the named Docker volume where the real
build output lives.

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

Use the new globals-based API (uefi 0.37):

1. Call `uefi::helpers::init().unwrap()` first thing.
2. Clear the screen: `uefi::system::with_stdout(|stdout|
   stdout.clear().unwrap())`.
3. Write the banner: `uefi::system::with_stdout(|stdout|
   stdout.output_string(cstr16!("Hello from Uncalibrated Sextant\r\n")).unwrap())`.
4. Block on a keypress via `uefi::system::with_stdin(|stdin|
   stdin.read_key())` in a loop, sleeping via
   `uefi::boot::stall(Duration::from_millis(10))` between polls until a
   key is returned.
5. Return `Status::SUCCESS`.

The sub-agent may need to consult the uefi 0.37 docs for exact
module paths and macro names — these helpers change name slightly
between versions and the above list is approximate. The specific
entry point shape is what's load-bearing; the exact helper names
are not.

### Step 3 — build and verify

Run:

```
make build
```

This invokes `scripts/build.sh`, which builds the Docker image
(cache-friendly; no-op on repeat) and runs `cargo build --release`
inside the container. The output path is the same as a native
build because the container mounts the host repo at `/work` and a
named volume at `/work/target`, so the `.efi` ends up at:

```
target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi
```

…accessible via the `target` named volume. To surface it for
inspection with `file`:

```
docker run --rm -v uncalibrated-sextant-target:/target alpine \
    sh -c 'apk add --no-cache file >/dev/null && \
           file /target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi'
```

Expected output (last line):

```
    PE32+ executable (EFI application) x86-64, for MS Windows
```

The "for MS Windows" tail is correct — EFI applications share the
PE32+ format with Windows binaries.

If `file` reports anything else (ELF, shared object, plain PE32
without the +64), the build configuration is wrong; debug before
proceeding. Alternatively, Phase 2 will add a `Makefile` target
that copies the built binary out to a host-visible path; for now
the one-liner above is fine.

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
  crate) and the Docker-based build (image, build script, minimal
  Makefile). Keep it short; later phases will expand.
- **`AGENTS.md`** — add a *Build commands* section with
  `make build` as the current build command, note the Docker
  dependency (and that no host Rust toolchain is required), and
  note that `make qemu` does not yet exist (coming in Phase 2).
- **`README.md`** — add a terse *Building* section pointing at
  `make build`, mention Docker as the only build-time dependency,
  and note OVMF/QEMU are not yet required (Phase 2).

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

- [ ] `make build` succeeds on a fresh clone with only Docker
      installed on the host — no host Rust toolchain required.
- [ ] `file` reports `PE32+ executable (EFI application) x86-64`
      on the produced binary.
- [ ] `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`,
      `rust-toolchain.toml`, `.gitignore`, `src/main.rs`,
      `Dockerfile`, `scripts/build.sh`, and `Makefile` are
      committed.
- [ ] `ARCHITECTURE.md`, `AGENTS.md`, and `README.md` reference
      the new build command and the Docker dependency.
- [ ] `pre-commit run --all-files` is **not** expected to pass
      yet — pre-commit is Phase 3 work. Do not add a
      pre-commit config in this phase.

## Bugs fixed during this work

- **Step 1 research reported wrong MSRV for `uefi` 0.37.0.** The
  research agent said MSRV was Rust 1.81; the crate actually
  requires 1.88. Discovered at Step 3 when `cargo build` failed
  immediately during dependency resolution with a clear error
  from cargo. Fixed by bumping `Dockerfile`, `scripts/build.sh`
  tag, and `rust-toolchain.toml` from 1.86.0 to 1.88.0, and
  correcting the version references throughout this plan. Lesson
  for future research steps: cross-check a crate's stated MSRV
  against its `rust-version` field directly rather than trusting
  a docs-page or README summary.
- **`uefi::boot::stall` API changed between uefi versions.** The
  plan sketched `uefi::boot::stall(10_000)` (bare integer
  microseconds), matching older releases. In 0.37 the function
  takes a `core::time::Duration`. Discovered at Step 3 when
  `cargo build` failed with a clear `E0308` mismatched-types
  error pointing at the call site. Fixed by adding
  `use core::time::Duration;` at the top of `src/main.rs` and
  calling `uefi::boot::stall(Duration::from_millis(10))`. The
  plan's Step 2 sketch has been updated to show the correct
  form.

## Future work

None specific to this phase. Items deferred out of the whole
milestone are listed in the master plan.
