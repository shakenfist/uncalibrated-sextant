// Scene state machine and boot-sequence script for uncalibrated-sextant.
//
// Implements three phases:
//   1. run_awaiting  — AWAITING OPERATOR screen with blinking cursor.
//   2. run_booting   — full boot sequence from DESIGN.md.
//   3. run_parked    — SYSTEM ONLINE parking screen with blinking cursor.
//
// Keypress polling uses uefi::system::with_stdin + read_key() (non-
// blocking, returns Ok(None) when no key is available) with a 50 ms
// stall between polls so the firmware's event loop is not starved.
//
// Timestamps are a monotonic millisecond counter accumulated from stall
// durations; good enough for Phase 6 serial drain without real RTC.

extern crate alloc;

use alloc::format;

use core::arch::x86_64::_rdtsc;
use core::time::Duration;

use crate::bootloader;
use crate::cursor::CursorState;
use crate::event::{Event, Phase, RingBuffer};
use crate::renderer::Renderer;
use crate::serial;

/// Read the x86 Time Stamp Counter.
///
/// Used exclusively for relative elapsed-time measurement (per-call
/// cost of `Scene::refresh_digest`). The TSC is calibrated once at
/// scene start against a known `uefi::boot::stall` duration to
/// recover `ticks_per_ms`; see `Scene::run`.
///
/// # Safety
/// `_rdtsc` is a single instruction with no memory side effects.
/// Marked `unsafe` by the compiler because it is a raw CPU
/// intrinsic; wrapped here so call sites can be safe.
#[inline]
fn read_tsc() -> u64 {
    // SAFETY: _rdtsc is a single-instruction read with no memory
    // side effects. Available on any x86-64 target (UEFI only runs
    // on x86-64 in this project).
    unsafe { _rdtsc() }
}

/// Per-call timing statistics for `Scene::refresh_digest`.
///
/// Accumulated across every call during a boot run and emitted as a
/// single summary line on the serial drain just before ACPI shutdown.
/// Provides the raw numbers needed to evaluate the phase-1 bail-out
/// criterion (total refresh wall-clock ≤ 5% of transcript duration).
pub(crate) struct RefreshStats {
    /// Number of `refresh_digest` calls recorded so far.
    pub(crate) count: u32,
    /// Sum of TSC tick deltas across all calls.
    pub(crate) total_ticks: u64,
    /// Maximum single-call TSC tick delta.
    pub(crate) max_ticks: u64,
    /// Ring of the most recent 256 per-call tick counts.
    pub(crate) sample_ring: [u64; 256],
    /// Next write index into `sample_ring` (wraps mod 256).
    sample_head: usize,
}

impl RefreshStats {
    const fn new() -> Self {
        Self {
            count: 0,
            total_ticks: 0,
            max_ticks: 0,
            sample_ring: [0u64; 256],
            sample_head: 0,
        }
    }

    /// Record one `refresh_digest` call that took `ticks` TSC counts.
    fn record(&mut self, ticks: u64) {
        self.count += 1;
        self.total_ticks = self.total_ticks.saturating_add(ticks);
        if ticks > self.max_ticks {
            self.max_ticks = ticks;
        }
        self.sample_ring[self.sample_head % 256] = ticks;
        self.sample_head += 1;
    }
}

/// Poll interval between read_key calls during cursor-blink loops.
pub(crate) const POLL_MS: u64 = 50;

/// Pacing delays in milliseconds.
pub(crate) const PACE_LINE_MS: u64 = 200; // after a normal line

/// On-screen toast lifetime after a mode switch.
const TOAST_MS: u64 = 1500;

/// Per-step dwell when cycling through every available
/// GOP mode (key '0').
const CYCLE_DWELL_MS: u64 = 1000;

/// Parking-screen prompt. Defined at module scope so `run_parked`
/// (which draws it) and `Scene::repaint` (which redraws it after a
/// runtime mode switch) cannot drift out of sync.
const SYSTEM_ONLINE_TEXT: &str = "SYSTEM ONLINE. AWAITING INSTRUCTIONS.";

/// Keystroke → (width, height) request for runtime mode
/// switches. Matches the master plan's documented binding
/// table. Key '0' is handled separately by the cycle path
/// (added in step 2c) and is therefore not in this table.
const MODE_KEYS: &[(char, u32, u32)] = &[
    ('1', 640, 480),
    ('2', 800, 600),
    ('3', 1024, 768),
    ('4', 1280, 720),
    ('5', 1280, 1024),
    ('6', 1920, 1080),
];

/// Stall for `ms` milliseconds and advance the monotonic clock.
///
/// Free function so both `Scene` and the locked-bootloader
/// sub-state-machine can share a single clock counter.
pub(crate) fn stall(clock_ms: &mut u64, ms: u64) {
    uefi::boot::stall(Duration::from_millis(ms));
    *clock_ms += ms;
}

/// Poll for a keypress once (non-blocking).
///
/// Returns `Some((unicode, scancode))` if a key was available,
/// `None` otherwise. `with_stdin` returns `Result<Option<Key>, _>`
/// directly (not wrapped in an outer Option as older docs suggested).
///
/// Free function so the locked-bootloader sub-state-machine can call
/// it without going through `Scene`.
pub(crate) fn poll_key() -> Option<(char, u16)> {
    use uefi::proto::console::text::Key;
    // with_stdin closure returns Result<Option<Key>, uefi::Error>.
    let result = uefi::system::with_stdin(|stdin| stdin.read_key());
    match result {
        Ok(Some(Key::Printable(c))) => Some((char::from(c), 0u16)),
        Ok(Some(Key::Special(sk))) => {
            // ScanCode is a transparent newtype over u16; access
            // the inner value via the public tuple-struct field.
            Some(('\0', sk.0))
        }
        _ => None,
    }
}

/// Steps in the boot-sequence script.
enum SceneStep<'a> {
    /// `draw_telemetry_line(label, status, row)`.
    Telemetry { label: &'a str, status: &'a str },
    /// `draw_line(text, row)` — full-width line, spaces preserved verbatim.
    Line(&'a str),
    /// Hybrid bitmap probe line: bitmap label, ASCII dot leader, bitmap status.
    /// Used for the language-probe opening beats where labels and statuses
    /// are not ASCII.
    Probe {
        label_bitmap: &'a [u8],
        label_width_px: usize,
        status_bitmap: &'a [u8],
        status_width_px: usize,
    },
}

/// First half of the boot-sequence script — everything from the language
/// probes through `SENSORIUM: nominal`. Plays before the locked-bootloader
/// sub-state-machine. Row assignment is done dynamically in `run_booting`.
static BOOT_SCRIPT_PRE: &[SceneStep<'static>] = &[
    // --- LANGUAGE PROBES ---
    // Worldbuilding opening beats: the system probes for language support
    // across several scripts and finds only English. Establishes that
    // English is no longer the default in this universe.
    SceneStep::Probe {
        label_bitmap: &crate::probes::PROBE_MANDARIN_LABEL_BITMAP,
        label_width_px: crate::probes::PROBE_MANDARIN_LABEL_WIDTH_PX,
        status_bitmap: &crate::probes::PROBE_MANDARIN_STATUS_BITMAP,
        status_width_px: crate::probes::PROBE_MANDARIN_STATUS_WIDTH_PX,
    },
    SceneStep::Probe {
        label_bitmap: &crate::probes::PROBE_HINDI_LABEL_BITMAP,
        label_width_px: crate::probes::PROBE_HINDI_LABEL_WIDTH_PX,
        status_bitmap: &crate::probes::PROBE_HINDI_STATUS_BITMAP,
        status_width_px: crate::probes::PROBE_HINDI_STATUS_WIDTH_PX,
    },
    // Spanish and English are Latin-script: rendered through the spleen
    // telemetry path so they match the rest of the boot transcript
    // visually. `castellano` is used in place of `español` to keep the
    // line ASCII (it is the formal name of the Spanish language and
    // appears in Spanish constitutional and official usage).
    SceneStep::Telemetry {
        label: "Detectando soporte para castellano",
        status: "FALLO",
    },
    SceneStep::Telemetry {
        label: "Probing for English support",
        status: "OK",
    },
    // --- REMOTE LINK ---
    SceneStep::Telemetry {
        label: "REMOTE LINK: serial @ COM2",
        status: "ACQUIRED",
    },
    // --- AWAITING OPERATOR transition line ---
    SceneStep::Telemetry {
        label: "AWAITING OPERATOR",
        status: "[connection confirmed]",
    },
    // --- VIDEO SUBSYSTEM ---
    SceneStep::Telemetry {
        label: "VIDEO SUBSYSTEM: direct hardware",
        status: "FAILED",
    },
    SceneStep::Line("                 supervised mode ..... OK"),
    SceneStep::Line("                 640x480x8 ........... OK"),
    SceneStep::Line("                 800x600x16 .......... OK"),
    SceneStep::Line("                 1024x768x32 ......... OK"),
    SceneStep::Line("                 mode locked:          1024x768x32"),
    // --- POINTER ---
    SceneStep::Telemetry {
        label: "POINTER: sensing",
        status: "OK",
    },
    // --- KEYBOARD ---
    SceneStep::Telemetry {
        label: "KEYBOARD: enumerate",
        status: "OK",
    },
    // --- AUDIO DAC ---
    SceneStep::Telemetry {
        label: "AUDIO DAC: 44.1 kHz sine",
        status: "FAILED",
    },
    // --- CLIPBOARD RELAY ---
    SceneStep::Telemetry {
        label: "CLIPBOARD RELAY: handshake",
        status: "OK",
    },
    // --- STORAGE ---
    SceneStep::Telemetry {
        label: "STORAGE: local media",
        status: "NONE",
    },
    // --- THRUSTER CONTROL ---
    SceneStep::Telemetry {
        label: "THRUSTER CONTROL: self-test",
        status: "FAILED",
    },
    // --- SENSORIUM ---
    SceneStep::Line("SENSORIUM: nominal"),
];

/// Second half of the boot-sequence script — only the post-bootloader
/// `EMERGENCY SAFE BOOT COMPLETE` line. Plays after the locked-bootloader
/// sub-state-machine returns successfully. The line is rendered on the
/// row the bootloader's `Continue` outcome reports as `next_row`.
static BOOT_SCRIPT_POST: &[SceneStep<'static>] = &[
    // --- BOOT COMPLETE ---
    SceneStep::Line("EMERGENCY SAFE BOOT COMPLETE. OPERATOR ASSISTANCE REQUIRED."),
];

/// Per-instance toast tracking. Toast text is not stored
/// once drawn — only the remaining TTL matters for cleanup.
#[derive(Copy, Clone, Debug)]
struct ToastState {
    remaining_ms: u64,
}

/// Snapshot of what is currently on screen, sufficient for
/// `Scene::repaint` to reconstruct the visible content
/// after a runtime mode switch.
///
/// Add one variant per scene runner. Carry just enough
/// state — script index, row, etc. — for the repainter to
/// replay the correct prefix; resist storing data that
/// repaint does not actually consume. Variant fields drift
/// fastest; keep them lean.
///
/// Every variant carries the indices and rows the repainter needs to
/// replay just the right prefix of the boot scripts. Repaint never
/// pushes ring-buffer events and never advances `clock_ms`: those
/// describe the *original* render and are not re-emitted on a
/// runtime mode-switch redraw.
///
/// `BootingBootloader` exists for totality. The locked-bootloader
/// scene is carved out from receiving mode keystrokes (per the master
/// plan) so a repaint should never trigger from that state in
/// production. Were a future scene to adopt the same input shape and
/// hit this path, repainting PRE only — leaving the bootloader's own
/// rows blank — is the safe fallback: the bootloader owns its own
/// drawing and a runtime mode switch from outside cannot reconstruct
/// its sub-state-machine.
#[derive(Copy, Clone, Debug)]
enum RepaintState {
    /// Initial state before `run_awaiting` starts: only the chrome
    /// (logo) has been painted.
    Chrome,
    /// `run_awaiting` is active: chrome + a blinking cursor at (0,0).
    /// The next blink tick repaints the cursor naturally.
    Awaiting,
    /// `run_booting` is mid-PRE: `played` PRE steps have been drawn
    /// to rows `[0, played)`.
    BootingPre { played: usize },
    /// `run_booting` has handed off to the locked-bootloader
    /// sub-state-machine. Mode keys are ignored there; if a repaint
    /// happens anyway, draw PRE only.
    BootingBootloader { pre_played: usize },
    /// `run_booting` is mid-POST: PRE is fully drawn at rows
    /// `[0, pre_played)`, the bootloader returned `bootloader_next_row`,
    /// and `post_played` POST steps have been drawn at rows
    /// `[bootloader_next_row, bootloader_next_row + post_played)`.
    BootingPost {
        pre_played: usize,
        bootloader_next_row: usize,
        post_played: usize,
    },
    /// `run_parked` is active: PRE + POST are drawn, plus the
    /// SYSTEM ONLINE row at `system_online_row`. The blinking
    /// cursor at the end of that row is repainted by the next tick.
    Parked {
        pre_played: usize,
        bootloader_next_row: usize,
        post_played: usize,
        system_online_row: usize,
    },
}

/// Scene orchestrator: owns the ring buffer, cursor state, and clock.
pub struct Scene {
    phase: Phase,
    ring: RingBuffer<256>,
    cursor: CursorState,
    /// Monotonic millisecond counter accumulated from stall durations.
    clock_ms: u64,
    /// Snapshot of what is currently on screen. Updated by the scene
    /// runners as they play; consulted by `Scene::repaint`.
    repaint_state: RepaintState,
    /// Active on-screen toast state; `None` when no toast is visible.
    toast: Option<ToastState>,
    /// Monotonic per-boot counter for the on-screen visual digest.
    /// Starts at `0`; the first `refresh_digest` call increments to `1`.
    /// Wraps at `u32::MAX` (136 years at 1 Hz — not a concern).
    digest_frame_counter: u32,
    /// TSC ticks per millisecond, calibrated once at `Scene::run` entry.
    /// Zero until calibration completes; `refresh_digest` uses it to
    /// convert tick deltas to microseconds for the serial drain summary.
    ticks_per_ms: u64,
    /// Per-call timing statistics for `refresh_digest`. Accumulated
    /// across the full boot run; emitted by `serial::drain`.
    refresh_stats: RefreshStats,
}

impl Scene {
    /// Create a new scene in the Awaiting phase.
    pub fn new() -> Self {
        Self {
            phase: Phase::Awaiting,
            ring: RingBuffer::new(),
            cursor: CursorState::new(),
            clock_ms: 0,
            repaint_state: RepaintState::Chrome,
            toast: None,
            digest_frame_counter: 0,
            ticks_per_ms: 0,
            refresh_stats: RefreshStats::new(),
        }
    }

    /// Run the full scene to completion, then ACPI-shutdown.
    ///
    /// Never returns; ACPI shutdown exits the process.
    pub fn run(&mut self, renderer: &mut Renderer) -> ! {
        // Calibrate TSC: read before and after a known 100 ms stall to
        // recover ticks-per-millisecond for the refresh-cost summary.
        // Stall is tight (no key polling) so the only elapsed time is
        // the firmware's `stall` call itself.
        let tsc_before = read_tsc();
        uefi::boot::stall(Duration::from_millis(100));
        let tsc_after = read_tsc();
        self.ticks_per_ms = (tsc_after.saturating_sub(tsc_before)) / 100;

        Self::draw_chrome(renderer);
        self.run_awaiting(renderer);
        self.refresh_digest(renderer);

        let next_row = self.run_booting(renderer);
        self.refresh_digest(renderer);

        self.run_parked(renderer, next_row);
        self.refresh_digest(renderer);

        serial::drain(&self.ring, &self.refresh_stats, self.ticks_per_ms);

        uefi::runtime::reset(
            uefi::runtime::ResetType::SHUTDOWN,
            uefi::Status::SUCCESS,
            None,
        )
    }

    // ----------------------------------------------------------------
    // Internal helpers
    // ----------------------------------------------------------------

    /// Paint static screen chrome: the Shaken Fist logo in the top-
    /// right corner. Called once before AWAITING and again after
    /// `run_booting` clears the screen for its fresh transcript.
    fn draw_chrome(renderer: &mut Renderer) {
        use crate::logo::{LOGO_BITMAP, LOGO_HEIGHT, LOGO_WIDTH};
        use crate::renderer::CELL_W;
        let logo_cols = LOGO_WIDTH / CELL_W;
        let col = renderer.screen_cols().saturating_sub(logo_cols);
        renderer.draw_text_bitmap(&LOGO_BITMAP, LOGO_WIDTH, LOGO_HEIGHT, col, 0);
    }

    /// Draw (or erase) the cursor at a given text cell.
    ///
    /// `col` and `row` are text-cell coordinates. If `glyph_bytes` is
    /// `Some`, draws the glyph; if `None`, clears the cell to background.
    fn draw_or_clear_cursor(
        renderer: &mut Renderer,
        glyph_bytes: Option<[u8; 16]>,
        col: usize,
        row: usize,
    ) {
        match glyph_bytes {
            Some(ref bytes) => renderer.draw_cursor_glyph(bytes, col, row),
            None => renderer.clear_cell(col, row),
        }
    }

    // ----------------------------------------------------------------
    // Phase: Awaiting
    // ----------------------------------------------------------------

    /// Show a lone blinking cursor and wait for the first keypress.
    ///
    /// No text is drawn here: the system has not yet probed for language
    /// support, so it cannot prompt in any specific language. The cursor
    /// sits at column 0, row 0 — a fresh, language-neutral "ready"
    /// signal. The language probes that open `run_booting` are what
    /// establish (in worldbuilding terms) which language the rest of
    /// the transcript is allowed to use.
    fn run_awaiting(&mut self, renderer: &mut Renderer) {
        const CURSOR_COL: usize = 0;
        const CURSOR_ROW: usize = 0;

        self.repaint_state = RepaintState::Awaiting;

        // Paint the digest before blocking on the first keypress so the
        // AWAITING screen carries a QR. The outer-loop `refresh_digest`
        // call sites in `Scene::run` fire at phase boundaries after the
        // runners return, which is too late for the AWAITING screen
        // itself — `blink_until_key` below holds here indefinitely.
        self.refresh_digest(renderer);

        self.blink_until_key(renderer, CURSOR_COL, CURSOR_ROW, |scene| {
            scene.ring.push(Event::SceneTransition {
                from: Phase::Awaiting,
                to: Phase::Booting,
                timestamp_ms: scene.clock_ms,
            });
            scene.phase = Phase::Booting;
        });
    }

    // ----------------------------------------------------------------
    // Phase: Booting
    // ----------------------------------------------------------------

    /// Walk the boot-sequence script and render each line with pacing.
    ///
    /// Splits the boot transcript around the locked-bootloader
    /// sub-state-machine: plays `BOOT_SCRIPT_PRE` (everything up to and
    /// including `SENSORIUM: nominal`), then hands control to
    /// `bootloader::run`, then plays `BOOT_SCRIPT_POST` starting at the
    /// row the bootloader's `Continue` outcome reports as `next_row`.
    ///
    /// Returns the first row *after* the last rendered line, so the
    /// parking screen can append its prompt without clearing.
    fn run_booting(&mut self, renderer: &mut Renderer) -> usize {
        // Clear the screen so the boot sequence starts fresh, then
        // repaint the logo (which the clear wiped).
        renderer.clear();
        Self::draw_chrome(renderer);
        self.repaint_state = RepaintState::BootingPre { played: 0 };

        // Start rendering at row 0; each line or telemetry entry
        // advances the row counter by 1.
        let mut row: usize = 0;

        // PRE: update repaint_state after each step so a mode-switch
        // mid-script can replay exactly the right prefix.
        row = self.play_script(renderer, BOOT_SCRIPT_PRE, row, &mut |played| {
            RepaintState::BootingPre { played }
        });

        let pre_played = BOOT_SCRIPT_PRE.len();
        self.repaint_state = RepaintState::BootingBootloader { pre_played };

        // Hand off to the locked-bootloader sub-state-machine. It
        // takes shared mutable references to the renderer, the ring
        // buffer, and the monotonic clock so its events and timing
        // land on the same timeline as the rest of the scene.
        let bootloader::BootloaderOutcome::Continue { next_row } =
            bootloader::run(renderer, &mut self.ring, &mut self.clock_ms, row);
        row = next_row;
        let bootloader_next_row = next_row;

        self.repaint_state = RepaintState::BootingPost {
            pre_played,
            bootloader_next_row,
            post_played: 0,
        };

        // POST: same pattern as PRE — track played count for repaint.
        row = self.play_script(renderer, BOOT_SCRIPT_POST, row, &mut |post_played| {
            RepaintState::BootingPost {
                pre_played,
                bootloader_next_row,
                post_played,
            }
        });

        self.ring.push(Event::SceneTransition {
            from: Phase::Booting,
            to: Phase::Parked,
            timestamp_ms: self.clock_ms,
        });
        self.phase = Phase::Parked;
        row
    }

    /// Render a slice of `SceneStep`s starting at `start_row`, with the
    /// shared `PACE_LINE_MS` pacing between each line. Returns the next
    /// free row.
    ///
    /// `make_state` is called after each step has been drawn (and the
    /// `LineRendered` event pushed) with the running count of completed
    /// steps; the returned `RepaintState` is stored on the scene. This
    /// keeps `repaint_state` accurate to the on-screen content even if
    /// a future caller invokes `Scene::repaint` between steps.
    fn play_script(
        &mut self,
        renderer: &mut Renderer,
        script: &[SceneStep<'static>],
        start_row: usize,
        make_state: &mut dyn FnMut(usize) -> RepaintState,
    ) -> usize {
        let mut row = start_row;
        for (idx, step) in script.iter().enumerate() {
            match step {
                SceneStep::Telemetry { label, status } => {
                    renderer.draw_telemetry_line(label, status, row);
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.repaint_state = make_state(idx + 1);
                    self.refresh_digest(renderer);
                    self.stall_with_keys(renderer, PACE_LINE_MS);
                }
                SceneStep::Line(text) => {
                    renderer.draw_line(text, row);
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.repaint_state = make_state(idx + 1);
                    self.refresh_digest(renderer);
                    self.stall_with_keys(renderer, PACE_LINE_MS);
                }
                SceneStep::Probe {
                    label_bitmap,
                    label_width_px,
                    status_bitmap,
                    status_width_px,
                } => {
                    renderer.draw_probe_line(
                        label_bitmap,
                        *label_width_px,
                        status_bitmap,
                        *status_width_px,
                        row,
                    );
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.repaint_state = make_state(idx + 1);
                    self.refresh_digest(renderer);
                    self.stall_with_keys(renderer, PACE_LINE_MS);
                }
            }
        }
        row
    }

    /// Replay a script prefix without pacing, ring-buffer pushes, or
    /// `repaint_state` updates. Used by `Scene::repaint` to reconstruct
    /// the visible state of the screen after a framebuffer-invalidating
    /// mode switch.
    ///
    /// Mirrors the per-step rendering in `play_script` exactly so the
    /// repainted output is pixel-identical to the original draw. The
    /// `count` argument bounds how many steps from `script` are
    /// replayed; rows are assigned as `start_row..start_row + count`.
    fn repaint_script_prefix(
        renderer: &mut Renderer,
        script: &[SceneStep<'static>],
        start_row: usize,
        count: usize,
    ) {
        let mut row = start_row;
        let limit = count.min(script.len());
        for step in &script[..limit] {
            match step {
                SceneStep::Telemetry { label, status } => {
                    renderer.draw_telemetry_line(label, status, row);
                }
                SceneStep::Line(text) => {
                    renderer.draw_line(text, row);
                }
                SceneStep::Probe {
                    label_bitmap,
                    label_width_px,
                    status_bitmap,
                    status_width_px,
                } => {
                    renderer.draw_probe_line(
                        label_bitmap,
                        *label_width_px,
                        status_bitmap,
                        *status_width_px,
                        row,
                    );
                }
            }
            row += 1;
        }
    }

    /// Repaint everything currently on screen at the renderer's
    /// current dimensions.
    ///
    /// Phase 1's keystroke-dispatcher hook: after a runtime
    /// `Renderer::set_mode`, the framebuffer is invalidated (UEFI 2.10
    /// §12.9), and the dispatcher (Phase 2) calls this to reconstruct
    /// what the operator was looking at. Read-only from the timeline's
    /// perspective: no `LineRendered` events are pushed, no `clock_ms`
    /// stalls, no recursion into `Scene::run` or the runner methods.
    /// Safe to call from inside a runner's poll loop.
    ///
    /// The repaint reads `self.repaint_state` and replays just the
    /// prefix of the boot scripts the snapshot describes. Cursor state
    /// during awaiting / parked is left to the next blink tick.
    fn repaint(&mut self, renderer: &mut Renderer) {
        // Honour the framebuffer-invalidation contract.
        renderer.clear();
        Self::draw_chrome(renderer);

        match self.repaint_state {
            RepaintState::Chrome | RepaintState::Awaiting => {
                // Chrome is already drawn; awaiting's cursor is
                // restored by the next blink tick.
            }
            RepaintState::BootingPre { played } => {
                Self::repaint_script_prefix(renderer, BOOT_SCRIPT_PRE, 0, played);
            }
            RepaintState::BootingBootloader { pre_played } => {
                // Carve-out fallback: the bootloader owns its own
                // rows and no external state describes them. PRE only.
                Self::repaint_script_prefix(renderer, BOOT_SCRIPT_PRE, 0, pre_played);
            }
            RepaintState::BootingPost {
                pre_played,
                bootloader_next_row,
                post_played,
            } => {
                Self::repaint_script_prefix(renderer, BOOT_SCRIPT_PRE, 0, pre_played);
                Self::repaint_script_prefix(
                    renderer,
                    BOOT_SCRIPT_POST,
                    bootloader_next_row,
                    post_played,
                );
            }
            RepaintState::Parked {
                pre_played,
                bootloader_next_row,
                post_played,
                system_online_row,
            } => {
                Self::repaint_script_prefix(renderer, BOOT_SCRIPT_PRE, 0, pre_played);
                Self::repaint_script_prefix(
                    renderer,
                    BOOT_SCRIPT_POST,
                    bootloader_next_row,
                    post_played,
                );
                renderer.draw_line(SYSTEM_ONLINE_TEXT, system_online_row);
            }
        }

        // Restore the digest QR after a framebuffer wipe.
        self.refresh_digest(renderer);
    }

    /// Compute and render the on-screen digest reflecting the
    /// current ring-buffer state. Called at scene-phase boundaries
    /// from the outer scene loop (`Scene::run`), after each painted
    /// line in the boot transcript, on cursor blink transitions, and
    /// at explicit refresh points inside `src/bootloader.rs`.
    ///
    /// The bootloader sub-state-machine places its own refresh calls
    /// after each visible state change; `Scene::repaint` during the
    /// bootloader scene falls back to PRE-only reconstruction (the
    /// bootloader's row content is not reconstructable from
    /// `RepaintState`) and `refresh_digest` participates normally.
    ///
    /// Hash path A: reads the framebuffer back via
    /// `BltOp::VideoToBltBuffer` and CRC32Cs the bytes outside the
    /// right-anchored digest region. Picked over path B (per-paint
    /// incremental hash) by 2c-measure: path A concentrates ~21.5M
    /// cycles per call (~7 ms at 3 GHz) into the named refresh sites
    /// instead of leaking hash overhead into every paint site forever,
    /// and it implicitly exercises the SPICE display's read-back path
    /// which the project otherwise never touches. See
    /// `PLAN-visual-digest-phase-02-payload.md` *Outcome* for the full
    /// A/B numbers and rationale.
    fn refresh_digest(&mut self, renderer: &mut Renderer) {
        let tsc_start = read_tsc();

        self.digest_frame_counter = self.digest_frame_counter.wrapping_add(1);

        // Path A: read the framebuffer back via
        // `BltOp::VideoToBltBuffer` and CRC32C every pixel byte
        // outside the digest region. Excluding the digest region
        // avoids the self-referencing-hash trap (the QR encodes a
        // hash of everything-not-itself).
        let framebuffer_hash = renderer.crc32c_framebuffer_excluding_digest();

        let mut buf = [0u8; crate::digest::DIGEST_PAYLOAD_CAPACITY];
        match crate::digest::encode(
            &self.ring,
            self.digest_frame_counter,
            framebuffer_hash,
            &mut buf,
        ) {
            Ok(len) => renderer.draw_digest(&buf[..len]),
            Err(_) => {
                // Encoder errors are programmer bugs at this point —
                // the buffer is sized exactly to capacity. Skip refresh
                // rather than crash; phase-3 closeout will surface this
                // via a log.
            }
        }

        let tsc_end = read_tsc();
        self.refresh_stats.record(tsc_end.saturating_sub(tsc_start));
    }

    /// Draw a toast on the bottom row naming the applied mode.
    /// Format depends on whether the firmware substituted:
    ///   exact       -> "mode 1024x768"
    ///   substitute  -> "requested 1280x720 -> using 1024x768"
    ///
    /// ASCII only — the renderer's font is ASCII (any non-ASCII
    /// character renders as `?`). Stores `Some(ToastState { ... })`
    /// on `self.toast` so `tick_toast` can clear it later.
    fn draw_toast(&mut self, renderer: &mut Renderer, requested: (u32, u32), applied: (u32, u32)) {
        let row = renderer.screen_rows().saturating_sub(1);
        let text = if requested == applied {
            format!("mode {}x{}", applied.0, applied.1)
        } else {
            format!(
                "requested {}x{} -> using {}x{}",
                requested.0, requested.1, applied.0, applied.1
            )
        };
        renderer.draw_text_at(&text, 0, row);
        self.toast = Some(ToastState {
            remaining_ms: TOAST_MS,
        });
    }

    /// Tick the toast TTL by `dt_ms`. If the TTL elapses, clear
    /// the toast by repainting the entire scene (cheap, and the
    /// canonical way to undo any partial-row state).
    fn tick_toast(&mut self, renderer: &mut Renderer, dt_ms: u64) {
        if let Some(state) = self.toast.as_mut() {
            if state.remaining_ms <= dt_ms {
                self.toast = None;
                self.repaint(renderer);
            } else {
                state.remaining_ms -= dt_ms;
            }
        }
    }

    /// Run a cursor-blink poll loop until a non-mode keystroke
    /// arrives. The cursor blinks at `(cursor_col, cursor_row)`;
    /// each tick polls keys, dispatches mode keys via
    /// `try_handle_mode_key` (which the loop calls in a way
    /// that respects the locked-bootloader carve-out — this
    /// helper is not invoked from src/bootloader.rs), and ticks
    /// the toast TTL.
    ///
    /// On a non-mode keystroke, pushes a `Keypress` event and
    /// then invokes `on_exit(self)`. The closure is responsible
    /// for any phase transition and final event pushes (e.g.
    /// `SceneTransition`); after it returns, the helper
    /// returns.
    fn blink_until_key<F: FnOnce(&mut Self)>(
        &mut self,
        renderer: &mut Renderer,
        cursor_col: usize,
        cursor_row: usize,
        on_exit: F,
    ) {
        // Initialised to `None` so the first iteration always
        // refreshes: the very first `Some([0xFF; 16])` (cursor On)
        // differs from `None` and re-encodes the QR against the
        // freshly-painted cursor.
        let mut last_glyph: Option<[u8; 16]> = None;
        loop {
            let glyph = self.cursor.tick(POLL_MS);
            Self::draw_or_clear_cursor(renderer, glyph, cursor_col, cursor_row);
            if glyph != last_glyph {
                self.refresh_digest(renderer);
                last_glyph = glyph;
            }
            self.tick_toast(renderer, POLL_MS);
            stall(&mut self.clock_ms, POLL_MS);

            if let Some((ch, sc)) = poll_key() {
                self.ring.push(Event::Keypress {
                    unicode: ch,
                    scancode: sc,
                    timestamp_ms: self.clock_ms,
                });

                if self.try_handle_mode_key(renderer, ch) {
                    continue;
                }

                on_exit(self);
                return;
            }
        }
    }

    /// Walk every available GOP mode, dwelling
    /// `CYCLE_DWELL_MS` per step. Interruptible by any
    /// keypress: the cycle stops, the binary settles at the
    /// most recent mode, and a single `ModeCycle` event is
    /// pushed naming `count` (modes switched) and
    /// `interrupted` (whether the walk completed naturally).
    ///
    /// If the interrupting key is itself a mode key (`'1'`-
    /// `'6'` or `'0'`), it is honoured by recursing into
    /// `try_handle_mode_key`. Other keys are discarded.
    fn cycle_modes(&mut self, renderer: &mut Renderer) {
        let modes = renderer.available_modes();
        let mut count: u32 = 0;
        let mut interrupted_by: Option<char> = None;

        'outer: for (w, h) in modes {
            // Apply this step.
            let (applied_w, applied_h) = renderer.set_mode(w, h);
            self.ring.push(Event::ModeSwitch {
                requested_w: w as u32,
                requested_h: h as u32,
                applied_w: applied_w as u32,
                applied_h: applied_h as u32,
                timestamp_ms: self.clock_ms,
            });
            self.repaint(renderer);
            self.draw_toast(
                renderer,
                (w as u32, h as u32),
                (applied_w as u32, applied_h as u32),
            );
            count += 1;

            // Dwell with key polling.
            let mut elapsed: u64 = 0;
            while elapsed < CYCLE_DWELL_MS {
                if let Some((ch, sc)) = poll_key() {
                    self.ring.push(Event::Keypress {
                        unicode: ch,
                        scancode: sc,
                        timestamp_ms: self.clock_ms,
                    });
                    interrupted_by = Some(ch);
                    break 'outer;
                }
                self.tick_toast(renderer, POLL_MS);
                stall(&mut self.clock_ms, POLL_MS);
                elapsed += POLL_MS;
            }
        }

        self.ring.push(Event::ModeCycle {
            count,
            interrupted: interrupted_by.is_some(),
            timestamp_ms: self.clock_ms,
        });

        // Honour the interrupting key if it was a mode key.
        if let Some(ch) = interrupted_by {
            let _ = self.try_handle_mode_key(renderer, ch);
        }
    }

    /// Stall for `total_ms` while polling for keystrokes. Mode
    /// keys are dispatched via `try_handle_mode_key`; non-mode
    /// keys are discarded (a small behaviour change from the
    /// pre-Phase-2 binary, where non-mode keys would sit in the
    /// firmware queue and be drained by `run_parked`'s blink
    /// loop). Toast TTL is ticked each chunk.
    ///
    /// The total stall budget is fixed: dispatching a mode key
    /// mid-stall does not reset the budget. Wall-clock time
    /// spent in `try_handle_mode_key` is *not* deducted from
    /// the budget either — the stall accounts only for `stall`
    /// calls within its own loop.
    fn stall_with_keys(&mut self, renderer: &mut Renderer, total_ms: u64) {
        let mut elapsed: u64 = 0;
        while elapsed < total_ms {
            let chunk = POLL_MS.min(total_ms - elapsed);
            if let Some((ch, sc)) = poll_key() {
                self.ring.push(Event::Keypress {
                    unicode: ch,
                    scancode: sc,
                    timestamp_ms: self.clock_ms,
                });
                // Returns false for non-mode keys — discard.
                let _ = self.try_handle_mode_key(renderer, ch);
            }
            self.tick_toast(renderer, chunk);
            stall(&mut self.clock_ms, chunk);
            elapsed += chunk;
        }
    }

    /// Try to handle a keystroke as a mode-switch request.
    /// Returns `true` if the keystroke was a mode key and was
    /// consumed; `false` if the caller should handle it as
    /// usual.
    ///
    /// Mode-switch path: look up `ch` in `MODE_KEYS`, call
    /// `Renderer::set_mode` with the requested dimensions,
    /// push a `ModeSwitch` event with both the request and
    /// the queried-back applied resolution, and call
    /// `Scene::repaint` so the framebuffer (invalidated by
    /// `set_mode` per UEFI 2.10 §12.9) shows the current
    /// scene state at the new dimensions. Then draws a toast
    /// on the bottom row naming the applied resolution.
    ///
    /// Key '0' delegates to `cycle_modes`, which walks every
    /// available GOP mode with a `CYCLE_DWELL_MS` dwell per
    /// step and is interruptible by any keypress.
    fn try_handle_mode_key(&mut self, renderer: &mut Renderer, ch: char) -> bool {
        if ch == '0' {
            self.cycle_modes(renderer);
            return true;
        }
        let Some(&(_, req_w, req_h)) = MODE_KEYS.iter().find(|(c, _, _)| *c == ch) else {
            return false;
        };
        let (applied_w, applied_h) = renderer.set_mode(req_w as usize, req_h as usize);
        self.ring.push(Event::ModeSwitch {
            requested_w: req_w,
            requested_h: req_h,
            applied_w: applied_w as u32,
            applied_h: applied_h as u32,
            timestamp_ms: self.clock_ms,
        });
        self.repaint(renderer);
        self.draw_toast(
            renderer,
            (req_w, req_h),
            (applied_w as u32, applied_h as u32),
        );
        true
    }

    // ----------------------------------------------------------------
    // Phase: Parked
    // ----------------------------------------------------------------

    /// Append a SYSTEM ONLINE prompt after the boot sequence and blink a
    /// cursor until the operator presses a key.
    ///
    /// Deliberately does NOT clear the screen — the boot-sequence log
    /// stays visible so the operator can read the whole transcript.
    /// The prompt lands one blank row below the final boot-sequence line.
    /// After returning, the caller issues ACPI shutdown.
    fn run_parked(&mut self, renderer: &mut Renderer, start_row: usize) {
        let text_row = start_row + 1; // leave one blank row after BOOT COMPLETE
        let cursor_col: usize = SYSTEM_ONLINE_TEXT.len() + 1;
        let cursor_row: usize = text_row;

        renderer.draw_line(SYSTEM_ONLINE_TEXT, text_row);

        // Capture the full repaint snapshot for the parked screen.
        // The previous repaint_state — set at the end of `play_script`
        // for POST — already carries the right `pre_played`,
        // `bootloader_next_row`, and `post_played`; lift them and add
        // the SYSTEM ONLINE row.
        let (pre_played, bootloader_next_row, post_played) = match self.repaint_state {
            RepaintState::BootingPost {
                pre_played,
                bootloader_next_row,
                post_played,
            } => (pre_played, bootloader_next_row, post_played),
            // Defensive: if the previous state was not BootingPost
            // (e.g. some future caller jumps straight here), fall
            // back to the script lengths so the snapshot is at least
            // self-consistent.
            _ => (
                BOOT_SCRIPT_PRE.len(),
                start_row.saturating_sub(BOOT_SCRIPT_POST.len()),
                BOOT_SCRIPT_POST.len(),
            ),
        };
        self.repaint_state = RepaintState::Parked {
            pre_played,
            bootloader_next_row,
            post_played,
            system_online_row: text_row,
        };

        self.refresh_digest(renderer);

        // Reuse the existing cursor state to preserve blink/glitch
        // counter continuity from the AWAITING screen.
        self.blink_until_key(renderer, cursor_col, cursor_row, |scene| {
            // Non-mode key — Parked → Parked transition signals
            // the final keypress that exits the parking loop.
            scene.ring.push(Event::SceneTransition {
                from: Phase::Parked,
                to: Phase::Parked,
                timestamp_ms: scene.clock_ms,
            });
        });
    }
}
