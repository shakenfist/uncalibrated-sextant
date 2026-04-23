# Agent guidance for uncalibrated-sextant

## What this repo is

Codename **uncalibrated-sextant**. A SPICE channel exercise harness
delivered as a tiny retrofuturist sci-fi mini-game running as a
`no_std` UEFI Rust binary inside a guest VM. The harness is driven by
[Ryll](../ryll/) over a serial link.

The codename is intentional: pick a true name later, when the content
tells us what it should be.

## Where to read first

1. [README.md](README.md) — project framing
2. [DESIGN.md](DESIGN.md) — channel mapping table, two-channel test
   architecture, design principles, aesthetic direction, open
   questions
3. [ARCHITECTURE.md](ARCHITECTURE.md) — implementation shape (stub
   until code lands)

## Current phase

Design only. There is no Rust code yet. Before scaffolding a Cargo
project, the open questions in DESIGN.md ("Open questions" section)
should be at least partially resolved — particularly art direction,
audio scope, and whether to build one scene first or design the full
sequence on paper.

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
