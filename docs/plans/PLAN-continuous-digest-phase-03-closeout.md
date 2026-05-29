# Continuous multi-channel visual digest — phase 3: docs sweep + decoder coordination + closeout

Parent plan:
[PLAN-continuous-digest.md](PLAN-continuous-digest.md).

## Prompt

Before responding to questions or implementing any step, read
the parent plan's *Phase 3 sketch* and the *Bugs fixed during
this work* / *Documentation index maintenance* sections.
Phase 3 is the docs-and-closeout phase for the whole
continuous-digest master plan; by the end of it the parent
plan is *Complete*.

Read the phase 2 plan's *Closeout* section
([PLAN-continuous-digest-phase-02-multi-channel.md](PLAN-continuous-digest-phase-02-multi-channel.md))
— the *Follow-ups for phase 3* subsection lists three concrete
work items this phase delivers: parent-plan capacity-figure
corrections, ryll-decoder unknown-tag verification, and the
`docs/visual-digest-format.md` rewrite.

Read these source files to ground the docs against current code:

- `src/digest.rs` — the authoritative source for the wire
  format. Particularly the `TAG_*` and `TAG_HASH_*` constants
  (lines 28–96), `DIGEST_SCHEMA_VERSION` (line 33),
  `DIGEST_PAYLOAD_CAPACITY` (line 89), `NUM_HASH_CHANNELS`
  (line 95), `RECORD_HASH_SIZE` (line 91), the `encode`
  function (line 232ish), the `event_tlv_bytes` function
  (line 289ish), and the per-variant record sizes in
  `size_of_record` (line 165). These are the things
  `docs/visual-digest-format.md` describes — drift between
  them is the bug step 3a fixes.
- `src/scene.rs` — particularly `ChannelHashes` (line ~159),
  `DigestRefresher::refresh` (line ~129), the per-line refresh
  insertions in `play_script` (around line 783), the blink-
  transition refresh in `blink_until_key` (line ~1015), and
  `Scene::refresh_digest` (line ~974). These are what
  `ARCHITECTURE.md`'s refresh-cadence paragraph should describe.
- `src/bootloader.rs` — the per-state-change refresh calls
  added in phase 1 step 1f. `ARCHITECTURE.md` should mention
  that the bootloader holds its own refresh-points, replacing
  the previous carve-out.

Read these docs (the ones being updated):

- `DESIGN.md` — particularly the *Two-channel test architecture*
  section and any other paragraphs that frame the on-screen
  digest as "the visual half of the two-channel architecture".
  The actual operator framing is now "the substitute for the
  never-built second-serial gRPC channel".
- `ARCHITECTURE.md` — particularly the paragraph at line 252
  ("The visual half of the two-channel test architecture
  landed via PLAN-visual-digest...") that still describes the
  pre-phase-1 cadence ("at every scene-phase boundary").
- `AGENTS.md` — particularly the *Most recently landed* section
  and any mention of the bootloader refresh-digest carve-out
  (the assert removed in phase 1 step 1e).
- `README.md` — already updated by phase 2 step 2c's sub-agent
  to mention schema v2 and the eight per-channel hashes;
  verify it's still accurate after phase 3 lands.
- `docs/visual-digest-format.md` — the wire-format spec.
  Currently describes schema v1 only (`0x01..=0x08` tags,
  92-byte body budget). Needs a comprehensive rewrite for
  schema v2.
- `docs/plans/PLAN-continuous-digest.md` — the parent plan.
  Contains the V10/L = 213 byte and V20/L = 666 byte typos
  flagged in phase 2's closeout (correct: 271 and 858).

Read the ryll repo to coordinate the decoder check:

- The ryll repo lives at `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll`.
  Step 3e (the decoder-coordination check) reads ryll's QR
  decoder to verify it ignores unknown TLV tags gracefully.
  Do not modify ryll from this repo; the output of step 3e is
  either a "verified OK" note in this plan or a coordination
  note for ryll's own backlog.

## Situation

Phases 1 and 2 changed enough of the digest's behaviour and
wire format that the project docs are out of date:

- `docs/visual-digest-format.md` still describes schema v1
  exclusively. Tag table goes up to `0x08`; capacity budget
  arithmetic uses the pre-phase-2 "92 bytes for records" figure
  (now 44 bytes after the 48-byte hash block lands); body
  layout omits the new TAG_HASH_* records entirely. The
  authoritative-spec contract ("drift between this document
  and the source is a bug") is currently being violated.
- `ARCHITECTURE.md`'s digest paragraph correctly notes that
  phase 2 added rolling hashes but still describes the
  refresh cadence as "at every scene-phase boundary" — the
  pre-phase-1 reality. Per-line / blink / bootloader refresh
  points are not mentioned.
- `DESIGN.md` frames the on-screen digest as "the visual half
  of the two-channel test architecture", which was accurate
  when the gRPC-over-serial transport was the planned
  partner. The operator-clarified framing (per the parent
  plan's *Situation*) is that the digest is the *substitute*
  for the never-built second-serial gRPC channel — different
  architectural role, worth reflecting in DESIGN.
- `AGENTS.md` has accurate phase-1a and phase-2c entries
  added by the respective sub-agents but no entry covering
  phase 1's cadence changes (1c, 1d, 1f). The "Visual on-
  screen digest" entry near line 128 still says "at every
  scene-phase boundary".
- The parent plan (`PLAN-continuous-digest.md`) has two
  capacity-figure typos in its *Open questions* section:
  V10/L = 213 bytes (correct: 271) and V20/L = 666 bytes
  (correct: 858). These do not invalidate any decision the
  plan documents — the V10/L direction was overturned for
  geometry reasons, not capacity — but a future reader
  consulting the parent plan would be misinformed.

Two separable concerns also remain:

1. **ryll-decoder unknown-tag handling.** Phase 2 added eight
   new TLV tags (0x11–0x18) that any v1 decoder will see and
   has to either ignore or fail on. The parent plan deferred
   verification of ryll's behaviour to phase 3. We need to
   read ryll's decoder, confirm graceful handling, and
   document the finding.
2. **Master-plan closeout.** The parent plan's execution
   table currently shows phase 3 as *Not started*. When this
   phase lands, that row goes to *Complete*, the
   `docs/plans/index.md` status flips to *Complete*, and a
   final commit-range link goes into the parent plan's
   closeout-equivalent section. The master plan is then done.

## Mission

By the end of phase 3:

- `docs/visual-digest-format.md` is the authoritative
  schema-v2 spec. It describes the 10-byte header (including
  the version bump from 1 to 2), the 48-byte hash block (8
  TAG_HASH_* records in tag-numeric order), the raw-event
  record region (44 bytes max, newest-first selection,
  oldest-first eviction), and the 4-byte framebuffer-hash
  trailer. The capacity arithmetic is correct. The full
  16-tag table is present. The CRC chaining semantics are
  documented (rolling CRC32C per channel, from boot, with a
  note on the `resume_initial` formula). The provenance
  section points at the right source files including
  `event_tlv_bytes`.
- `ARCHITECTURE.md`'s digest paragraph accurately describes
  the per-line / blink / bootloader-refresh-points cadence
  introduced in phase 1, replacing the "at every scene-phase
  boundary" wording. The relationship between the
  framebuffer-hash trailer (path A, unchanged) and the per-
  channel rolling hashes (phase 2) is clear.
- `DESIGN.md` reframes the on-screen digest as the
  substitute for the never-built second-serial gRPC
  channel — a continuous multi-channel state oracle for
  client-side wedge detection, not just the visual half of
  a two-channel test architecture. The two-channel framing
  can stay as historical context, but the substitute role
  should be the primary description.
- `AGENTS.md`'s *Most recently landed* section gains an
  entry covering phase-1 cadence changes (per-line in
  `play_script`, blink-transition refresh in
  `blink_until_key`, bootloader-internal refresh points).
  The pre-phase-1 "scene-phase boundary" language in the
  older "Visual on-screen digest" entry is corrected.
- The parent plan's capacity-figure typos (V10/L = 213 →
  271; V20/L = 666 → 858) are corrected. Both appear in
  the *Open questions* section; the *TLV capacity strategy*
  and *Why QR and not a denser code* entries are the
  affected locations.
- ryll's decoder is read, verified to ignore unknown TLV
  tags gracefully (or, if not, a coordination note is left
  for the ryll repo's own backlog), and the finding is
  documented in this plan's *Closeout*.
- The parent plan's execution table row for phase 3 goes to
  *Complete (commits <first>..<last>)*; the
  `docs/plans/index.md` row for the master plan flips from
  "Phases 1–2 complete; phase 3 pending" to "Complete";
  this phase plan gains its own *Closeout* section.

Out of scope:

- Any further wire-format changes (the schema v2 layout is
  locked).
- Any ryll-side code changes (read-only verification only).
- Any new test infrastructure (the smoke + chaining
  assertions are sufficient for this scope).

## Open questions

The substantive decisions for this phase are already settled
by phases 1 + 2. The questions below are minor and worth
confirming as the phase executes.

- **DESIGN.md reframing scope.** **Default: edit the digest
  description in place, keeping the two-channel framing as
  historical context.** The "substitute for the never-built
  second-serial gRPC channel" framing is the operator's
  clarified intent; making it the primary description
  matches what's actually shipping. Discarding the two-
  channel framing entirely would orphan the channel-mapping
  table elsewhere in DESIGN.md that uses it as a reference
  point — keep it.

- **AGENTS.md *Most recently landed* — separate entry or
  extend existing?** **Default: add one new entry covering
  phase-1 cadence changes (1c+1d+1f as a logical group),
  separate from the existing phase-1a "Measurement scaffold"
  and phase-2c "Multi-channel rolling hashes" entries.**
  Keeping the scaffold entry distinct preserves the
  commit-by-commit history that section seems to want;
  combining would lose the "cadence is independent of
  hashing" reading.

  Also fix the older "Visual on-screen digest" entry's
  stale "at every scene-phase boundary" language as part of
  the same commit.

- **Ryll-coordination output format.** **Default: a
  *Coordination notes* subsection in this plan's *Closeout*
  with one paragraph per finding** (one finding expected:
  "ryll's decoder ignores unknown tags — verified OK"). If
  the finding is "ryll does not ignore unknown tags", the
  same subsection records what ryll would need to change
  and where in their repo, but does not modify ryll from
  this repo.

- **Parent-plan typo-fix scope.** **Default: fix only the
  numeric values (V10/L 213→271, V20/L 666→858) and leave
  the surrounding text intact.** The decisions those
  sections document were not driven by the wrong numbers
  (V10/L was overturned for geometry reasons, not
  capacity), so no reasoning needs to be re-derived.

- **`make digest-payload-smoke` doc-string update.** The
  smoke's header comment (lines 1–32) describes the v1
  format. **Default: do *not* update the smoke comment in
  this phase** — the smoke's code-level assertions already
  cover v2; the header comment is descriptive and step 3a
  (the spec rewrite) is the authoritative place to describe
  v2. Sweeping the smoke comment would duplicate the spec
  in another location, defeating the single-source rule.
  Worth a small note in step 3a's commit message that the
  smoke header comment is stale-by-design.

## Steps

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 3a   | medium | sonnet | none      | Rewrite `docs/visual-digest-format.md` for schema v2. The current document describes schema v1 only — tag table stops at 0x08, body budget says "92 bytes", layout omits the hash block. Replace with a comprehensive v2 spec covering: schema version (now 0x02), the 48-byte hash block immediately after the 10-byte header (8 records in tag-numeric order 0x11..=0x18, each 6 bytes: tag + len=4 + CRC32C LE), the raw-event record region (44 bytes max, newest-first selection, oldest-first eviction, count cap), and the unchanged 4-byte CRC32C trailer over non-digest framebuffer pixels. Full 16-tag table (8 raw + 8 hash). Capacity arithmetic table corrected: header 10 + hash block 48 + raw 44 + trailer 4 = 106 = V5/L. Add a short subsection on the CRC chaining semantics (rolling CRC32C per channel, from-boot, with the `resume_initial(f) = (f ^ 0xFFFF_FFFF).reverse_bits()` formula and a one-line "why" — CRC_32_ISCSI xorout + refin/refout). Provenance section adds `event_tlv_bytes` as the single source of truth for "what bytes an event produces on the wire". Note in the commit message that the smoke's header comment is intentionally not synced — the spec is the single source. |
| 3b   | medium | sonnet | none      | Edit `DESIGN.md` to reframe the on-screen digest as the substitute for the never-built second-serial gRPC channel — a continuous multi-channel state oracle for client-side wedge detection, not just the visual half of a two-channel test architecture. Keep the two-channel framing as historical context (the channel-mapping table elsewhere in DESIGN.md still references it). The new description should mention: (a) the digest's role as a cross-channel CI oracle (server publishes expected state; client hashes its render and compares; mismatch halts the run for interactive debug); (b) why side-channel-through-display rather than second serial (UEFI second-serial portability across OpenStack / Shaken Fist / Proxmox / oVirt is hard); (c) what's currently carried (display state via framebuffer-hash trailer + 8 per-channel rolling CRC32C hashes covering every Event variant + most-recent raw events for context); (d) future channels (USB redir, pointer, audio, smartcard) slot into the reserved 0x10..=0x1F tag range as the firmware grows support for them. Keep the section structurally similar; mostly rewrite the descriptive paragraphs in place. |
| 3c   | medium | sonnet | none      | Update `ARCHITECTURE.md` and `AGENTS.md` to reflect phase-1's cadence changes. **`ARCHITECTURE.md`:** the paragraph at line 252 starting "The visual half of the two-channel test architecture landed via PLAN-visual-digest" still says "at every scene-phase boundary" — replace with the actual cadence: per-line in `play_script`, on every cursor blink transition in `blink_until_key` (covers both AWAITING and PARKED), and at named refresh points inside `bootloader::run` after each visible state change. The existing "Multi-channel rolling hashes" paragraph (line 262) is correct and stays. **`AGENTS.md`:** the *Most recently landed* section is missing an entry for phase-1's cadence changes (steps 1c + 1d + 1f as a logical group); add one. Fix the older "Visual on-screen digest" entry's stale "at every scene-phase boundary" wording in the same commit. |
| 3d   | low    | sonnet | none      | Fix the two capacity-figure typos in `docs/plans/PLAN-continuous-digest.md`. The values appear in the *Open questions* section's *TLV capacity strategy* entry (V10/L is quoted as "~213 bytes" in the (b) alternative) and the *Why QR and not a denser code* entry (which quotes both V10/L = 213 and V20/L = 666). Correct values per QR Code 2005 Table 7 ECC-L byte-mode: V10/L = 271 bytes; V20/L = 858 bytes. Fix only the numbers; do not rewrite the surrounding reasoning — the decisions those sections document were not driven by the wrong numbers. |
| 3e   | medium | opus   | none      | Cross-repo ryll-decoder verification (research only, no code changes anywhere). Locate ryll's QR-decoder code (likely under `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/`; grep for `SXDG` / `digest` / `qr` / TLV parsing). Read the decoder's tag-handling code and determine whether unknown TLV tags (specifically the new 0x11..=0x18 range) are ignored gracefully or cause the decoder to fail. Document the finding in this phase plan's *Closeout → Coordination notes* subsection. If the finding is "ignored OK", a one-paragraph confirmation is sufficient. If the finding is "decoder fails on unknown tags", document the specific code locations and what would need to change (filenames and line numbers in ryll's repo) — but do not modify ryll from this repo. Whichever way it goes, no Rust or shell code in this repo is touched. |
| 3f   | low    | sonnet | none      | Phase closeout + master plan closeout. Update the parent plan (`PLAN-continuous-digest.md`)'s execution table row for phase 3 from "Not started" to "Complete (commits <first>..<last>)". Update the parent plan's *Future work* section if any items uncovered during phase 3 deserve recording (e.g., a ryll-coordination follow-up if step 3e found unknown-tag handling needs work). Update `docs/plans/index.md`'s row for the continuous-digest master plan from "Phases 1–2 complete; phase 3 pending" to "Complete (commits <first>..<last>)". Add a *Closeout* section to this phase plan with commit map, decisions made during execution, any deviations, and the *Coordination notes* subsection populated by step 3e. |

Commit granularity: one commit per step. Steps 3a, 3b, 3c, 3d
are independent doc edits. Step 3e is research-only (its
output goes into 3f's closeout). Step 3f is the single final
commit that closes the master plan.

## Verification

After each step:

- `pre-commit run --all-files` is green.
- For steps that touch source (none in this phase), `cargo
  build --release` and `make digest-payload-smoke` would
  also need to be green — but every step in phase 3 is
  docs-only, so the rust/build checks are trivially still
  green from phase 2's closeout.

Step 3a additionally: the rewritten spec table for the 16
TLV tags should be visually cross-checkable against
`src/digest.rs`'s `TAG_*` and `TAG_HASH_*` constants and
the per-variant value shapes from `event_tlv_bytes`. Any
drift is a bug.

Step 3e additionally: the finding should be specific
enough that a future reader can locate the relevant ryll
code without re-doing the search.

Step 3f additionally: the parent plan's execution table
and `docs/plans/index.md` should both show the master plan
as *Complete* with consistent commit-range references.

## Closeout

(Populated by step 3f. Should include: commit map of all
phase-3 steps, decisions made during execution, any
deviations from the plan, and a *Coordination notes*
subsection populated by step 3e's ryll-decoder finding.)

## Back brief

Before executing step 3a, please back brief the operator as
to your understanding of the plan and how the work you
intend to do aligns with it. Particular points worth
confirming:

- The DESIGN.md reframing scope (rewrite digest description
  in place; keep two-channel framing as historical context).
- The ryll-coordination output format (a Coordination notes
  subsection in this plan's Closeout; no ryll-repo changes
  from this branch).
- The smoke header-comment policy (not synced this phase —
  spec is the single source).
