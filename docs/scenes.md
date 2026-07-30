# Running the scenes

How to launch Uncalibrated Sextant against a SPICE client, and what
each scene does. See
[DESIGN.md](https://github.com/shakenfist/uncalibrated-sextant/blob/main/DESIGN.md)
for the channel mapping and two-channel test architecture.

## Running under SPICE

`make spice` builds the binary, launches QEMU with a SPICE server on
`127.0.0.1:5900`, and auto-spawns `remote-viewer` to attach. This
confirms the SPICE Display and Inputs channels work end-to-end against
the binary; it is the required launch path for any scene that exercises
a SPICE channel rather than QEMU's bare GTK display.

If `remote-viewer` is not installed, the script exits with a clear
hint — install it with `sudo apt install virt-viewer`.

**Exit gesture: Ctrl-C in the terminal.** There is no QEMU-owned
window in this configuration — closing the `remote-viewer` window does
not stop QEMU. Always exit via Ctrl-C in the terminal that launched
`make spice`; the script's trap will kill both QEMU and `remote-viewer`
cleanly. This is the opposite of the instinct from `make qemu`.

If port 5900 is already in use, override with
`SPICE_PORT=5901 make spice`.

Host dependencies for `make qemu` and `make release`:
`qemu-system-x86_64`, `ovmf`, and `qemu-utils` (for `qemu-img`).
Docker remains the only dependency for `make build` alone.

## The scene state machine

The binary runs the full scene state machine — a wordless lone-cursor
"awaiting" screen, a scripted boot sequence with the locked-bootloader
sub-scene, and a SYSTEM ONLINE parking screen — with blinking cursor,
LFSR-driven glitch substitution, and the Shaken Fist logo rendered as a
tiled 8x16 glyph grid in the top-right corner. The opening beats probe
for Mandarin / Hindi / Spanish / English language support (the three
non-English probes report failure in their own scripts, English OK),
establishing that English is no longer the default in the fictional
universe. The bottom-right of the framebuffer carries a QR code
(schema v2) that encodes eight per-channel rolling CRC32C hashes (one
per event variant, accumulated since boot) followed by the most-recent
ring-buffer raw events, plus a CRC32C of the rest of the screen; an
external decoder (e.g. ryll) can read this from a screenshot to
validate the display path and confirm every event was received,
independently of serial. On final shutdown, the event ring buffer is
drained to the UEFI Serial protocol as plain text, groundwork for the
eventual gRPC-over-serial transport. See
[visual-digest-format.md](visual-digest-format.md) for the QR wire
format.

## Locked-bootloader scene

The locked-bootloader scene plays mid-boot, between the
`SENSORIUM: nominal` telemetry line and `EMERGENCY SAFE BOOT
COMPLETE`. It is the first scene that exercises SPICE clipboard
paste as a real channel test.

**Operator UX.** The scene opens with two telemetry lines
establishing a diegetic failure (`Advanced b64 cryptographic
coprocessor: OFFLINE` and `NIST 800-53 SC-28(1) Secret hardening:
DISABLED BY CONFIGURATION`), then presents a three-option prompt:

```
Decryption of next-stage bootloader failed. (R)etry, (I)gnore, or (A)bort?
```

- **(R)etry** — plays an animated `Retrying decryption........`
  leader (eight dots, 200 ms each), then re-renders the prompt
  in place with an attempt counter `(attempt N)`. After five
  retries a sticky note appears: `Continued retry will not change
  the outcome.`
- **(I)gnore** — advances to the blob screen.
- **(A)bort** — cold-resets the VM; the run replays from firmware.

**Blob screen.** Selecting Ignore shows the encoded payload:

```
c2V4dGFudHtIRUxMT19PUEVSQVRPUn0=
```

Decode it externally (`echo c2V4dGFudHtIRUxMT19PUEVSQVRPUn0= | base64 -d`
gives `sextant{HELLO_OPERATOR}`) and paste the decoded value back
to continue boot.

**Canonical client: ryll, not remote-viewer.** `remote-viewer`
cannot deliver a clipboard paste as Inputs-channel keystrokes
when no guest-side vdagent is present. Use `make spice-ryll`,
which spawns ryll with `--enable-paste-as-keystrokes`.

**Paste shortcut: `Ctrl+Alt+V`.** This is ryll's paste-as-keystrokes
shortcut. Do **not** use `Ctrl+Shift+V` — that is the obvious
first guess (matching most terminal emulators) but it is not what
ryll binds, and the keystrokes will arrive at the guest as literal
Ctrl+Shift+V rather than triggering paste. The *Menu → Paste*
option in ryll's UI always works and avoids the keyboard-mapping
question entirely (useful on Mac keyboards where Option/Alt mapping
depends on the keymap layer).

**Four flow paths:**

- **Correct paste** — `Booting...` appears, boot continues through
  `EMERGENCY SAFE BOOT COMPLETE` to the parking screen.
- **Wrong paste** — the input line re-renders in place with
  `(wrong, attempt N of 3)`; after three wrong pastes the
  visible countdown begins immediately.
- **Abort** — cold reset; the VM restarts and the scene replays
  from the beginning.
- **Timeout** — after 60 s of silence at the paste prompt a
  visible countdown appears (`Awaiting decoded payload. Aborting
  in NN...`, counting from 30 to 00 at 1 Hz), then
  `BOOTLOADER UNRECOVERABLE. SHUTTING DOWN.`, then ACPI shutdown.

## Display-mode keystrokes

The following keys switch the GOP framebuffer to a specific resolution
at any time — from the awaiting screen, during booting, or from the
parked screen:

| Key | Resolution  |
|-----|-------------|
| `1` | 640×480     |
| `2` | 800×600     |
| `3` | 1024×768    |
| `4` | 1280×720    |
| `5` | 1280×1024   |
| `6` | 1920×1080   |
| `0` | Cycle through every available mode, ~1 s per mode (interruptible) |

After each switch a brief toast appears on the bottom row naming the
applied resolution (e.g. `mode 1024x768`). If the firmware does not
expose the exact requested mode the nearest available mode is used
instead and the toast shows the substitution form (`requested 1280x720
-> using 1024x768`). The renderer's font is ASCII-only, so the toast
renders exactly as shown — no Unicode `×` or `→`. Under default OVMF +
QEMU all six bindings resolve exactly
— no substitutions are needed — but a future host or `-vga` variant
may differ.

Key `0` walks every mode the firmware exposes with a one-second dwell
per step. Pressing any key during the walk stops the cycle; if the
interrupting key is itself a mode key (`1`–`6`), the walk stops *and*
that resolution is applied.

**Bootloader carve-out.** Mode keys are silently ignored inside the
locked-bootloader R/I/A prompt and paste-prompt loops; those scenes own
their own key handling. Pressing `1` at `(R)etry, (I)gnore, or (A)bort?`
logs the keypress and does nothing — the mode does not change. Mode
keys resume their normal meaning once the bootloader scene completes.

**Acceptance test path.** `make spice-ryll` against ryll's
`display-mode-ui` branch is the canonical test for ryll window-tracking
behaviour. With ryll's *Obey guest size hints* hamburger toggle on (the
default), ryll's window refits to each new resolution. With the toggle
off, ryll's window stays pinned while the binary's scene resizes inside
it — allowing edge-case testing of toggle-off → mode-change → toggle-on
round trips and maximised-window behaviour.
