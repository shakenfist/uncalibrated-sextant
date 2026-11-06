# Agent guidance for uncalibrated-sextant

## What this repo is

Codename **uncalibrated-sextant**. A SPICE channel exercise harness
delivered as a tiny retrofuturist sci-fi mini-game running as a
`no_std` UEFI Rust binary inside a guest VM. The harness is driven by
[Ryll](../ryll/) over a serial link.

The codename is intentional: pick a true name later, when the content
tells us what it should be.

## Build commands

- `make build` — build the UEFI binary inside Docker. Docker is the
  only build-time dependency; no host Rust toolchain is required.
- `make qemu` — build, assemble a FAT32 ESP image (via Alpine/mtools
  in Docker), and launch an interactive QEMU/OVMF GTK window on the
  host. Requires `qemu-system-x86_64` and `ovmf` on the host.
- `make spice` — build and assemble the ESP image, then launch QEMU
  with a SPICE server on `127.0.0.1:5900` and auto-spawn
  `remote-viewer` to attach; Ctrl-C in the terminal to exit (not the
  remote-viewer window). Requires `virt-viewer` (`sudo apt install
  virt-viewer`) in addition to the `make qemu` host dependencies.
- `make spice-ryll` — build + assemble + launch QEMU with SPICE;
  spawn ryll instead of remote-viewer (with
  `--enable-paste-as-keystrokes` for `Ctrl+Alt+V` paste). Use this
  for any scene that requires clipboard paste delivery, starting with
  the locked-bootloader scene.
- `make release` — build + assemble, then produce
  `dist/uncalibrated-sextant.img` (raw) and
  `dist/uncalibrated-sextant.qcow2` via `qemu-img convert`. Requires
  `qemu-utils` in addition to the `make qemu` dependencies.
- `make release-verify` — runs `make release`, then boots both
  artifacts headless and checks that the serial log contains the
  expected banner within 30 s.
- `make screenshot` — boot the ESP image headless, drive AWAITING
  through the boot sequence via QMP `send-key`, and capture the
  parking-screen frame to `docs/images/boot-sequence.png` via QMP
  `screendump`. Regenerate whenever the visible scene changes.
- `make vendor-probes` — regenerate `src/probes.rs` from
  `scripts/vendor-language-probes.py` after editing the PROBES
  table at the top of that script. Apt prerequisite is
  `fonts-unifont` (one-time install).
- `make clean` — remove the named Docker volume
  (`uncalibrated-sextant-target`) and the local `target/` and `dist/`
  directories.

### Style commands

These run rustfmt and clippy inside the same Docker image used by
`make build`. They are the commands the `rust-check` pre-commit hook
invokes directly.

- `./scripts/check-rust.sh check` — check formatting and clippy
  warnings; exits non-zero on any failure.
- `./scripts/check-rust.sh fix` — auto-apply `cargo fmt` and
  `cargo clippy --fix`; safe to run on a dirty working tree.

## Before pushing

Run [PUSH-AUDIT.md](PUSH-AUDIT.md) over the accumulated diff against
`origin/develop`, not over the last commit. It is a two-wave runbook:
mechanical checks first, then four judgment sub-agents covering code
quality, tests, documentation and security. Every master plan in
`docs/plans/` ends with a phase that runs it, so the usual trigger is
reaching that phase; run it before any other push that is more than a
typo fix.

## Where to read first

1. [README.md](README.md) — project framing
2. [DESIGN.md](DESIGN.md) — channel mapping table, two-channel test
   architecture, design principles, aesthetic direction, open
   questions
3. [ARCHITECTURE.md](ARCHITECTURE.md) — implementation shape
4. `src/renderer/mod.rs` — GOP renderer module of record; owns
   `ScopedProtocol<GraphicsOutput>` and implements per-glyph BitBlt,
   dot-leader telemetry layout, cursor glyph drawing, and logo tiling
5. `src/scene.rs` — scene state machine (Awaiting → Booting → Parked);
   owns the `RingBuffer`, `CursorState`, and the split `BOOT_SCRIPT_PRE`
   / `BOOT_SCRIPT_POST`; calls `bootloader::run` between the two halves
6. `src/bootloader.rs` — locked-bootloader sub-state-machine; plays
   mid-`run_booting` between `SENSORIUM: nominal` and `EMERGENCY SAFE
   BOOT COMPLETE`; owns the R/I/A prompt, retry animation, blob screen,
   paste capture + validation, visible countdown, and both terminal
   paths (cold reset on Abort, ACPI shutdown on timeout)
7. `src/cursor.rs` — `CursorState`: 1 Hz blink, LFSR-driven glitch
   substitution selecting from four broken-glyph variants
8. `src/event.rs` — `RingBuffer<256>` with `Event::Keypress`,
   `Event::LineRendered`, `Event::SceneTransition`, and the
   `BootloaderDecision` / `PasteReceived` / `BootloaderTimeout`
   variants; chronological `iter()` feeds the serial drain. The
   `Event`/`Phase`/`BootloaderChoice` wire-format types are
   re-exported from the shared `shakenfist-visual-digest` crate
   (one source of truth for encoder + future Ryll decoder);
   `RingBuffer<N>` stays Sextant-local
9. `src/serial.rs` — `write_startup_banner` (for release-verify)
   and `drain` (end-of-scene plain-text dump of the ring buffer);
   both share a `with_serial` helper that briefly opens the Serial
   protocol and drops it
10. `src/logo.rs` — 128x128 monochrome Shaken Fist logo bitmap,
    generated by `scripts/vendor-logo.py`
11. `src/probes.rs` — language-probe label and status bitmaps for the
    boot-sequence opening beats, generated by
    `scripts/vendor-language-probes.py` from GNU Unifont
12. `scripts/spice.sh` — SPICE-attached QEMU launch script; read this
    to understand the SPICE infrastructure path (`make spice`), the
    port-wait retry logic, and the Ctrl-C exit convention

## Invariants and gotchas

What the code looks like is in
[ARCHITECTURE.md](ARCHITECTURE.md); this is the part that will bite a
session that has not been told.

**`cargo test` runs zero tests.** The host-testable encoder tests
moved to `shakenfist-visual-digest` along with the encoder. What
stands in for them is `make digest-payload-smoke`, which boots the
real firmware under OVMF, drives the scripted scene through to
parking, decodes the QR off a screendump and asserts the TLV. It is
the load-bearing oracle for any change to the digest, and a green
`cargo test` says nothing about one.

**The wire format lives in another repository.** `Event`, `Phase`,
`BootloaderChoice`, `ChannelHashes` and the TLV encoder come from
[`shakenfist-visual-digest`](https://github.com/shakenfist/visual-digest-rust),
pinned by rev in `Cargo.toml` with `default-features = false` for
`no_std`. `src/event.rs` `pub use`-re-exports them, so
`crate::event::*` imports still work, and keeps `RingBuffer<256>`
local. Changing the format means a change there first, then a rev
bump here; `docs/visual-digest-format.md` is a pointer to that
repository's copy of the spec, not a second copy.

**The digest payload budget is tight and overflowing it panics.** A
QR Version 5 / ECC Low code holds 106 bytes. Eight rolling-hash
records take 48 of them, leaving 44 for raw events -- about three per
frame -- and the encoder deducts the hash block before truncating, so
hashes always survive and events are what get dropped. Feed the
encoder more than the configured ECC level can hold and it panics,
which under the `uefi` crate's panic handler presents as a silent
firmware hang rather than an error: that is what a mislabelled
capacity constant cost once already.

**LOAD-BEARING CARVE-OUT for sub-agents touching scene code.**
`try_handle_mode_key` is not called from `src/bootloader.rs`, and that
must stay true. The locked-bootloader R/I/A and paste-prompt loops own
their own key handling; mode keys received there are
logged-and-ignored. Pressing `'1'` at the
`(R)etry, (I)gnore, or (A)bort?` prompt does not change the display
mode. This is intentional and load-bearing for correct scene
semantics.

**Clipboard paste needs ryll, not remote-viewer.** `make spice-ryll`
against ryll's `display-mode-ui` branch is the canonical acceptance
path for both the display-mode affordance and the locked-bootloader
scene; remote-viewer cannot deliver paste as keystrokes. Mode keys
work in Awaiting, Booting and Parked.

**Regenerate the reference screenshot when the visible scene
changes.** It is committed at `docs/images/boot-sequence.png` and
`make screenshot` rebuilds it. Install the pre-commit hooks
(`pre-commit install`) before your first commit: style enforcement
through `scripts/check-rust.sh` is the gate for all contributions.

## Design principles to respect

These are easy to violate by accident. Re-read them before suggesting
features:

- The game would survive having its test purpose stripped — diegetic
  mechanics, not "ClipboardQuest"
- No fourth-wall breaks; deadpan voice
- Debuggability over fidelity — a broken channel should be visually
  obvious in a single screenshot
- Stay in the genre idiom (atompunk / vault-terminal / Asimov-robot
  flavour) without lifting names, characters, or assets from any
  specific IP

## Sibling projects to be aware of

- [ryll](../ryll/) — the test driver
- [kerbside](../kerbside/) / [kerbside-patches](../kerbside-patches/)
  — the SPICE proxy under test
- [uefi-latency-guest](../uefi-latency-guest/) — the minimal probe
  this project complements (do not merge into it)
- [instar](../instar/) — source of the gRPC-over-serial transport
  pattern intended for this project
