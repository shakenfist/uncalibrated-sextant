# First playable — phase 3: Pre-commit, style, minimal CI

Parent plan: [PLAN-first-playable.md](PLAN-first-playable.md).

## Prompt

Before working on this phase, re-read `DESIGN.md`, the master
plan, and the Phase 1 and Phase 2 plans. Phase 1 established a
Docker-based Rust build and scaffolded `src/main.rs`. Phase 2
added ESP assembly, `make qemu`, and `make release` tooling and
four supporting shell scripts. Phase 3 wraps a style and
correctness net around all of that so future changes land with
clippy / rustfmt / shellcheck discipline rather than drifting.

No Rust code changes, scene work, or build-path rewrites are in
scope here. The mission is a passing `pre-commit run
--all-files` on a clean tree and a pattern future phases (and
future phases' sub-agents) can rely on.

Cross-repo references, in order of likely usefulness:

- `shakenfist/ryll/.pre-commit-config.yaml` — canonical pattern
  for a shakenfist Rust project's pre-commit setup. Read and
  adapt rather than reinvent.
- `shakenfist/ryll/scripts/check-rust.sh` — canonical
  Docker-wrapped rustfmt + clippy helper. Named explicitly in
  the master plan as the model for our own helper.
- `shakenfist/ryll/rustfmt.toml` / `shakenfist/ryll/Cargo.toml`
  (for `[workspace.lints]` or `[lints]`) — canonical style
  settings for Rust code in this workspace.
- `shakenfist/ryll/.github/workflows/` — prior art for GitHub
  Actions CI if we decide to add a workflow in this phase (see
  *Open questions*).

Plus in-repo:

- `scripts/build.sh`, `scripts/mkesp.sh`, `scripts/qemu.sh`,
  `scripts/verify-release.sh` — the four shell scripts that
  now exist and will be run through shellcheck here.
- `Dockerfile` — already installs `rustfmt` and `clippy` as
  rustup components, so `scripts/check-rust.sh` can use the
  same image without extending it.
- `src/main.rs` — the only Rust file; will need to pass
  `cargo fmt --check` and `cargo clippy -D warnings`.

All planning documents go in `docs/plans/`.

## Situation

The repository has a working build and launch toolchain but no
style discipline. No pre-commit config exists, no rustfmt or
clippy enforcement, no shellcheck on the scripts that landed in
Phase 2. Running `cargo clippy -D warnings` or `shellcheck
scripts/*.sh` today might pass or might surface issues; we do
not know and we have no repeatable local-dev gate that will
catch drift.

The Phase 1 Dockerfile already includes rustfmt and clippy
components, so Rust-side linting does not need a new image —
only a wrapper script in the ryll pattern.

## Mission and problem statement

Install a pre-commit hook suite that enforces:

- Rustfmt formatting (`cargo fmt --check`) via a Docker wrapper
- Clippy lints with `-D warnings` via the same Docker wrapper
- Shellcheck on all files under `scripts/`
- Whitespace / EOF hygiene on all text files
- YAML validity if / when YAML appears

The phase is done when `pre-commit run --all-files` exits 0 on
a clean tree, and the hooks are wired up so future commits get
them automatically.

## Open questions

- **CI in this phase or later?** The master plan's sketch
  notes CI as optional-in-Phase-3. The cheapest useful CI is a
  GitHub Actions workflow that runs `pre-commit run
  --all-files` on push and PR. That is trivial if we are
  willing to let GitHub-hosted runners install pre-commit +
  Docker on-demand, but might be slow and a bit wasteful of
  shared runners. The shakenfist self-hosted runner pattern
  (see memory: full `[self-hosted, vm, <os>, <size>]` label
  set required) is better but requires infrastructure
  decisions that feel out of scope here. Default: add a
  minimal GitHub-hosted workflow as a Step 4 stretch; defer
  self-hosted integration to Future work.
- **Pre-commit installation on host.** Pre-commit itself is a
  Python tool. Most shakenfist contributors have it via pipx
  or a global venv. We do not need to add another install
  path — the plan assumes contributors have pre-commit
  available and documents the one-line install command in
  `README.md` / `AGENTS.md` for anyone who does not. No
  Docker-wrapped pre-commit (that is overkill; pre-commit is
  a small Python tool and installing it host-side is normal
  practice).
- **`msrv` in `clippy.toml`.** Clippy can be told to respect
  the MSRV from `Cargo.toml`'s `rust-version` field. We pin
  Rust to 1.88.0 and the `uefi` crate's MSRV is 1.88, so
  setting `msrv = "1.88"` in `clippy.toml` is correct. Confirm
  at Step 2.
- **Lint severity policy.** Clippy has many warning levels
  (`warn`, `style`, `complexity`, `perf`, `pedantic`, `nursery`,
  `cargo`). The safe starting policy is to deny warnings at
  the default level (`-D warnings`) and leave `pedantic` /
  `nursery` off for now. Add them later if they earn their
  keep. No `[lints]` section in `Cargo.toml` at this phase —
  keep lints driven from the clippy CLI invocation in
  `check-rust.sh`.

## Execution

| Step | Effort | Model  | Isolation | Brief for sub-agent |
|------|--------|--------|-----------|---------------------|
| 1    | low    | sonnet | none      | Read ryll's pre-commit config, `scripts/check-rust.sh`, and `rustfmt.toml`. Report the concrete patterns to lift. See Step 1 below. |
| 2    | medium | sonnet | none      | Write `.pre-commit-config.yaml`, `rustfmt.toml`, `clippy.toml`, and `scripts/check-rust.sh`. See Step 2 below. |
| 3    | medium | sonnet | none      | `pre-commit install`, then `pre-commit run --all-files`. Fix anything it surfaces — expect shellcheck nits, possibly rustfmt diffs, possibly clippy warnings in `src/main.rs`. Iterate until clean. See Step 3 below. |
| 4    | low    | sonnet | none      | Decide and implement: minimal GitHub Actions workflow that runs `pre-commit run --all-files` (stretch; defer if it turns into a real infrastructure problem). See Step 4 below. |
| 5    | low    | sonnet | none      | Update `README.md`, `ARCHITECTURE.md`, `AGENTS.md`. Document pre-commit install and the `scripts/check-rust.sh` helper. See Step 5 below. |

### Step 1 — mirror ryll's pattern

Read and summarise:

- `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/.pre-commit-config.yaml`
  — copy the hook list (repo + rev + hooks) and note any
  customisations. Which repos does ryll pull from? Which
  arguments does it pass? Does it skip any files?
- `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/scripts/check-rust.sh`
  — read verbatim. What does ryll's `check-rust.sh` do
  exactly? Does it accept `check` / `fix` subcommands? Does
  it invoke Docker, and if so with what image? How does it
  handle the cargo target volume?
- `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/rustfmt.toml`
  — the actual Rust formatting settings to mirror.
- `/srv/kasm_profiles/mikal/vscode/src/shakenfist/ryll/clippy.toml`
  if present — any clippy configuration.
- Anything else under `shakenfist/ryll/` that looks like
  style or CI infrastructure (`.cargo/`, `deny.toml`, etc.).

Output: a concise summary of what to lift verbatim, what to
adapt, and anything surprising. No file changes. Under 400
words.

### Step 2 — write the configs

Create:

- **`.pre-commit-config.yaml`** based on ryll's, adapted for
  uncalibrated-sextant's file set. Hooks should include at
  least:
  - `pre-commit/pre-commit-hooks`: `trailing-whitespace`,
    `end-of-file-fixer`, `check-yaml`, `check-merge-conflict`,
    `check-added-large-files`, `mixed-line-ending`.
  - `shellcheck-py/shellcheck-py` or equivalent: shellcheck
    over `scripts/`.
  - A local hook invoking `./scripts/check-rust.sh check` for
    rustfmt + clippy, scoped to Rust file changes.
- **`rustfmt.toml`** — copy ryll's.
- **`clippy.toml`** — at minimum `msrv = "1.88"`. Any other
  fields lifted from ryll.
- **`scripts/check-rust.sh`** (executable) — Docker-wrapped
  rustfmt + clippy helper following ryll's pattern. At
  minimum:
  - `./scripts/check-rust.sh check` → `cargo fmt --all --
    --check` + `cargo clippy --all-targets -- -D warnings`
    inside the Phase 1 Docker image.
  - `./scripts/check-rust.sh fix` → `cargo fmt --all` (in-place
    fix) + `cargo clippy --all-targets --fix --allow-dirty
    --allow-staged -- -D warnings`.
  - Reuse the named `uncalibrated-sextant-target` volume from
    Phase 1 so cargo incremental state is shared.

Keep Makefile unchanged in this step. A `make check` convenience
target can be a Step 5 addition if it earns its keep.

### Step 3 — run pre-commit and fix fallout

1. Install hooks: `pre-commit install`.
2. Run: `pre-commit run --all-files`.
3. Expect some issues. Likely candidates:
   - `end-of-file-fixer` on any doc that does not end in a
     newline.
   - Shellcheck nits in `scripts/build.sh`, `mkesp.sh`,
     `qemu.sh`, `verify-release.sh`. Things like quoting,
     `$PATH` assumptions, inline `sh -c` tricks.
   - Clippy warnings on `src/main.rs`: the current code is
     small but `-D warnings` promotes any lint to error.
     Expected candidates: missing `# Safety` docs on
     `unwrap()`, or none at all.
   - Rustfmt noise: main.rs should already be close to
     formatted; check any diff.
4. For each failure, decide: fix in-place, or add a narrow
   `exclude:` rule. Prefer fixing. Only exclude if the hook is
   wrong about the file (e.g. binary blobs).
5. Iterate until `pre-commit run --all-files` exits 0.

Do not commit work in progress. The management session commits
the final clean state.

### Step 4 — minimal GitHub Actions workflow (stretch)

If time permits, add `.github/workflows/pre-commit.yml`:

```yaml
name: pre-commit
on: [push, pull_request]
jobs:
  pre-commit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: pip install pre-commit
      - run: pre-commit run --all-files
```

Important caveat: the local hook invoking `scripts/check-rust.sh`
uses Docker, which Ubuntu runners have but configuring it to hit
our named volume on a fresh VM is fiddly. The first cut of the
workflow can skip the Rust hook (`SKIP=check-rust`) and keep
only the whitespace / YAML / shellcheck hooks. Flag the Rust
gap in `Future work`; real Rust CI is better done against the
shakenfist self-hosted runners, which is its own plan.

If the workflow turns out to be more than a trivial diff,
defer to Future work instead. The phase's core mission is
local pre-commit, not CI.

### Step 5 — documentation

Update:

- **`README.md`** — short *Contributing* section pointing at
  `pre-commit install` and `pre-commit run --all-files` as the
  local gate. One-liner on `scripts/check-rust.sh fix` for
  auto-formatting. Noting that `pre-commit` itself is a
  required contributor-side dep alongside Docker.
- **`AGENTS.md`** — add `./scripts/check-rust.sh check` and
  `./scripts/check-rust.sh fix` to Build commands (or a new
  Style commands subsection). Note that `make` targets do not
  yet invoke pre-commit — contributors run it explicitly.
- **`ARCHITECTURE.md`** — brief note that style enforcement
  lives in `.pre-commit-config.yaml` + `scripts/check-rust.sh`
  and that the Phase 1 Docker image is reused for Rust-side
  checks.

## Agent guidance

Follow the master plan's *Agent guidance* section verbatim. Two
phase-specific emphases:

- **Do not add lint suppressions to paper over real issues.**
  If clippy flags something in `src/main.rs`, fix the code.
  Only use `#[allow(...)]` if the lint is genuinely wrong for
  the situation, and document why in a comment.
- **Do not run `cargo clippy --fix` on changes that alter
  semantics.** `--fix` is fine for trivial formatting-adjacent
  things; inspect any diff before accepting. clippy
  occasionally offers a "fix" that is actually a behaviour
  change.

## Success criteria

Phase 3 is complete when:

- [ ] `pre-commit install` wires hooks into `.git/hooks/` on a
      fresh clone.
- [ ] `pre-commit run --all-files` exits 0 on a clean tree
      with no uncommitted changes.
- [ ] `./scripts/check-rust.sh check` passes standalone.
- [ ] `./scripts/check-rust.sh fix` is available and idempotent
      (second run reports nothing changed).
- [ ] `README.md`, `AGENTS.md`, `ARCHITECTURE.md` mention
      `pre-commit` as a contributor dep and name the helper
      script.
- [ ] The Step 4 workflow either lands and passes on the first
      CI run, or is explicitly deferred to Future work with a
      one-liner rationale.

## Bugs fixed during this work

(None yet — populated during execution.)

## Future work

- Self-hosted-runner CI on shakenfist infrastructure (the
  `[self-hosted, vm, <os>, <size>]` label pattern), so the
  Rust hook runs under real Docker-with-named-volumes and we
  get fast CI feedback without abusing GitHub-hosted runners.
- `make check` / `make fmt` / `make lint` Makefile
  conveniences if contributors find running
  `scripts/check-rust.sh` directly awkward.
- `pedantic` / `nursery` clippy lints if we decide the marginal
  value is worth the noise.
- `deny.toml` (via `cargo-deny`) for supply-chain and licence
  policing, matching ryll. Probably Phase 4+ once we have more
  dependencies than just `uefi`.
- Markdown-lint on `docs/` if it turns out our plan files have
  drifting styles.
