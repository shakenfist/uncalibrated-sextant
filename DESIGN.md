# Uncalibrated Sextant — Design

## Purpose

Exercise every meaningful SPICE channel from inside a guest VM, using
a small UEFI Rust binary, in a way that is both rigorously assertable
(by Ryll, headlessly) and visually obvious to a human reviewing a
screenshot. The harness is dressed as a tiny retrofuturist sci-fi
mini-game so that the channel exercises emerge naturally from
gameplay rather than feeling synthetic.

## Two-channel test architecture

The guest emits the same stream of test events through two
independent channels, both fed from a single in-memory ring buffer:

- **Serial (gRPC-over-serial, bidirectional)** — primary headless
  assertion channel. Ryll drives the guest ("render this payload as a
  QR", "report current framebuffer hash", "advance to next scene")
  and consumes structured events ("key_down scancode=0x1e at t=…").
  Transport pattern lifted from [instar](../instar/).
- **Visual (on-screen digest)** — periodic QR or compact text
  rendered into the framebuffer, encoding the last N events from the
  same ring buffer. A SPICE-client-side harness screenshots the
  client's view and decodes this. This validates the *display path*
  (does the client see what the guest drew?), independent of serial.

Because both channels read from the same ring buffer, they should
agree by construction. Divergence between "what serial says arrived"
and "what the client screenshot decoded" localises the bug to display
vs input path.

A second serial port (added to Shaken Fist if needed; Nova supports
this via `hw:serial_port_count`) keeps test events out of the firmware
console log. Cleaner framing on its own channel, no risk of collision
with mid-event firmware panics.

### Connection handshake (avoiding the UEFI-fast / client-slow race)

A UEFI binary boots in a handful of seconds, which is fast enough to
finish the opening boot-sequence scene before a human has pointed
virt-viewer at the VM, or even before a CI-orchestrated SPICE client
has finished its TLS handshake. Starting the sequence too early
means the client misses it; screenshots come back blank; the
visually-asserted half of the test becomes unreliable.

We solve this with an explicit handshake, holding at a diegetic
"AWAITING OPERATOR" screen (blinking cursor, CRT wobble slowly
warming up) until one of two signals arrives:

- **Default (headless CI):** wait for Ryll's `Start` command on
  serial. Ryll is orchestrating both sides of the test, so it is
  the natural arbiter of when the SPICE client is ready. It only
  sends `Start` once the client has connected and the
  client-side capture harness is armed.
- **Fallback (human play via virt-viewer):** wait for the first
  client-originated input event. A disconnected client cannot send
  keyboard or pointer events into the guest, so the first such
  event is proof that someone is watching.

The AWAITING screen is a small but real piece of content in its own
right, not dead time — it's the operator's first view of the
system, and it sets the tone before the boot sequence proper
begins. A steady blinking cursor on this screen (and anywhere else
the system is waiting for input) is the liveness signal that
distinguishes "waiting for you" from "frozen"; see *Boot sequence*
for how that blink doubles as the cheapest available glitch surface.
It also doubles as a reconnect-safe state: if the client
disconnects mid-session and reconnects, SPICE replays the last
framebuffer, so holding on the opening screen until a positive
connection signal means no client ever sees the sequence
mid-flight.

## SPICE channel mapping

| SPICE channel    | In-game mechanic                                | What Ryll asserts                                          |
| ---------------- | ----------------------------------------------- | ---------------------------------------------------------- |
| Display (main)   | Scene rendering, parallax, mode walk on boot    | Frame hash matches; mode renegotiation succeeded at each step |
| Cursor           | Cursor sprite swaps over hotspots               | Cursor shape hash matches expected for current hotspot     |
| Inputs (kbd)     | Dialog choices, inventory keys                  | Guest received key event matches what client sent          |
| Inputs (pointer) | Mouse-driven menu, click targets                | Guest received pointer position + click matches client     |
| Audio playback   | Chimes, ambient music, SFX                      | Audio frames received guest-side; client receives playback |
| Audio record     | "Voice command" minigame                        | Mic data flows client → guest                              |
| Clipboard        | Pick up scroll → text copies to clipboard       | Roundtrip text matches; both directions                    |
| USB redirection  | Insert "data crystal" (USB stick) into console  | USB device enumerated and read guest-side                  |
| Smartcard (opt)  | Key-card door                                   | Smartcard auth events arrive                               |
| Folder share (opt) | "Captain's log" persistent file               | Read/write to shared folder roundtrips                     |

If a channel has no plausible in-game mechanic, that's a finding —
either the channel doesn't matter for our coverage or we need to
revisit the setting.

## Design principles

1. **The game would survive having its test purpose stripped.** If
   the scene only makes sense as a test harness in costume, it has
   drifted into corporate-escape-room territory. Diegetic mechanics,
   not "ClipboardQuest".
2. **No fourth-wall breaks, but in-character self-questioning is
   fine and encouraged.** No NPC announces "I shall test thy USB
   redirection!". Deadpan README, deadpan in-game text. The wall
   in question is between the fiction and the test harness; the
   wall *inside* the fiction — an anxious narrator wondering
   aloud about its circumstances — is load-bearing, not a break
   (see Aesthetic direction).
3. **Debuggability over fidelity.** A broken channel should be
   visually obvious in a single screenshot. Optimise scene design
   for "what would it look like if X were broken?".
4. **Mode walks are a feature, not boilerplate.** Stepping through
   resolutions during a "system coming online" boot sequence both
   makes diegetic sense and forces SPICE to renegotiate the display
   channel multiple times — a known bug surface.
5. **Game people would actually play with virt-viewer for five
   minutes.** This is the test for whether a feature belongs in the
   game or behind a debug flag.
6. **Prefer per-glyph / per-tile draws over monolithic framebuffer
   writes.** The *way* we draw controls which SPICE opcodes the
   server emits, which controls what gets exercised on the wire.
   Repeated glyphs and tessellated tiles let GLZ's dictionary do
   its job (first occurrence is a bitmap, subsequent ones are
   dictionary references), and that dictionary-reuse path is
   exactly the kind of thing we want under test. A straight
   `memcpy` into the GOP linear framebuffer collapses to a single
   `DRAW_COPY` the server cannot decompose; a per-glyph BitBlt
   with a stable glyph cache exposes the full compression path.

## Aesthetic direction

Inspirations (from the project owner): Asimov's robots and
Foundation, Battlestar Galactica (remake), Fallout (especially the
new TV series' deadpan tone), Terminator, and — importantly for
voice — Martha Wells' Murderbot Diaries. The unifying thread is
retrofuturist tech, dry humour about catastrophe, and machines that
are uncanny, sometimes sympathetic, and sometimes quietly unsure
of themselves.

The strongest fit for this project's constraints is the **Fallout**
end of the spectrum:

- Pip-Boy / vault-terminal aesthetic is essentially what UEFI looks
  like already (low colour count, chunky text on dark background)
- Enclosed-facility framing keeps the renderable area honest for a
  no_std binary with a small asset budget
- Atompunk-corporate-dystopia tone gives a deadpan voice that
  doesn't drift twee

The Asimov / BSG threads layer naturally on top: the guest can be a
small robot or shipboard subsystem booting up in an abandoned
facility, re-enumerating its own sensors — which is *literally* the
test ("can I see, can I hear, can I feel input, can I touch the
outside world via USB"), dressed as fiction.

**No IP infringement.** Stay in the genre idiom — low-res palette,
enclosed-vault setting, deadpan corporate dystopia humour — without
lifting names, characters, or assets.

### Voice: unreliable narration leaks

Most of the time the machine reports dryly: telemetry lines, OK /
FAILED, coordinate readouts. Occasionally, usually after a
FAILED or an unexpected result, a line of the narrator's inner
voice leaks into the log — anxious, questioning, slightly
confused, and unsure whether to trust its own sensors. It is not
cartoonishly neurotic; it is earnest and understated, the way
Martha Wells' Murderbot is. The contrast between dry external
telemetry and panicked internal parentheticals is the *point*:
the machine is, against its better judgement, a person.

One useful device: **in-character speculation that happens to be
literally true of the harness itself.** The narrator wondering "is
it possible I'm on a test bench in a workshop? Why wouldn't they
tell me?" is both diegetic (a character speculating about its
situation) and true (it is, in fact, running in QEMU for a test
suite). That wink is functional rather than meta, which keeps it
on the correct side of principle 2. Use it sparingly — rare
enough to feel like a genuine leak, not a schtick.

This voice has a practical payoff for the harness: failures are
more interesting than successes. A test bench that shows a clean
sequence of OKs is visually boring and tells the reviewer nothing
about whether the failure path works. Having the narrator *react*
to failures — in an earnest, self-doubting way — makes FAILED
lines screenshottable content rather than errors to hide, and
gives us a diegetic excuse for why some capabilities are still
stubs. It also ties neatly into the Wire-level control ladder:
when `direct hardware control: FAILED` eventually becomes `OK`,
the narrator's worry softens, and the tonal shift is a real
in-game reward for the implementation work.

**Keep the self-doubt deliberately multi-valent.** The narrator
should not resolve onto any single interpretation of what is
wrong — hardware fault, environmental degradation, drift,
something more fundamental, a supply-chain attack. The text
should let readers map the uncertainty onto whichever register
they find meaningful (aging machinery, reliability of one's own
senses, existential unease, something else) without ever
committing to one reading. Ambiguity is the feature, not a
failure to decide. See `docs/creator-notes/` for the design
discussion this principle came out of.

## Wire-level control

How directly we talk to the virtual display hardware is both an
engineering axis and a design axis, because the choice of
abstraction determines what SPICE opcodes reach the client and
therefore what gets tested. We want this choice to be deliberate,
progressive, and — importantly — visible inside the fiction.

Three rungs on the ladder, in order of effort:

1. **GOP-only (baseline).** Write to OVMF's linear framebuffer via
   the Graphics Output Protocol. OVMF's QXL-GOP driver turns our
   writes into `DRAW_COPY` operations for dirty regions. We control
   *content*, QEMU picks opcodes. This exercises the common path
   and is enough for the first milestone.
2. **GOP plus targeted QXL helper.** Keep GOP for the general
   framebuffer, but open the QXL PCI device via UEFI's PCI I/O
   Protocol for specific tests — post explicit `DRAW_FILL`,
   `DRAW_OPAQUE` with a chosen ROP3, or `BITBLT` with reused bitmap
   IDs to force dictionary reuse. Coexisting with OVMF's QXL-GOP
   driver is the awkward bit; we'd either negotiate regions with it
   or briefly disconnect it for a test.
3. **Full QXL-direct.** `DisconnectController()` on OVMF's QXL-GOP
   driver at startup, take over the device, render everything via
   the command ring. Full deterministic control over every SPICE
   display opcode emitted. The only way to truthfully assert
   "ryll handles every opcode we can throw at it" rather than
   "ryll handles whatever OVMF happens to emit".

**Diegetic framing — the self-test attempts direct control and
falls back if it fails.** The boot sequence openly reports what
video mode the subsystem ended up in:

```
VIDEO SUBSYSTEM: direct hardware control ...... FAILED
                 supervised paravirtualisation .. OK
```

This is not a facade — it is the *actual* implementation state for
the first milestone, which lives on rung 1 and renders the
`FAILED` line truthfully. When we implement rung 2 or 3 properly,
the same line flips to `OK` and the game gains a real capability.
The aesthetic even changes subtly: full QXL-direct lets us render
effects the GOP path can't. A running gag across versions, and a
self-documenting record of what the test harness can currently
exercise on the wire.

**Honest uncertainty.** I know the shape of the QXL device and
UEFI's driver model but I have not previously written a Rust UEFI
app that `DisconnectController`s OVMF's QXL-GOP and drives the
command ring. There will be real research in the rung-2/rung-3
implementation phases, and the `uefi-rs` PCI I/O bindings are the
place to start.

## Boot sequence (first scene)

The first scene we build — and the thing we use to prove out build
tooling, bootable-VHD packaging, and release process before any
game design is committed — is the system coming online.

**Tone:** The game boots to a 1980s green-screen (or amber, TBD)
interface of a computer starting up. The signal is not clean; it
wavers, shudders, and is occasionally a little out of focus. The
boot sequence is reasonably fast — tens of seconds — but appears
to show meaningful information: register dumps, memory checks,
peripheral probes, microcode loads, all in the idiom of old
minicomputers and workstations (PDP-11 / RT-11 / VMS / Symbolics
Genera / Alto / CM5 / Multics / early Unix dmesg). Messages are
composed *in the style* of those systems, not lifted verbatim — the
genre idiom isn't IP, specific strings are.

**The boot messages do double duty as the SPICE channel self-test.**
Each line the system prints is both diegetic texture and a real
capability probe, with a structured event sent over serial to Ryll
for assertion:

```
REMOTE LINK: serial @ COM2 ......... ACQUIRED   <— Ryll arrives
AWAITING OPERATOR ................... [connection confirmed]
VIDEO SUBSYSTEM: direct hardware .... FAILED    <— see Wire-level control
    (supervised mode is the fallback. Who is supervising me, then?
     And from where? It is probably fine.)
                 supervised mode .... OK
                 640x480x8 ........... OK
                 800x600x16 .......... OK
                 1024x768x32 ......... OK
                 mode locked:         1024x768x32
POINTER: sensing ..................... OK       (coord 512, 384)
KEYBOARD: enumerate .................. OK       (modifiers nominal)
AUDIO DAC: 44.1 kHz sine ............. [o]      <— a chime plays
CLIPBOARD RELAY: handshake ........... OK
STORAGE: local media ................. NONE     (expected)
THRUSTER CONTROL: self-test .......... FAILED
    (is it possible I am on a test bench in a workshop? Why
     wouldn't they tell me?)
SENSORIUM: nominal
BOOT COMPLETE IN 23.4s
```

Every `OK` is the guest observing that a SPICE channel delivered
something plausible, and every such line is also a serial event for
Ryll to verify independently. The display mode walk exercises the
SPICE display channel's renegotiation path (a known bug surface);
the chime exercises the audio channel; the pointer coordinate
proves the Simple Pointer Protocol returned a sensible event; the
clipboard handshake probes that channel too.

**The boot text is also a GLZ stress pattern by construction.** The
repeated dots-to-column alignment, the recurring `OK`/`FAILED`
tokens, the monospace glyph set that appears hundreds of times on
screen, and — once we add the CRT-scruff overlay — the tessellated
scanline tiles, are the kind of content GLZ's image-dictionary was
designed for. If we obey principle 6 and render text via per-glyph
BitBlt from a stable cache (rather than pre-rasterising whole lines
into the framebuffer), the server sees the same bitmap ID re-issued
hundreds of times per frame, which exercises the
dictionary-matching path ryll has to handle. The aesthetic and the
compression test reinforce each other rather than competing.

**On the "poor video signal" aesthetic vs frame-hash assertions.**
Keep the CRT degradation as *localised sprite overlays* — scanline
bands, a wavering horizontal tear, an occasional out-of-focus halo
around certain regions — rather than a global framebuffer-wide
filter. The base framebuffer stays clean and deterministic, which
is what Ryll hashes. The scruff is layered on top in known regions,
so the renderer can enumerate exactly which pixels are "decorative"
vs "signal" and Ryll's assertions can either exclude the decorative
regions or consult a separate clean layer. Bonus: the *shape and
position* of the degradation sprites is itself under the renderer's
control, so we can use them as a deliberate cursor-channel /
display-partial-update test (a drifting tear is an endless stream of
small dirty rectangles, which is exactly the kind of thing that
breaks in cursor vs display prioritisation).

This resolves the tension between "looks cool and unstable" and
"is testable": the instability is diegetic, visible, and fun, but
it's a layer, not the signal.

**Blinking cursor and the character-ROM glitch.** A steady
blinking cursor — classic terminal cadence, around 1 to 1.5 Hz —
carries two loads simultaneously. Functionally, it is the
liveness indicator that lets an operator distinguish "waiting
for input" from "session frozen"; the AWAITING OPERATOR screen
and any post-sequence parking screen rely on this. Aesthetically,
it is the cheapest glitch surface available to us. The cursor
glyph is rendered from a character ROM that is *slightly*
degraded: most blinks produce the canonical glyph, but
occasionally one of a small pre-authored set of broken variants
appears instead — a missing pixel, a smeared edge, a shifted
column, a stuck phosphor trail. The variants are deterministic
bitmaps with stable identifiers and the substitution cadence,
though noisy, is ultimately scripted, so a future Ryll milestone
can assert which variant appeared when.

This is also a quietly effective test of the display channel's
small-repeated-update path. A blinking cursor is a stream of
tiny BitBlt operations at a regular cadence; when the cursor
glyph varies, GLZ's dictionary has to handle multiple bitmap IDs
in quick succession at the same screen position. Neither
behaviour is exotic, both are the kind of thing that can break
subtly under load.

Note, for correctness: the text cursor is **display**-channel
content, not SPICE-cursor-channel content. The SPICE cursor
channel is specifically for the mouse pointer (shape, visibility,
position). The two are easy to conflate and the design doc should
not.

**Decision for the first-playable milestone.** The cursor
glitch is much cheaper to implement than a full CRT scruff
overlay (a handful of small bitmaps and a substitution schedule
versus a compositor layer), so it *is* the minimum-viable
glitch for the first-playable milestone. The scanline-tile
overlay becomes a stretch goal within that milestone; the
drifting horizontal tear and out-of-focus halo effects remain
explicitly tracked later-milestone work. The point is to ship a
visible, on-brand glitch effect without the overlay being a
gating dependency — not to drop the overlay forever.

## First milestone scope

We bootstrap this project via a local `make qemu` Makefile target,
modelled on [ryll's](../ryll/Makefile). No OpenStack deploy, no
Shaken Fist changes, no Ryll integration — just the UEFI binary
running under QEMU with OVMF on a developer machine, producing
something a human can sit in front of.

Concretely, in this first milestone:

- **Human-play mode only.** Use the first-client-input handshake
  from the *Connection handshake* section (wait for the first
  keypress or pointer event, not a serial `Start` command).
  A developer runs `make qemu`, a QEMU display window opens,
  a keypress leaves the AWAITING screen and kicks off the boot
  sequence.
- **Serial channel is stubbed.** Events are ring-buffered
  internally but not yet framed over serial. The gRPC-over-serial
  transport, lifted from `instar`, is a subsequent milestone once
  we've proven UEFI + build tooling + aesthetic work in isolation.
- **No assertions against Ryll.** Success for this milestone is
  purely visual: the boot sequence renders as intended, the
  localised CRT scruff looks right, the narrator leaks land, and
  the whole thing is screenshottable enough that we feel good
  about the aesthetic. It is a demo, not yet a test harness.
- **No Shaken Fist or Nova work.** The second serial port, the
  `hw:serial_port_count` configuration, and any integration with
  the orchestration layer are all deferred until we have a
  working harness to exercise against them.
- **Wire-level control stays on rung 1 (GOP only).** The boot
  sequence's `direct hardware control: FAILED` line is
  truthfully reporting what the harness is actually doing.
  Rungs 2 and 3 are for later milestones.

This lets us get a bootable artifact, a working build pipeline,
and a first taste of the aesthetic in hand — without committing
to any cross-repo work — before we decide whether the concept has
legs.

## Story sketch (placeholder)

A long-dormant maintenance subsystem boots in an abandoned facility.
Its sensors come online one at a time during a self-test. As it
regains capabilities it explores its immediate surroundings, finds
clues about why everything is empty, and eventually contacts a
distant operator (Ryll) over an old serial link.

This is just enough premise to justify the scenes that hit each
channel. To be expanded — or replaced — once we commit to a setting.

## Open questions

- **True name** — keep `uncalibrated-sextant` as codename until the
  content tells us what it should be called
- **Art direction** — true 8-bit palette? EGA 16-colour? VGA 256?
  Pixel size? Tile size? Target virtual resolution (likely
  letterboxed inside whatever GOP gives us)
- **Audio strategy** — PC speaker via UEFI is trivial but limited;
  AC97/HDA via SPICE needs a driver in the UEFI binary. How much
  audio coverage do we actually want?
- **USB redirection mechanic** — what does "insert the data crystal"
  look like in practice? UEFI USB stack support is non-trivial
- **First scene vs whole game** — build one complete scene that
  exercises 2–3 channels end-to-end, then expand? Or design the
  full sequence on paper first?
- **Concept art** — ASCII mockups initially are fine; commissioning
  pixel art is a separate decision
- **Kerbside-as-distance-measurement scene (later milestone).** A
  future scene concept where the narrator ostensibly computes its
  distance from home via round-trip latency measurement while the
  harness is, in reality, benchmarking Kerbside's Python SPICE
  proxy under a usbredir stress load (large data transfer from a
  redirected USB disk). Dual-purpose: a real performance benchmark
  for a known concern, plus screenshottable output and a natural
  place for narrator self-doubt ("has my clock drifted? is the
  operator spoofing light-speed delay?"). Not in scope for the
  first-playable milestone. Source discussion in
  `docs/creator-notes/2026-04-concepts.md`.

## Tooling notes

- **Claude Design** (Anthropic Labs,
  <https://www.anthropic.com/news/claude-design-anthropic-labs>) may
  have a part to play in the pre-implementation phase — it's oriented
  at interactive prototypes, mockups, wireframes, and pitch decks
  rather than pixel-grid sprite work, so not a replacement for
  Aseprite/Piskel once we commit to an art style. Plausible uses
  include mood-boarding aesthetic directions, storyboarding the
  channel-walk boot sequence, or building a clickable web paper
  prototype to stress-test whether a scene is actually fun before
  investing in no_std UEFI code. We're not yet sure which of these
  (if any) will earn its keep; noted here so we remember to try it
  when an appropriate question comes up.
