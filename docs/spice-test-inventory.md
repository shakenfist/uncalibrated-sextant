# SPICE channel test inventory

Working catalogue of SPICE-protocol operations the harness should
exercise, classified by whether they can play out as set-dressing
(the binary performs them on its own and the player observes) or
require interactive participation (the player must do something for
the test to make sense). The third column captures a tentative
metaphorical role for each operation — the in-fiction meaning of
the test, which is what determines whether and where it earns
screen time in the game. The fourth column tracks implementation
state and links to the plan that landed the work.

This document is **deliberately a starter, not a specification.**
Ryll is going to grow new tests over time, and we want a single
place to land them so they accumulate vocabulary rather than feeling
wedged into existing scenes. When a new test arrives:

1. Add a row to the relevant channel table (or open a new one).
2. Decide its mode and pencil in a metaphor — even a rough one.
3. Leave the status column as `—` until a plan exists.
4. *Don't* immediately schedule it into a scene. The metaphor
   column is a vocabulary; the scene order is a separate design
   problem.

## How to read the columns

- **Operation** — the actual thing being exercised. Imagine the
  Ryll-side assertion you would write.
- **Mode** — `set` (binary performs unilaterally; player observes),
  `play` (player must initiate or respond), or `both` (occurs in
  set-dressing form and gains a deeper interactive form later, or
  vice versa).
- **Metaphorical role** — the in-fiction meaning. Compact and
  evocative beats accurate; a bad metaphor can be replaced, an
  absent metaphor can't earn screen time. `?` means worth filling
  in but not obvious yet.
- **Status** — `—` for unimplemented; otherwise a link to the plan
  that did the work, optionally prefixed with `binary:` when the
  binary side exists but Ryll does not yet drive it as a real
  assertion. The first-playable milestone laid down a few
  binary-side capabilities that future Ryll plans will turn into
  real tests; those rows carry a `binary:` link.

Not every row needs to land on screen. Some operations belong to
the test harness only — Ryll asserts them, the player never sees
them. The metaphor column is the discriminator: if a metaphor is
strong, the operation is a candidate for gameplay; if it is thin
or absent, leave it as a silent assertion.

## Connection lifecycle (Main channel)

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Initial connection established | set | First contact — someone is watching | — |
| Capability negotiation | set | Each side admitting what it can and cannot do | — |
| Authentication ticket accepted | set | The operator presenting credentials | — |
| Multiple-monitor capability advertised | set | The instrument acknowledging additional eyes | — |
| Server keepalive received | set | Steady pulse from the other end | — |
| Graceful disconnect | both | The operator stepping away | — |
| Forced disconnect (server kill) | set | Connection severed without warning — abandonment | — |
| Reconnect after disconnect | both | The operator returns; what does the instrument remember? | — |
| Channel renegotiation mid-session | set | Quiet recalibration the operator may not notice | — |

## Display channel

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Mode set: 640x480, 800x600, 1024x768, etc. | set | Tuning the lens / adjusting perception | binary: [phase 4](plans/PLAN-first-playable-phase-04-renderer.md) |
| Mode walk across all offered modes | both | The instrument testing what it can perceive | — |
| Bit-depth changes (8 / 16 / 32) | set | Colour vision dimming and brightening | — |
| Image compression negotiated (LZ / GLZ / quic) | set | How frankly the instrument transmits what it sees | — |
| MJPEG / H.264 video stream open | set | Sustained moving imagery — something is in motion | — |
| Per-monitor mode (multi-head) | set | Peripheral vision activating | — |
| Hot-add monitor | both | A new eye opening | — |
| Hot-remove monitor | both | Going partially blind | — |
| Bandwidth degradation under throttle | set | Signal weakening — fog, distance, interference | — |
| Pixel-accurate rendering verification | set | How truthful the instrument's reports are | — |
| Cursor over moving stream content | set | Tracking a moving target — coordination test | — |
| Damaged / partial frame recovery | set | The instrument re-orienting after losing its place | — |

## Cursor channel

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Cursor shape change (app-driven) | set | The operator's tool changing — their intent shifting | binary: [phase 5](plans/PLAN-first-playable-phase-05-boot-sequence.md) |
| Cursor hot-spot accuracy | play | How precisely the operator can indicate | — |
| Cursor hide / show | set | The operator approaching / stepping back | binary: [phase 5](plans/PLAN-first-playable-phase-05-boot-sequence.md) |
| Custom application cursor (game-defined) | set | The operator pointing at something specific | binary: [phase 5](plans/PLAN-first-playable-phase-05-boot-sequence.md) |
| Cursor over hotspot zone (e.g. door, object) | play | Recognition — *I see what you're looking at* | — |
| Animated cursor frames | set | Liveness / impatience | binary: [phase 5](plans/PLAN-first-playable-phase-05-boot-sequence.md) |
| Cursor at edge / off-screen behaviour | set | The operator's attention drifting away | — |

## Inputs — keyboard

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Single key press / release | play | Spoken commands; smallest possible utterance | binary: [phase 5](plans/PLAN-first-playable-phase-05-boot-sequence.md) |
| Modifier keys (Shift / Ctrl / Alt) | play | Tone, emphasis, register | — |
| Function keys / arrows / page navigation | play | Ritual gestures — non-verbal commands | — |
| Key repeat under hold | play | Insistence / impatience | — |
| Unicode / IME input | play | Languages the instrument doesn't natively speak | — |
| Specific keystroke sequences (cheat codes / passphrases) | play | Hidden knowledge being entered — the operator has been somewhere else | — |
| Keyboard layout change mid-session | set | The operator switching language / context | — |

## Inputs — pointer

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Pointer movement | play | The operator's attention shifting | — |
| Left button click | play | A decision being made | — |
| Right button click | play | Asking for context — *what is this?* | — |
| Middle button / scroll | play | Browsing, reviewing | — |
| Drag from A to B | play | Moving something with intent | — |
| Pointer enters known region | play | Curiosity / approach | — |
| Pointer leaves known region | play | Withdrawing attention | — |
| Absolute vs relative mode | set | Whether the operator sees the same screen or steers blind | — |
| Multi-button / side buttons | play | Specialised tools — the operator is well equipped | — |

## Audio playback (server → client)

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Single tone / chime | set | Status pulse — the instrument's heartbeat | — |
| Continuous ambient hum | set | Room tone — *the instrument is alive* | — |
| Speech-like waveform | set | An internal voice — narration leaking audibly | — |
| Sample rate negotiation | set | The instrument finding its register | — |
| Stereo / multi-channel | set | Direction / spatial awareness | — |
| Sync with on-screen event (lip-sync / hit-stinger) | set | Sound matching action — coherence test | — |
| Volume change mid-stream | set | Volume of inner voice rising or falling | — |

## Audio record (client → server, microphone)

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Microphone permission requested | play | The instrument asking to listen | — |
| Mic open, ambient capture | play | *Being listened to* — possibly the strongest single beat in the game | — |
| Voice-level detection | play | The operator speaking back — what does the instrument hear? | — |
| Mic mute / unmute | play | Privacy boundaries being drawn | — |

## Clipboard (vdagent / port channel)

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Text clipboard server → client (instrument copies) | set | The instrument hands the operator a payload only the outside world can read | — |
| Text clipboard client → server (operator pastes in) | play | A message dropped in from outside — content the instrument did not generate | — |
| Image clipboard | both | A picture being shown / received | — |
| Large clipboard payload | play | The operator depositing something substantial | — |
| Clipboard ownership negotiation | set | Quiet protocol about who is currently speaking | — |
| Multi-format clipboard item | set | A message offered in several forms — *take it however you can read it* | — |

**Note on transport.** Real SPICE clipboard requires
`spice-vdagent` on the guest, which talks to the host over a
virtio-serial port. UEFI has neither, and writing a
virtio-serial driver plus vdagent protocol implementation in
`no_std` is a milestone of its own (see
[PLAN-locked-bootloader.md](plans/PLAN-locked-bootloader.md)
*Future work*). In the meantime the two halves split
asymmetrically:

- **Client → server.** `remote-viewer` and `virt-viewer` fall
  back to replaying clipboard paste as Inputs-channel
  keystrokes when no vdagent is detected on the guest. The
  existing keyboard polling receives those keystrokes
  character-by-character. Real `play` with no extra UEFI work.
- **Server → client.** No comparable fallback exists. First
  versions render the payload on screen as text the operator
  copies visually from their SPICE client. Genre-honest as a
  fallback (terminal tradition is full of "write this down")
  but not a real channel test until vdagent lands. Status
  rows for these stay `—` until vdagent does.

## USB redirection

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Device hot-plug detected | play | Physical contact — *something is being attached* | — |
| Device hot-unplug | play | Removal / loss of a sense | — |
| Specific device class (HID / mass storage / audio / camera) | play | The kind of intrusion matters — a camera is not a keyboard | — |
| Bulk transfer through redirect | play | Sustained contact / data flowing in | — |
| Isochronous transfer (webcam / audio device) | play | Live feed from the other side | — |
| USB device filter rule applied | set | Permission boundaries — what is allowed to attach | — |

## Smartcard

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Card reader enumerated | set | An identification slot revealed | — |
| Card insertion event | play | Identification offered — *who are you?* | — |
| ATR exchange | set | Challenge and response | — |
| APDU command / response | both | Conversation in a formal protocol — ritual | — |
| Card removal | play | Identity withdrawn | — |
| Multiple readers | set | The instrument can be approached from several directions | — |

## Folder share / WebDAV

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Share mounted | set | A doorway opened | — |
| Directory listing | play | The operator looking around | — |
| File read by client | play | The operator inspecting / copying something out | — |
| File write by client | play | The operator depositing something more permanent than clipboard | — |
| Large file transfer | play | Substantial gift / burden / payload | — |
| File delete by client | play | The operator removing evidence — a heavier act than write | — |

## Cross-channel and synchronization

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| Cursor change synchronised with display change | set | Coordination — left hand knows right hand | — |
| Audio chime fires when display state changes | set | Modalities reinforcing each other | — |
| Clipboard arrival triggers cursor update | both | One channel waking another | — |
| Pointer position drives audio panning | play | Spatial coherence — the world responds to attention | — |
| Latency between input and rendered response | set | Reaction time — how present is the instrument? | — |
| Bandwidth contention across simultaneous channels | set | Triage — what does the instrument prioritise under load? | — |

## Security and transport

| Operation | Mode | Metaphorical role | Status |
|-----------|------|-------------------|--------|
| TLS-encrypted channel established | set | The connection itself is private — or claimed to be | — |
| Cipher / cipher-suite chosen | set | Quality of privacy ? | — |
| Ticket replay rejected | set | The instrument refusing imposters | — |
| Channel-level MAC verified | set | Tamper detection ? | — |
| Per-channel encryption opt-out | set | A channel deliberately left in the clear — *something is meant to be overheard* | — |

## What is deliberately not in this table

- **Tunnel channel** — deprecated, no value testing.
- **Subscribe / unsubscribe to event streams** — implementation
  detail of the test harness, not user-facing behaviour.
- **Anything purely about Ryll's internal state machine** —
  belongs in Ryll's own design docs, not here.
- **Anything that can only be tested by changing the SPICE server
  source** — out of scope; the harness drives a stock server.

## Adding a row

Suggested template, in the spirit of the table format:

```
| <one-line operation description> | set / play / both | <evocative phrase, ?-acceptable> | — |
```

Set the status to `—` when adding the row. Update it to a plan
link (`[phase N](plans/PLAN-...md)`, or `binary: [phase N](...)`
if only one side is built) once a plan covers the work.

If a new test does not fit any existing table, add a new section
heading and start a one-row table. Section ordering is loose —
roughly, channels first, then cross-cutting concerns. The table is
worth more than the prose; if a row earns a paragraph of
explanation, that paragraph probably belongs in a scene-design
document instead.
