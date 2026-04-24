# First playable — phase 2: Build tooling and `make qemu`

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read `DESIGN.md` (especially
*First milestone scope*), the master plan, and the Phase 1 plan
([PLAN-first-playable-phase-01-skeleton.md](PLAN-first-playable-phase-01-skeleton.md))
to understand the existing build infrastructure. Phase 1 landed
a Docker-based cargo build that produces a valid `.efi`; Phase 2
extends that infrastructure to produce a bootable image, launch
QEMU against it, and produce distributable artifacts.

This phase is about **packaging and execution**: no Rust code
changes are in scope. The Phase 1 `src/main.rs` (banner + keypress
loop) is the payload, and the goal is to prove the boot path end
to end. Phase 4 onward will iterate on the rendered content; this
phase iterates on nothing visible except "does it boot".

The phase-1 plan's "Bugs fixed during this work" section records
two surprises that happened despite careful Step 1 research
(wrong MSRV, changed `stall` API). Expect similar surprises here
— OVMF flag incantations, QEMU version differences, Docker volume
plumbing, and exactly how OVMF VARS are stored are all plausible
sources of friction. Plan for it.

**Operating environment reminder.** This workspace runs on a
mutable Linux host with a real X session (`qemu-system-x86_64`
already installed and in routine use by ryll). GUI tools launched
from this shell open windows naturally in the desktop session.
Rust builds are wrapped in Docker to keep the host clean of
Rust toolchains, but other system packages live on the host
normally. Do not attempt elaborate display forwarding or
docker-in-docker dances. See `feedback_host_environment.md` in
memory.

Cross-repo references, in order of likely usefulness:

- `shakenfist/ryll/Makefile` — the canonical reference for
  running QEMU in this environment. Read what it does; mirror
  its flag pattern; don't reinvent.
- `shakenfist/uefi-latency-guest/Makefile` — C-based prior art
  that assembles a GPT-formatted disk with `UEFI-GPT-image-creator`
  and then `qemu-img convert`s to qcow2. Useful reference for the
  `make release` target even though the build language differs.
- OVMF — installed on this host via the Debian `ovmf` package,
  firmware files at `/usr/share/OVMF/OVMF_CODE_4M.fd` and
  `OVMF_VARS_4M.fd`. Confirm at Step 1.
- `mtools` (`mformat`, `mmd`, `mcopy`) — rootless userspace FAT
  filesystem manipulation without loopback mounts. Used inside
  a disposable Docker step so host doesn't need the package.

All planning documents go in `docs/plans/`.

## Situation

Phase 1 landed at commits `7712239` (scaffolding) and `8a0442d`
(docs). The repo now has:

- `Cargo.toml` pinning `uefi = "=0.37.0"`, `Cargo.lock`,
  `.cargo/config.toml`, `rust-toolchain.toml` (1.88.0),
  `.gitignore`
- `Dockerfile` (`rust:1.88-slim` base with UEFI target
  preinstalled) and `scripts/build.sh` (bind-mount host source,
  named volume `uncalibrated-sextant-target` for cargo output)
- Minimal `Makefile` with `build` and `clean` targets
- `src/main.rs` using the new uefi 0.37 globals-based API —
  clears screen, prints banner, waits for keypress
- Up-to-date `README.md`, `AGENTS.md`, `ARCHITECTURE.md`

The working artifact — a valid PE32+ EFI application — lives in
a Docker named volume. It has never been run. Nothing on the
host can see it without a Docker intermediary. There is no way
to exercise the binary in QEMU, and no artifact that could be
distributed or uploaded to a VM host.

## Mission and problem statement

Produce a `make qemu` target that builds the UEFI binary,
assembles it into a FAT-formatted ESP image with
`/EFI/BOOT/BOOTX64.EFI`, and launches QEMU with OVMF firmware so
that a human watching the QEMU window sees the Phase 1 banner
and can exit the program with a keypress.

Produce a `make release` target that emits at least a raw `.img`
and a `.qcow2` — both bootable under QEMU — to a host-visible
`dist/` directory.

Update `make clean` to tear down the new state (ESP image,
release artifacts, any additional Docker volumes).

After this phase, the cycle time for iterating on the UEFI
payload is `edit src/main.rs; make qemu`. That cycle time is what
phases 4 and 5 are going to rely on, so it needs to be tight.

## Open questions

Most of the shape questions the original draft of this plan
left open were resolved by clarifying the environment: the
workspace runs on a mutable Linux host (named Kasm) with a
real X session, QEMU is already installed and in routine use
(that is how ryll's `make qemu` works today), and GUI apps
launched from this shell open windows naturally in the desktop
session. No X-forwarding gymnastics or container display
plumbing is needed. See `feedback_host_environment.md` in
memory for context.

The resolved shape for Phase 2 is therefore:

- **Rust build and ESP assembly in Docker.** Rust stays in the
  Phase 1 Docker image. ESP image assembly runs in a tiny
  disposable Docker step (Alpine + `mtools`) so the host
  doesn't need `mtools`.
- **QEMU run on host.** Invoke `qemu-system-x86_64` directly
  from `scripts/qemu.sh`; the resulting window is a GTK window
  in the operator's X session, just like ryll.
- **`-display gtk`** is the default, matching ryll's precedent.
- **OVMF from the host Debian `ovmf` package**, typically at
  `/usr/share/OVMF/OVMF_CODE_4M.fd` and `OVMF_VARS_4M.fd`.

Remaining open questions, to be resolved at the step noted:

- **ESP image layout.** A plain FAT32 image with one file at
  `/EFI/BOOT/BOOTX64.EFI` is the simplest thing that boots.
  GPT partitioning (as in `uefi-latency-guest`) is more
  realistic for distribution but optional for local
  iteration — OVMF's fallback bootloader will happily boot
  from a FAT image passed directly as a `-drive`. Default
  plan: plain FAT for `make qemu`; GPT-wrapped raw for
  `make release` only if portability testing turns out to
  require it. Decide at Step 2.
- **Release artifact formats.** Baseline is raw `.img`;
  `.qcow2` is one `qemu-img convert` away. VHD and VMDK are
  stretch. Plan for raw + qcow2 at Step 4.
- **Where release artifacts land.** Default `dist/` at the
  repo root (host-visible, `.gitignore`d). Settle at Step 4.
KVM acceleration is **always on** for this project — `/dev/kvm`
is available on the host and the first-playable milestone's
later phases (and subsequent SPICE-performance-measurement
work) depend on realistic timing, not software-emulation
timing. `scripts/qemu.sh` passes `-enable-kvm` unconditionally.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | low    | sonnet | none      | Confirm host has `qemu-system-x86_64` and `ovmf`, record their paths, and note whether `/dev/kvm` is accessible. Read ryll's Makefile for reference patterns. Report back. See Step 1 below. |
| 2    | medium | sonnet | none      | Implement ESP assembly + QEMU launch. Create `scripts/mkesp.sh` (Docker + mtools, rootless), `scripts/qemu.sh` (host invocation), and extend `Makefile` with a `qemu` target. See Step 2 below. |
| 3    | low    | sonnet | none      | Run `make qemu`, verify the banner is visible in the QEMU window, verify keypress exits cleanly. Capture the firmware log. See Step 3 below. |
| 4    | medium | sonnet | none      | Add `make release` target producing raw + qcow2 artifacts in `dist/`. Verify each independently boots. See Step 4 below. |
| 5    | low    | sonnet | none      | Update `README.md`, `ARCHITECTURE.md`, `AGENTS.md` to reflect the new commands, dependencies, and artifact layout. See Step 5 below. |

### Step 1 — confirm host tooling and record paths

The shape questions are settled (see *Open questions*), so this
step is now a short confirmation pass rather than an
open-ended research exercise. Effort downgraded from medium/opus
to low/sonnet.

**Confirm on the host (not in a container):**

- `which qemu-system-x86_64` — expected to return a path. If
  missing, stop and report; `apt-get install qemu-system-x86`
  is the user's preferred remedy but confirm before installing.
- `ls /usr/share/OVMF/` — expected to contain at least
  `OVMF_CODE_4M.fd` and `OVMF_VARS_4M.fd` from the Debian `ovmf`
  package. If missing, stop and report.
- `cat /srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/Makefile`
  — read ryll's `qemu` target (if present) to mirror its
  QEMU flag patterns. Do not reinvent; this is the project's
  canonical reference for running QEMU in this environment.

**Output:** a concise note back to the management session
recording (a) the two confirmed paths (`qemu-system-x86_64`,
OVMF firmware) and (b) any relevant flags lifted from ryll's
Makefile. No file changes in this step.

### Step 2 — implement ESP assembly and QEMU launcher

Create:

- **`scripts/mkesp.sh`** — rootless ESP image assembly via
  `mtools`, running inside a disposable Docker container so the
  host doesn't need `mtools` installed. The script should:
  1. Pull (or rely on cached) a small Alpine or Debian-slim
     image with `mtools` and `mkfs.vfat` available (for
     example, `alpine` with `apk add --no-cache mtools
     dosfstools`, or a purpose-built Dockerfile fragment; the
     existing `rust:1.88-slim` build image does not have
     mtools by default).
  2. Inside the container, bind-mount the host `dist/`
     directory (creating it if necessary) and the Docker
     cargo-target named volume from Phase 1 read-only, create
     an empty ~33 MiB FAT image (`dd if=/dev/zero of=esp.img
     bs=1M count=33`), format it (`mformat -i esp.img -v ESP
     ::`), create the directory tree (`mmd -i esp.img ::/EFI
     ::/EFI/BOOT`), and copy the `.efi` in as `BOOTX64.EFI`
     (`mcopy -i esp.img
     /path/to/uncalibrated-sextant.efi ::/EFI/BOOT/BOOTX64.EFI`).
  3. The resulting `dist/esp.img` is host-visible for `qemu.sh`
     to read directly.
- **`scripts/qemu.sh`** — wraps `qemu-system-x86_64`, invoked
  **on the host** (no Docker wrapper). Mirrors ryll's flag
  pattern where sensible. Minimum flag set:
  - `-machine q35 -cpu qemu64 -m 256M`
  - `-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd`
  - a writable copy of `OVMF_VARS_4M.fd` (`qemu-img create` or
    straight `cp` into `dist/`; `-drive if=pflash,format=raw,file=dist/OVMF_VARS_4M.fd`)
  - `-drive format=raw,file=dist/esp.img`
  - `-display gtk`
  - `-serial file:dist/serial.log` — captures firmware chatter
    for the Step 3 review and as baseline material for Phase 3's
    CI work.
  - `-enable-kvm` unconditionally. KVM is always available on
    this host and SPICE performance measurement in later phases
    depends on realistic timing.

  Takes the ESP image path as a positional argument for reuse
  by `make release`'s verification pass.
- **`Makefile`** — add:
  - `qemu` target that depends on `build`, calls `mkesp.sh` to
    produce `dist/esp.img`, then calls `qemu.sh dist/esp.img`.
  - Extend `clean` to remove `dist/`.
  - Keep `build` and existing `clean` behaviour intact.

No `Dockerfile.run` is needed. QEMU runs on the host; only
`mkesp.sh`'s ephemeral container touches Docker.

Important: Makefile recipes use tabs. Any shell loops or
conditionals live in `scripts/` not in Makefile recipes (per
the project convention in CLAUDE.md: "Do not write large
scripts in CI workflow steps. Write them to a shell script in
tools/ and then call them from there." — same principle applies
to Makefile recipes).

### Step 3 — verify `make qemu`

Run `make qemu`. Expect:

1. A QEMU GTK window opens in the operator's X session.
2. OVMF's boot splash appears briefly, then hands off to
   `BOOTX64.EFI`.
3. The Phase 1 banner (`Hello from Uncalibrated Sextant`) is
   visible on screen, cleared of OVMF chatter.
4. A keypress returns `EFI_SUCCESS` and the UEFI binary
   exits. QEMU may then return to OVMF's shell or reboot,
   depending on firmware behaviour — either is fine for this
   phase.

**Do not skip the visual check.** A misconfigured `-drive` or
`-bios` can produce a QEMU session that boots OVMF but never
transfers to `BOOTX64.EFI`. `make qemu` exits 0 in that case
because QEMU exited 0, but the phase goal is not met. The
management session must confirm the banner was actually
visible.

Capture the firmware serial log (via `-serial file:...`) and
attach or summarise it in the Step 3 report even on success
— useful as a baseline for Phase 3's CI work.

### Step 4 — `make release`

Extend the Makefile with a `release` target that:

1. Depends on `build`.
2. Runs `scripts/mkesp.sh` (or a sibling `scripts/mkrelease.sh`
   if the release image needs GPT partitioning the `qemu` image
   doesn't — decide based on Step 2's choice).
3. Copies the ESP image out to `dist/uncalibrated-sextant.img`
   (host-visible).
4. Runs `qemu-img convert -f raw -O qcow2` to produce
   `dist/uncalibrated-sextant.qcow2`.
5. Reports the file sizes and any verification hashes.

**Verify:** both artifacts boot independently under QEMU. A
short `scripts/verify-release.sh` (or an extra `make` target
like `make release-verify`) that launches QEMU against each
artifact and waits for a keypress is worth having — even if
only run by hand.

Add `/dist` to `.gitignore`.

### Step 5 — documentation

Update:

- **`README.md`** — extend the *Building* section (or add a
  *Running* section alongside it) with `make qemu`. Note that
  host dependencies are now Docker + `qemu-system-x86_64` +
  `ovmf` (the latter two via the distro's `qemu-system-x86`
  and `ovmf` Debian packages). Mention `make release` and the
  `dist/` output.
- **`AGENTS.md`** — extend *Build commands* with `make qemu`
  and `make release`. Note the host dependencies (Docker,
  qemu-system-x86_64, ovmf).
- **`ARCHITECTURE.md`** — describe the new scripts and the ESP
  assembly / QEMU launch flow. Keep it short; Phase 4 will
  expand the architecture sections more substantially.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. Two
phase-specific emphases:

- **The visual verification in Step 3 is load-bearing.** Do not
  mark Phase 2 complete without a human (or the management
  session, acting on a sub-agent's description) confirming the
  banner was visible in the QEMU window. An exit-zero `make
  qemu` that never reached `BOOTX64.EFI` is the phase's most
  likely failure mode.
- **Mirror ryll where possible.** `shakenfist/ryll/Makefile` is
  the canonical reference for running QEMU on this host. Read
  it first and lift its flag patterns before inventing new ones.
  Differences should be justified, not incidental.

## Success criteria

Phase 2 is complete when:

- [ ] `make qemu` on a fresh clone (with host dependencies
      Docker, `qemu-system-x86_64`, and `ovmf` installed) opens
      a GTK window where the Phase 1 banner is visible and a
      keypress exits cleanly.
- [ ] `make release` produces `dist/uncalibrated-sextant.img`
      (raw) and `dist/uncalibrated-sextant.qcow2`, both of
      which individually boot under QEMU to the same banner.
- [ ] `make clean` removes `dist/`, any ESP image, any new
      named Docker volumes, and the host `target/` dir.
- [ ] `README.md`, `ARCHITECTURE.md`, `AGENTS.md` describe the
      new commands and their dependencies.
- [ ] `pre-commit run --all-files` is **not** expected to pass
      yet — pre-commit is Phase 3 work. Do not add a
      pre-commit config in this phase.

## Bugs fixed during this work

(None yet — populated during execution.)

## Future work

- Automated screenshot verification (Phase 6). This phase
  relies on human eyes for the banner check.
- Additional release artifact formats (VHD, VMDK) beyond raw
  and qcow2.
- KVM acceleration support if not enabled by default in the
  chosen Docker run environment. Software emulation works for
  Phase 2 but will be slow for phases with any animation.
- QMP monitor integration for Phase 5+ scripted interaction
  (separate from Ryll's serial-based control path).
- `make qemu-headless` variant using `-nographic` for
  ssh/tmux workflows — nice to have, not required.
