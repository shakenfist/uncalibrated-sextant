# Uncalibrated Sextant

A small UEFI Rust binary that boots inside a guest VM and exercises the
SPICE remote-display protocol end-to-end — display, cursor, keyboard,
pointer, audio, clipboard and USB redirection — by being a tiny game
the guest renders to its framebuffer and that a [Ryll](../ryll/)-driven
test harness drives over a serial channel.

The codename is deliberate: a sextant that needs calibrating is exactly
what a SPICE channel under test is — an instrument whose readings you
have to verify before you can trust them.

## Status

Design phase. No code yet. See [DESIGN.md](DESIGN.md) for the channel
mapping, two-channel test architecture, and aesthetic direction.

## Why a UEFI binary

Booting an entire OS to test a remote-display protocol is unpredictable
and slow. UEFI gives us a deterministic pre-OS environment with direct
access to the firmware's Graphics Output Protocol, Simple Pointer
Protocol, Simple Text Input Ex Protocol, and Serial I/O Protocol — all
the surfaces SPICE actually delivers events into — without an OS in the
way. The companion project [uefi-latency-guest](../uefi-latency-guest/)
takes the same approach for latency probing and is intentionally
minimal; uncalibrated-sextant is the rich counterpart that asks "did
every channel work *correctly*", not just "did the path work at all".

## Why a game

Two reasons. First, a diegetic harness exercises SPICE channels in
combinations that synthetic tests don't think to try (a cursor sprite
changing over a hotspot is the cursor channel under realistic load,
not a unit test for the cursor channel). Second, when something
breaks, "the sword cursor doesn't change to a hand over the door" is
instantly obvious to a human reviewing a screenshot — far more so than
a hash mismatch in a log. Fun is a side benefit; debuggability is the
load-bearing reason.

## Sibling projects

- [ryll](../ryll/) — drives the test harness, parses serial events,
  asserts outcomes
- [kerbside](../kerbside/) and
  [kerbside-patches](../kerbside-patches/) — the SPICE proxy layer
  under test
- [uefi-latency-guest](../uefi-latency-guest/) — the minimal latency
  probe; this repo is its richer cousin
- [instar](../instar/) — source of the gRPC-over-serial transport
  pattern we plan to lift
