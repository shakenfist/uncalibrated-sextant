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

use core::time::Duration;

use crate::bootloader;
use crate::cursor::CursorState;
use crate::event::{Event, Phase, RingBuffer};
use crate::renderer::Renderer;
use crate::serial;

/// Poll interval between read_key calls during cursor-blink loops.
pub(crate) const POLL_MS: u64 = 50;

/// Pacing delays in milliseconds.
const PACE_LINE_MS: u64 = 200; // after a normal line

/// On-screen toast lifetime after a mode switch.
const TOAST_MS: u64 = 1500;

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

/// Snapshot of what is currently on screen, sufficient to repaint from
/// scratch at the renderer's current dimensions. Updated by the scene
/// runners as they play.
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
        }
    }

    /// Run the full scene to completion, then ACPI-shutdown.
    ///
    /// Never returns; ACPI shutdown exits the process.
    pub fn run(&mut self, renderer: &mut Renderer) -> ! {
        Self::draw_chrome(renderer);
        self.run_awaiting(renderer);
        let next_row = self.run_booting(renderer);
        self.run_parked(renderer, next_row);

        serial::drain(&self.ring);

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

        loop {
            // Advance cursor state by one poll interval.
            let glyph = self.cursor.tick(POLL_MS);
            Self::draw_or_clear_cursor(renderer, glyph, CURSOR_COL, CURSOR_ROW);
            stall(&mut self.clock_ms, POLL_MS);

            if let Some((ch, sc)) = poll_key() {
                self.ring.push(Event::Keypress {
                    unicode: ch,
                    scancode: sc,
                    timestamp_ms: self.clock_ms,
                });
                self.ring.push(Event::SceneTransition {
                    from: Phase::Awaiting,
                    to: Phase::Booting,
                    timestamp_ms: self.clock_ms,
                });
                self.phase = Phase::Booting;
                return;
            }
        }
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
                    stall(&mut self.clock_ms, PACE_LINE_MS);
                }
                SceneStep::Line(text) => {
                    renderer.draw_line(text, row);
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.repaint_state = make_state(idx + 1);
                    stall(&mut self.clock_ms, PACE_LINE_MS);
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
                    stall(&mut self.clock_ms, PACE_LINE_MS);
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
    #[allow(dead_code)] // Wired into runner blink loops in step 2d.
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
    /// Key '0' (cycle) is handled in step 2c. For now this
    /// dispatcher returns `false` for '0', so the caller
    /// continues to treat it as a non-mode key.
    #[allow(dead_code)] // Wired into runner sites in step 2d.
    pub(crate) fn try_handle_mode_key(&mut self, renderer: &mut Renderer, ch: char) -> bool {
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

        // Reuse the existing cursor state to preserve blink/glitch
        // counter continuity from the AWAITING screen.
        loop {
            let glyph = self.cursor.tick(POLL_MS);
            Self::draw_or_clear_cursor(renderer, glyph, cursor_col, cursor_row);
            stall(&mut self.clock_ms, POLL_MS);

            if let Some((ch, sc)) = poll_key() {
                self.ring.push(Event::Keypress {
                    unicode: ch,
                    scancode: sc,
                    timestamp_ms: self.clock_ms,
                });
                // Parked → Parked transition signals the final keypress.
                self.ring.push(Event::SceneTransition {
                    from: Phase::Parked,
                    to: Phase::Parked,
                    timestamp_ms: self.clock_ms,
                });
                return;
            }
        }
    }
}
