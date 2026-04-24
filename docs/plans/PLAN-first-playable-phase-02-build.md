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
— OVMF firmware paths, QEMU flag names, display-forwarding quirks
in the Kasm/Docker environment, and Docker volume plumbing are
all likely to need iteration. Plan for it.

Cross-repo references, in order of likely usefulness:

- `shakenfist/ryll/Makefile` — the model named in the master plan
  for `make qemu`. Read what it does; don't reinvent.
- `shakenfist/uefi-latency-guest/Makefile` — C-based prior art
  that assembles a GPT-formatted disk with `UEFI-GPT-image-creator`
  and then `qemu-img convert`s to qcow2. Useful reference for the
  `make release` target even though the build language differs.
- OVMF — typically installed as `/usr/share/OVMF/OVMF_CODE.fd` and
  `OVMF_VARS.fd` on Debian/Ubuntu by the `ovmf` package, or
  `edk2-ovmf` on Fedora/Arch. Path may vary.
- `mtools` (`mformat`, `mmd`, `mcopy`) — rootless userspace FAT
  filesystem manipulation without loopback mounts. Preferred over
  `mkfs.vfat` + `mount` for ESP assembly because it avoids needing
  root or privileged containers.
- `qemu-system-x86_64` — target is `-machine q35 -cpu qemu64 -m
  256M -bios /path/to/OVMF_CODE.fd -drive format=raw,file=esp.img`
  at minimum. Display mode to be decided at Step 1.

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

- **Host vs Docker for the QEMU run.** Three shapes are plausible:
  1. **All-Docker** — QEMU, OVMF, and mtools all live inside a
     container; display is forwarded via X11 socket bind-mount,
     VNC port forward, or `-display spice-app`. Most portable,
     matches the Phase 1 "Docker is the only host dep" rule
     cleanly, but display forwarding is the finicky part.
  2. **Docker-build + host-run** — Docker only for cargo and ESP
     assembly; QEMU and OVMF are installed on the host. Simplest
     from a display standpoint. Requires the host to have
     `qemu-system-x86_64`, `ovmf`, and possibly `mtools` (or we
     keep mtools in Docker).
  3. **Two-image Docker** — `Dockerfile` for the build image
     (existing), `Dockerfile.run` for a separate image carrying
     qemu/ovmf/mtools. Same portability as (1) but keeps the
     build image small and layers clean.
  
  The owner's stated preference (Phase 1 amendment) is to avoid
  installing Rust on the host. That principle extends naturally
  to QEMU and OVMF. Shape (3) is therefore the default
  expectation; Step 1 should confirm whether (1) can reasonably
  fold into (3) for display, or whether a split image is needed.

- **Display mode.** Options and tradeoffs:
  - `-display gtk` — native GTK window; needs X socket
    (`/tmp/.X11-unix`) bind-mounted into the container and a
    passthrough `$DISPLAY`. Works well in most Linux desktop
    environments.
  - `-display spice-app` — spawns virt-viewer automatically.
    Thematic for this project but requires virt-viewer present.
  - `-vnc :N` / `-spice port=M` — headless QEMU, user connects
    separately. Robust, works over network, but "run and view"
    becomes two commands.
  - `-nographic` — serial only. Fine for Phase 3+ assertion work;
    not useful for Phase 2's human-verification goal.
  
  Decide at Step 1 based on what the Kasm environment actually
  allows. `-display gtk` with X socket forwarding is the likely
  default; `-vnc` is the reliable fallback.

- **OVMF firmware layout.** Debian's `ovmf` package places code
  at `/usr/share/OVMF/OVMF_CODE.fd` and vars at `OVMF_VARS.fd`.
  The run image can just `apt-get install ovmf` and use those
  paths. Confirm at Step 1.

- **ESP image layout.** A plain FAT32 image with one file at
  `/EFI/BOOT/BOOTX64.EFI` is the simplest thing that boots. GPT
  partitioning (as in `uefi-latency-guest`) is more realistic
  but optional — OVMF's fallback bootloader will happily boot
  from a FAT image passed directly as a `-drive`. Default:
  plain FAT image for `make qemu`; GPT-wrapped raw for
  `make release` if it turns out to matter for portability.
  Decide at Step 2.

- **Release artifact formats.** Baseline is raw `.img`; adding
  `.qcow2` is one `qemu-img convert` away and near-free. VHD
  and VMDK are stretch. Plan for raw + qcow2 at Step 4.

- **Artifact host-visibility.** Release artifacts go into `dist/`
  at the repo root (host-visible path), added to `.gitignore`.
  Named Docker volumes (cargo target, optionally an ESP
  scratch volume) remain invisible to the host; that's fine for
  build intermediate state.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | medium | opus   | none      | Research: confirm Kasm environment capabilities for display forwarding, find the right OVMF package/paths, pick the host/Docker split shape, pick display mode. See Step 1 below. |
| 2    | medium | sonnet | none      | Implement ESP assembly + QEMU launch. Create `scripts/mkesp.sh`, `scripts/qemu.sh`, possibly `Dockerfile.run`, and extend `Makefile` with a `qemu` target. See Step 2 below. |
| 3    | low    | sonnet | none      | Run `make qemu`, verify the banner is visible in the QEMU window, verify keypress exits cleanly. Capture the firmware log. See Step 3 below. |
| 4    | medium | sonnet | none      | Add `make release` target producing raw + qcow2 artifacts in `dist/`. Verify each independently boots. See Step 4 below. |
| 5    | low    | sonnet | none      | Update `README.md`, `ARCHITECTURE.md`, `AGENTS.md` to reflect the new commands, dependencies, and artifact layout. See Step 5 below. |

### Step 1 — research and decide

**Check:**

- Is `qemu-system-x86_64` installed on the host? Is `ovmf`? Is
  `mformat` (mtools)? Run `which qemu-system-x86_64 ovmf mformat`
  on the host and record.
- What display does QEMU have available inside this Kasm
  environment? Specifically: does `/tmp/.X11-unix` exist and is
  it writable? Is `$DISPLAY` set? Is Wayland involved?
- Does `shakenfist/ryll/Makefile` have a `qemu` target, and if
  so what does it do? Read it; don't reinvent if it already
  solved this.
- What OVMF paths does the Debian `ovmf` package install? `dpkg
  -L ovmf` on any Debian system with the package, or check
  Debian's package tracker.
- Are there Kasm-specific constraints on bind-mounting
  `/tmp/.X11-unix`, running privileged containers, or using KVM
  inside a container? (KVM acceleration is a nice-to-have for
  QEMU; without it we fall back to software emulation which is
  slow but works.)

**Decide:**

- Shape — all-Docker / Docker-build + host-run / two-image
  Docker — based on what's available and what respects the
  "don't install on host" preference.
- Display mode — `-display gtk`, `-vnc`, `-display spice-app`,
  or other.
- OVMF strategy — package install in the run image, or
  bundled binaries, or other.

**Output:** a short paragraph to the management session
recording the decisions and anything surprising. No file
changes in this step.

### Step 2 — implement ESP assembly and QEMU launcher

Create:

- **`scripts/mkesp.sh`** — rootless ESP image assembly via
  `mtools`. Creates an empty FAT image (`dd if=/dev/zero`),
  formats it (`mformat -i esp.img -v ESP ::`), creates the
  directories (`mmd -i esp.img ::/EFI ::/EFI/BOOT`), and copies
  the `.efi` as `BOOTX64.EFI` (`mcopy -i esp.img
  /path/to/uncalibrated-sextant.efi ::/EFI/BOOT/BOOTX64.EFI`).
  Lives in a Docker container (either the build image extended
  with mtools, or the run image).
- **`scripts/qemu.sh`** — wraps `qemu-system-x86_64` with the
  decided flags. Takes the ESP image path as a positional arg
  for reuse by `make release` verification.
- **`Dockerfile.run`** (or single Dockerfile with a run stage,
  per Step 1 decision) — minimal Debian-based image with
  `qemu-system-x86_64`, `ovmf`, `mtools`, and whatever display
  plumbing Step 1 settled on.
- **`Makefile`** — add:
  - `qemu` target that depends on `build`, calls `mkesp.sh`,
    then calls `qemu.sh`.
  - Extend `clean` to remove the ESP image and any additional
    named volumes.

Important: Makefile recipes use tabs. Any shell loops or
conditionals live in `scripts/` not in Makefile recipes (per
the project convention in CLAUDE.md: "Do not write large
scripts in CI workflow steps. Write them to a shell script in
tools/ and then call them from there." — same principle applies
to Makefile recipes).

### Step 3 — verify `make qemu`

Run `make qemu`. Expect:

1. A QEMU window (or VNC/SPICE session, depending on display
   mode) opens.
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
  *Running* section alongside it) with `make qemu`. Note the
  Docker dependency is now "Docker plus a display the run
  container can use" (X11, VNC, or SPICE per Step 1). Mention
  `make release` and the `dist/` output.
- **`AGENTS.md`** — extend *Build commands* with `make qemu`
  and `make release`. Note the display requirements for the
  `qemu` target.
- **`ARCHITECTURE.md`** — describe the new scripts and the ESP
  assembly / QEMU launch flow. Keep it short; Phase 4 will
  expand the architecture sections more substantially.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. Two
phase-specific emphases:

- **The visual verification in Step 3 is load-bearing.** Do not
  mark Phase 2 complete without a human (or the management
  session, acting on sub-agent description) confirming the
  banner was visible in the QEMU window. An exit-zero `make
  qemu` that never reached `BOOTX64.EFI` is the phase's most
  likely failure mode.
- **Docker image size matters modestly.** The run image will be
  noticeably bigger than the build image because OVMF and QEMU
  are large. That is fine, but do not install unnecessary
  packages. Strip down after install where practical
  (`apt-get clean`, `rm -rf /var/lib/apt/lists/*`).

## Success criteria

Phase 2 is complete when:

- [ ] `make qemu` on a fresh clone (with only Docker on the
      host, or Docker plus whatever display plumbing Step 1
      decided) opens a window where the Phase 1 banner is
      visible and a keypress exits cleanly.
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
