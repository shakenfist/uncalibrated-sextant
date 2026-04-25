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

use core::time::Duration;

use crate::cursor::CursorState;
use crate::event::{Event, Phase, RingBuffer};
use crate::renderer::Renderer;
use crate::serial;

/// Poll interval between read_key calls during cursor-blink loops.
const POLL_MS: u64 = 50;

/// Pacing delays in milliseconds.
const PACE_LINE_MS: u64 = 200; // after a normal line

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

/// The boot-sequence script, in order.
/// Row assignment is done dynamically in `run_booting`.
static BOOT_SCRIPT: &[SceneStep<'static>] = &[
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
    // --- BOOT COMPLETE ---
    SceneStep::Line("EMERGENCY SAFE BOOT COMPLETE. OPERATOR ASSISTANCE REQUIRED."),
];

/// Scene orchestrator: owns the ring buffer, cursor state, and clock.
pub struct Scene {
    phase: Phase,
    ring: RingBuffer<256>,
    cursor: CursorState,
    /// Monotonic millisecond counter accumulated from stall durations.
    clock_ms: u64,
}

impl Scene {
    /// Create a new scene in the Awaiting phase.
    pub fn new() -> Self {
        Self {
            phase: Phase::Awaiting,
            ring: RingBuffer::new(),
            cursor: CursorState::new(),
            clock_ms: 0,
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

    /// Stall for `ms` milliseconds and advance the monotonic clock.
    fn stall(&mut self, ms: u64) {
        uefi::boot::stall(Duration::from_millis(ms));
        self.clock_ms += ms;
    }

    /// Poll for a keypress once (non-blocking).
    ///
    /// Returns `Some((unicode, scancode))` if a key was available,
    /// `None` otherwise. `with_stdin` returns `Result<Option<Key>, _>`
    /// directly (not wrapped in an outer Option as older docs suggested).
    fn poll_key(&self) -> Option<(char, u16)> {
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

        loop {
            // Advance cursor state by one poll interval.
            let glyph = self.cursor.tick(POLL_MS);
            Self::draw_or_clear_cursor(renderer, glyph, CURSOR_COL, CURSOR_ROW);
            self.stall(POLL_MS);

            if let Some((ch, sc)) = self.poll_key() {
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
    /// Returns the first row *after* the last rendered line, so the
    /// parking screen can append its prompt without clearing.
    fn run_booting(&mut self, renderer: &mut Renderer) -> usize {
        // Clear the screen so the boot sequence starts fresh, then
        // repaint the logo (which the clear wiped).
        renderer.clear();
        Self::draw_chrome(renderer);

        // Start rendering at row 0; each line or telemetry entry
        // advances the row counter by 1.
        let mut row: usize = 0;

        for step in BOOT_SCRIPT {
            match step {
                SceneStep::Telemetry { label, status } => {
                    renderer.draw_telemetry_line(label, status, row);
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.stall(PACE_LINE_MS);
                }
                SceneStep::Line(text) => {
                    renderer.draw_line(text, row);
                    self.ring.push(Event::LineRendered {
                        row,
                        timestamp_ms: self.clock_ms,
                    });
                    row += 1;
                    self.stall(PACE_LINE_MS);
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
                    self.stall(PACE_LINE_MS);
                }
            }
        }

        self.ring.push(Event::SceneTransition {
            from: Phase::Booting,
            to: Phase::Parked,
            timestamp_ms: self.clock_ms,
        });
        self.phase = Phase::Parked;
        row
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
        const TEXT: &str = "SYSTEM ONLINE. AWAITING INSTRUCTIONS.";
        let text_row = start_row + 1; // leave one blank row after BOOT COMPLETE
        let cursor_col: usize = TEXT.len() + 1;
        let cursor_row: usize = text_row;

        renderer.draw_line(TEXT, text_row);

        // Reuse the existing cursor state to preserve blink/glitch
        // counter continuity from the AWAITING screen.
        loop {
            let glyph = self.cursor.tick(POLL_MS);
            Self::draw_or_clear_cursor(renderer, glyph, cursor_col, cursor_row);
            self.stall(POLL_MS);

            if let Some((ch, sc)) = self.poll_key() {
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
