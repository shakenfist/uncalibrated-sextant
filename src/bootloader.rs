// Locked-bootloader sub-state-machine for uncalibrated-sextant.
//
// Plays mid-`Scene::run_booting`, between the `SENSORIUM: nominal` line
// and the `EMERGENCY SAFE BOOT COMPLETE` line. Implements the diegetic
// failed-decryption flow: telemetry preamble announcing the b64 / NIST
// failure, an R/I/A operator prompt, retry-with-attempt-counter on R,
// cold reset on A, encoded-blob + paste capture on I. Validates the
// pasted payload byte-exact against `sextant{HELLO_OPERATOR}`; success
// returns to the caller for boot continuation, wrong-paste-cap-reached
// or silent-wait-elapsed both fall through to a 30 s visible countdown
// followed by ACPI shutdown.
//
// State carrier (`BootloaderScene`) holds shared mutable references to
// the renderer, ring buffer, and the scene clock (`clock_ms`) so the
// sub-state-machine's events and stall timing land on the same timeline
// as the rest of the run. The bootloader does not own a clock — that
// would create two clocks the serial drain would have to reconcile.
//
// Per principle 6: every dot in the retry animation, every digit in the
// countdown, every echo character of the paste input is its own
// `draw_glyph` (or `draw_text_at`, which calls `draw_glyph` per char)
// call. No multi-glyph blit shortcuts.

use crate::event::{BootloaderChoice, Event, RingBuffer};
use crate::renderer::Renderer;
use crate::scene::{poll_key, stall, PACE_LINE_MS, POLL_MS};

/// Maximum length of the paste capture buffer in bytes.
///
/// `sextant{HELLO_OPERATOR}` is 23 bytes; 64 leaves headroom for paste
/// artifacts and any future longer payloads.
const PASTE_BUFFER_LEN: usize = 64;

/// The byte-exact decoded value the operator must paste to continue.
const PASTE_TARGET: &str = "sextant{HELLO_OPERATOR}";

/// The encoded blob shown to the operator (base64 of `PASTE_TARGET`).
const ENCODED_BLOB: &str = "c2V4dGFudHtIRUxMT19PUEVSQVRPUn0=";

/// Per-dot delay during the retry animation (ms).
const RETRY_SLEEP_MS: u64 = 200;

/// Number of dots in the retry-animation dot leader.
const RETRY_DOT_COUNT: u32 = 8;

/// After this many retry presses, render the diegetic nudge below the
/// prompt and leave it sticky for the rest of the prompt's lifetime.
const RETRY_NUDGE_AFTER: u32 = 5;

/// Maximum idle time at the awaiting-paste prompt with the buffer
/// empty before we transition to the visible-countdown timeout (ms).
const PASTE_SILENT_WAIT_MS: u64 = 60_000;

/// Visible-countdown duration before halt + ACPI shutdown (seconds).
const TIMEOUT_COUNTDOWN_S: u64 = 30;

/// Stall after the error-halt line renders, before ACPI shutdown (ms).
/// Long enough that a CI session recording catches the message clearly.
const ERROR_HALT_MS: u64 = 5_000;

/// Stall after `Booting...` renders on the success path, before the
/// caller continues with `EMERGENCY SAFE BOOT COMPLETE` (ms).
const BOOT_PAUSE_MS: u64 = 600;

/// Maximum number of wrong-paste re-prompts before falling through to
/// the visible countdown (does not include the first paste attempt).
const WRONG_PASTE_LIMIT: u32 = 3;

/// Outcome of `bootloader::run`.
///
/// Only one variant: the success path that returns control to the
/// caller. The Abort and Timeout terminal paths call
/// `uefi::runtime::reset` directly inside `run`, so they never return.
pub enum BootloaderOutcome {
    /// Operator pasted the correct payload. Caller continues the boot
    /// sequence at `next_row`.
    Continue { next_row: usize },
}

/// Internal state carrier for the bootloader sub-state-machine.
struct BootloaderScene<'a> {
    renderer: &'a mut Renderer,
    ring: &'a mut RingBuffer<256>,
    clock_ms: &'a mut u64,
    /// First row of the bootloader scene region (passed in from the
    /// caller; the telemetry preamble lines render on this row and
    /// `start_row + 1`).
    start_row: usize,
    /// Highest row used so far. The success path's region-clear walks
    /// `start_row..=highest_row` to wipe the scene before rendering
    /// `Booting...`.
    highest_row: usize,
    /// 1-indexed count of times the R/I/A prompt has rendered.
    /// `attempt = 1` is the first render; `attempt = 2` is the first
    /// re-render after a Retry; etc.
    prompt_attempt: u32,
    /// Count of wrong pastes received at the awaiting-payload prompt.
    /// Separate from `prompt_attempt` (different prompts, different
    /// meanings, different parser tags). Reaches `WRONG_PASTE_LIMIT`
    /// to trigger the timeout flow.
    wrong_paste_count: u32,
    /// Row the R/I/A prompt currently lives on.
    prompt_row: usize,
    /// Row the diegetic nudge lives on (one below the prompt). Only
    /// rendered after `RETRY_NUDGE_AFTER` retries; once rendered it
    /// is sticky.
    nudge_row: usize,
    /// True once the diegetic nudge has been rendered. Used so subsequent
    /// retries clear (and re-render) the nudge row alongside the prompt.
    nudge_rendered: bool,
    /// Row the wrong-paste indicator lives on (one below the input
    /// row). Rendered in-place on each wrong-paste outcome so the
    /// count updates from "1 of 3" to "2 of 3" without ghost digits.
    /// Set by `run_blob_and_paste` before the paste loop; the initial
    /// value here is a placeholder that is overwritten before use.
    wrong_indicator_row: usize,
}

/// Entry point for the locked-bootloader sub-state-machine.
///
/// Renders the telemetry preamble, prompts for R/I/A, and (on Ignore)
/// captures and validates a paste against `PASTE_TARGET`. On success,
/// returns `BootloaderOutcome::Continue { next_row }`. On Abort or
/// timeout / wrong-paste-cap-reached, terminates the run via
/// `uefi::runtime::reset` and never returns to the caller.
pub fn run(
    renderer: &mut Renderer,
    ring: &mut RingBuffer<256>,
    clock_ms: &mut u64,
    start_row: usize,
) -> BootloaderOutcome {
    let mut scene = BootloaderScene {
        renderer,
        ring,
        clock_ms,
        start_row,
        highest_row: start_row,
        prompt_attempt: 0,
        wrong_paste_count: 0,
        // The prompt sits two rows below `start_row` (one row each for
        // the b64-OFFLINE and NIST-DISABLED telemetry lines).
        prompt_row: start_row + 2,
        nudge_row: start_row + 3,
        nudge_rendered: false,
        // Placeholder; `run_blob_and_paste` sets this to `input_row + 1`
        // (i.e. `start_row + 7`) before the paste loop begins.
        wrong_indicator_row: start_row + 7,
    };

    scene.render_telemetry_preamble();
    scene.run_prompt_loop()
}

impl<'a> BootloaderScene<'a> {
    /// Track that `row` has been written to and bump `highest_row` if
    /// needed. Used so the success-path region-clear knows how far
    /// down to wipe.
    fn note_row(&mut self, row: usize) {
        if row > self.highest_row {
            self.highest_row = row;
        }
    }

    /// Render the two telemetry preamble lines that establish the
    /// scene's diegetic failure: b64 coprocessor OFFLINE and NIST
    /// 800-53 SC-28(1) Secret hardening DISABLED BY CONFIGURATION.
    /// Each line emits `Event::LineRendered` and is paced at
    /// `PACE_LINE_MS` to match the rest of the boot transcript.
    fn render_telemetry_preamble(&mut self) {
        let row1 = self.start_row;
        self.renderer.draw_telemetry_line(
            "Advanced b64 cryptographic coprocessor",
            "OFFLINE",
            row1,
        );
        self.ring.push(Event::LineRendered {
            row: row1,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(row1);
        stall(self.clock_ms, PACE_LINE_MS);

        let row2 = self.start_row + 1;
        self.renderer.draw_telemetry_line(
            "NIST 800-53 SC-28(1) Secret hardening",
            "DISABLED BY CONFIGURATION",
            row2,
        );
        self.ring.push(Event::LineRendered {
            row: row2,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(row2);
        stall(self.clock_ms, PACE_LINE_MS);
    }

    /// Render the R/I/A prompt at `prompt_row`, with `(attempt N)`
    /// suffix when `prompt_attempt >= 2` (i.e. on every render after
    /// the first). Increments `prompt_attempt` to reflect the new
    /// 1-indexed render count, emits `Event::LineRendered`, and
    /// re-renders the diegetic nudge row if it has previously been
    /// shown (so the nudge does not get clobbered by a `clear_row`).
    fn render_prompt(&mut self) {
        self.prompt_attempt += 1;

        const PROMPT_BASE: &str =
            "Decryption of next-stage bootloader failed. (R)etry, (I)gnore, or (A)bort?";

        // Draw the base prompt at column 0.
        self.renderer.draw_text_at(PROMPT_BASE, 0, self.prompt_row);

        // On the second and later renders, append ` (attempt N)`.
        if self.prompt_attempt >= 2 {
            let suffix_col = PROMPT_BASE.len();
            // Per principle 6, build the suffix glyph-by-glyph; format
            // the attempt number into a small ASCII buffer first.
            let mut nbuf = [0u8; 10];
            let nlen = format_u32(self.prompt_attempt, &mut nbuf);
            // Render " (attempt " then the digits then ")".
            let prefix = " (attempt ";
            self.renderer
                .draw_text_at(prefix, suffix_col, self.prompt_row);
            let mut col = suffix_col + prefix.len();
            for &b in &nbuf[..nlen] {
                self.renderer.draw_glyph(b as char, col, self.prompt_row);
                col += 1;
            }
            self.renderer.draw_glyph(')', col, self.prompt_row);
        }

        self.ring.push(Event::LineRendered {
            row: self.prompt_row,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(self.prompt_row);

        // If the nudge has been rendered before, re-render it so it
        // remains visible alongside the freshly-redrawn prompt.
        if self.nudge_rendered {
            self.render_nudge();
        }
    }

    /// Render the diegetic nudge below the prompt and mark it as
    /// shown. Idempotent in effect: subsequent calls re-render the
    /// same line on the same row.
    fn render_nudge(&mut self) {
        const NUDGE: &str = "Continued retry will not change the outcome.";
        self.renderer.draw_text_at(NUDGE, 0, self.nudge_row);
        self.ring.push(Event::LineRendered {
            row: self.nudge_row,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(self.nudge_row);
        self.nudge_rendered = true;
    }

    /// Render the prompt and poll until the operator selects R, I,
    /// or A (case-insensitive). Loops on Retry; cold-resets on Abort;
    /// returns from the function on Ignore via the paste flow.
    ///
    /// No indecision timeout: the operator may step away from the
    /// R/I/A prompt indefinitely. The silent-wait timer applies only
    /// to the awaiting-paste prompt below.
    fn run_prompt_loop(&mut self) -> BootloaderOutcome {
        loop {
            self.render_prompt();

            // Poll forever for r/i/a. Any other key is ignored.
            let choice = loop {
                stall(self.clock_ms, POLL_MS);
                if let Some((ch, sc)) = poll_key() {
                    // Always emit the existing Keypress event so the
                    // serial drain reflects every actual keystroke.
                    self.ring.push(Event::Keypress {
                        unicode: ch,
                        scancode: sc,
                        timestamp_ms: *self.clock_ms,
                    });
                    let lower = ch.to_ascii_lowercase();
                    match lower {
                        'r' => break BootloaderChoice::Retry,
                        'i' => break BootloaderChoice::Ignore,
                        'a' => break BootloaderChoice::Abort,
                        _ => continue,
                    }
                }
            };

            // Record the decision before taking any action so the
            // serial drain gets a chance to capture it (modulo the
            // Abort cold-reset caveat noted below).
            self.ring.push(Event::BootloaderDecision {
                choice,
                attempt: self.prompt_attempt,
                timestamp_ms: *self.clock_ms,
            });

            match choice {
                BootloaderChoice::Retry => {
                    self.run_retry_animation();
                    // After the animation, clear the prompt row(s) and
                    // (if rendered) the nudge row, then loop to
                    // re-render the prompt with the bumped counter.
                    self.renderer.clear_row(self.prompt_row);
                    if self.nudge_rendered {
                        self.renderer.clear_row(self.nudge_row);
                    }
                    // After RETRY_NUDGE_AFTER retry presses, render
                    // the sticky nudge once. `prompt_attempt` is the
                    // count of times the prompt has been rendered so
                    // far; the operator has pressed R `prompt_attempt`
                    // times by the time this branch runs (each render
                    // ended in a retry press to land us here).
                    if self.prompt_attempt >= RETRY_NUDGE_AFTER && !self.nudge_rendered {
                        self.render_nudge();
                    }
                    // Loop continues, re-render prompt with bumped N.
                }
                BootloaderChoice::Ignore => {
                    return self.run_blob_and_paste();
                }
                BootloaderChoice::Abort => {
                    // Cold reset means the serial drain does not run
                    // before the firmware reboots, so this Abort
                    // decision will not appear in `dist/serial.log`
                    // for the killed run — it would only appear if
                    // the run somehow survived to drain. The next
                    // run's drain starts fresh anyway.
                    uefi::runtime::reset(
                        uefi::runtime::ResetType::COLD,
                        uefi::Status::SUCCESS,
                        None,
                    );
                }
            }
        }
    }

    /// Play the animated retry leader: write `Retrying decryption`
    /// in place over the prompt row, then blit eight dots one at a
    /// time at `RETRY_SLEEP_MS` per dot. Per principle 6 each dot
    /// is its own `draw_glyph` call.
    fn run_retry_animation(&mut self) {
        // Clear prompt row first so the animation starts on a clean
        // line; the prompt's `(attempt N)` suffix would otherwise
        // ghost behind the shorter `Retrying decryption` text.
        self.renderer.clear_row(self.prompt_row);

        const RETRYING: &str = "Retrying decryption";
        self.renderer.draw_text_at(RETRYING, 0, self.prompt_row);

        let mut col = RETRYING.len();
        for _ in 0..RETRY_DOT_COUNT {
            stall(self.clock_ms, RETRY_SLEEP_MS);
            self.renderer.draw_glyph('.', col, self.prompt_row);
            col += 1;
        }
    }

    /// Render the encoded-blob screen, then capture and validate a
    /// paste from the operator. On success returns
    /// `BootloaderOutcome::Continue`; on wrong-paste-cap-reached or
    /// silent-wait-elapsed, falls through to the visible countdown
    /// (which never returns).
    ///
    /// Layout: row+0 / row+1 are the existing telemetry preamble;
    /// the blob screen reuses row+2 onward (the same rows the R/I/A
    /// prompt occupied). Rows are cleared before the new content
    /// goes down.
    fn run_blob_and_paste(&mut self) -> BootloaderOutcome {
        // Wipe the prompt and (if shown) nudge — the blob screen
        // overlays this region.
        self.renderer.clear_row(self.prompt_row);
        if self.nudge_rendered {
            self.renderer.clear_row(self.nudge_row);
        }

        // Row layout for the blob screen.
        let intro_row = self.start_row + 2;
        let blob_row = self.start_row + 4;
        let input_row = self.start_row + 6;
        self.wrong_indicator_row = input_row + 1;

        // Intro line.
        self.renderer.draw_text_at(
            "Cryptographic co-processor offline. Encrypted bootloader payload follows. \
             Decode externally and paste back to continue.",
            0,
            intro_row,
        );
        self.ring.push(Event::LineRendered {
            row: intro_row,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(intro_row);

        // Blank row at start_row + 3.

        // Encoded blob.
        self.renderer.draw_text_at(ENCODED_BLOB, 0, blob_row);
        self.ring.push(Event::LineRendered {
            row: blob_row,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(blob_row);

        // Blank row at start_row + 5.

        // Input prompt; subsequent characters echo at column
        // `input_col_start` onward.
        const INPUT_PROMPT: &str = "Awaiting decoded payload> ";
        self.renderer.draw_text_at(INPUT_PROMPT, 0, input_row);
        self.ring.push(Event::LineRendered {
            row: input_row,
            timestamp_ms: *self.clock_ms,
        });
        self.note_row(input_row);

        let input_col_start = INPUT_PROMPT.len();

        loop {
            match self.capture_paste(input_row, input_col_start) {
                PasteOutcome::Correct(len) => {
                    self.ring.push(Event::PasteReceived {
                        len,
                        correct: true,
                        timestamp_ms: *self.clock_ms,
                    });
                    return self.run_success();
                }
                PasteOutcome::Wrong(len) => {
                    self.ring.push(Event::PasteReceived {
                        len,
                        correct: false,
                        timestamp_ms: *self.clock_ms,
                    });
                    self.wrong_paste_count += 1;
                    if self.wrong_paste_count >= WRONG_PASTE_LIMIT {
                        // Three strikes — bypass the silent-wait
                        // phase and go straight to the visible
                        // countdown. Operator is definitionally not
                        // silent.
                        self.run_timeout();
                    }
                    // Wipe the previous echo and re-render the bare
                    // input prompt (no suffix on this row). The next
                    // `capture_paste` call echoes fresh from
                    // `input_col_start` with no row contention.
                    self.renderer.clear_row(input_row);
                    self.renderer.draw_text_at(INPUT_PROMPT, 0, input_row);
                    self.ring.push(Event::LineRendered {
                        row: input_row,
                        timestamp_ms: *self.clock_ms,
                    });

                    // Render `(wrong, attempt N of 3)` on the
                    // indicator row below the input prompt. Clear it
                    // first so the digit count updates in place
                    // without ghost characters (e.g. "1 of 3" ->
                    // "2 of 3").
                    self.renderer.clear_row(self.wrong_indicator_row);
                    let mut col = 0;
                    self.renderer
                        .draw_text_at("(wrong, attempt ", col, self.wrong_indicator_row);
                    col += "(wrong, attempt ".len();
                    let mut nbuf = [0u8; 10];
                    let nlen = format_u32(self.wrong_paste_count, &mut nbuf);
                    for &b in &nbuf[..nlen] {
                        self.renderer
                            .draw_glyph(b as char, col, self.wrong_indicator_row);
                        col += 1;
                    }
                    self.renderer
                        .draw_text_at(" of ", col, self.wrong_indicator_row);
                    col += " of ".len();
                    let mut lbuf = [0u8; 10];
                    let llen = format_u32(WRONG_PASTE_LIMIT, &mut lbuf);
                    for &b in &lbuf[..llen] {
                        self.renderer
                            .draw_glyph(b as char, col, self.wrong_indicator_row);
                        col += 1;
                    }
                    self.renderer.draw_glyph(')', col, self.wrong_indicator_row);
                    self.note_row(self.wrong_indicator_row);
                    self.ring.push(Event::LineRendered {
                        row: self.wrong_indicator_row,
                        timestamp_ms: *self.clock_ms,
                    });
                    // Loop: capture_paste resets to its own input
                    // column tracking.
                }
                PasteOutcome::Timeout => {
                    self.run_timeout();
                }
            }
        }
    }

    /// Poll keys into a fixed buffer until Enter, buffer-fill, or the
    /// silent-wait timer elapses. Returns the outcome — Correct
    /// (matched `PASTE_TARGET`), Wrong (anything else, including
    /// buffer-fill), or Timeout (silent-wait elapsed with the buffer
    /// empty).
    fn capture_paste(&mut self, input_row: usize, input_col_start: usize) -> PasteOutcome {
        let mut buf = [0u8; PASTE_BUFFER_LEN];
        let mut len: usize = 0;
        let mut col = input_col_start;
        let mut idle_ms: u64 = 0;

        loop {
            stall(self.clock_ms, POLL_MS);

            if let Some((ch, sc)) = poll_key() {
                idle_ms = 0;
                self.ring.push(Event::Keypress {
                    unicode: ch,
                    scancode: sc,
                    timestamp_ms: *self.clock_ms,
                });

                // Enter terminates the paste.
                if ch == '\r' || ch == '\n' {
                    // Strip a single trailing CR if present (ryll's
                    // Enter mapping shouldn't put one in the buffer
                    // at the same time it sends an Enter scancode,
                    // but the strip-once rule is harmless and
                    // documented).
                    if len > 0 && buf[len - 1] == b'\r' {
                        len -= 1;
                    }
                    let target = PASTE_TARGET.as_bytes();
                    if len == target.len() && &buf[..len] == target {
                        return PasteOutcome::Correct(len);
                    } else {
                        return PasteOutcome::Wrong(len);
                    }
                }

                // Printable ASCII range: append + echo. Anything
                // outside this range (control characters, special
                // scancodes) we ignore on the input path. The echo
                // is one `draw_glyph` per character.
                if ch as u32 >= 0x20 && (ch as u32) < 0x7f {
                    if len < PASTE_BUFFER_LEN {
                        buf[len] = ch as u8;
                        len += 1;
                        self.renderer.draw_glyph(ch, col, input_row);
                        col += 1;
                        if len == PASTE_BUFFER_LEN {
                            // Buffer fills before Enter: treat as a
                            // wrong paste so the operator gets a
                            // re-prompt rather than a silent stall.
                            return PasteOutcome::Wrong(len);
                        }
                    } else {
                        // Defensive — should be unreachable given
                        // the buffer-fill branch above.
                        return PasteOutcome::Wrong(len);
                    }
                }
            } else {
                idle_ms += POLL_MS;
                // 60 s without any key arriving triggers timeout
                // regardless of buffer state — covers the operator-
                // walked-away-mid-paste and ryll-disconnected-mid-
                // paste cases as well as the never-started case.
                // `idle_ms` resets on each successful poll_key above.
                if idle_ms >= PASTE_SILENT_WAIT_MS {
                    return PasteOutcome::Timeout;
                }
            }
        }
    }

    /// Success path: clear the bootloader scene region, render
    /// `Booting...` on `start_row + 2`, stall `BOOT_PAUSE_MS`, and
    /// return `Continue { next_row: start_row + 3 }`.
    fn run_success(&mut self) -> BootloaderOutcome {
        for row in self.start_row..=self.highest_row {
            self.renderer.clear_row(row);
        }
        let booting_row = self.start_row + 2;
        self.renderer.draw_text_at("Booting...", 0, booting_row);
        self.ring.push(Event::LineRendered {
            row: booting_row,
            timestamp_ms: *self.clock_ms,
        });
        stall(self.clock_ms, BOOT_PAUSE_MS);
        BootloaderOutcome::Continue {
            next_row: self.start_row + 3,
        }
    }

    /// Timeout path: emit `BootloaderTimeout`, clear the scene
    /// region, run the visible 30-to-0 countdown with in-place digit
    /// updates, render the error halt line, stall `ERROR_HALT_MS`,
    /// then ACPI-shutdown via `uefi::runtime::reset`. Diverges.
    fn run_timeout(&mut self) -> ! {
        self.ring.push(Event::BootloaderTimeout {
            timestamp_ms: *self.clock_ms,
        });

        for row in self.start_row..=self.highest_row {
            self.renderer.clear_row(row);
        }

        let countdown_row = self.start_row + 2;
        const COUNTDOWN_PREFIX: &str = "Awaiting decoded payload. Aborting in ";
        self.renderer
            .draw_text_at(COUNTDOWN_PREFIX, 0, countdown_row);
        // Trailing "..." after the NN digits, drawn once and not
        // touched per tick.
        let nn_col = COUNTDOWN_PREFIX.len();
        let dots_col = nn_col + 2;
        self.renderer.draw_text_at("...", dots_col, countdown_row);

        // Tick from TIMEOUT_COUNTDOWN_S down to 0 inclusive. Each
        // tick clears the two NN cells and draws the new digits;
        // the rest of the row is not redrawn. Zero-padded so the
        // cell count never changes.
        let mut secs = TIMEOUT_COUNTDOWN_S;
        loop {
            // Clear the two NN cells, then draw the zero-padded
            // digits. Per-glyph blits via `clear_cell` and
            // `draw_glyph`.
            self.renderer.clear_cell(nn_col, countdown_row);
            self.renderer.clear_cell(nn_col + 1, countdown_row);
            let tens = ((secs / 10) as u8 + b'0') as char;
            let ones = ((secs % 10) as u8 + b'0') as char;
            self.renderer.draw_glyph(tens, nn_col, countdown_row);
            self.renderer.draw_glyph(ones, nn_col + 1, countdown_row);

            if secs == 0 {
                break;
            }
            stall(self.clock_ms, 1_000);
            secs -= 1;
        }

        let halt_row = countdown_row + 2;
        self.renderer
            .draw_text_at("BOOTLOADER UNRECOVERABLE. SHUTTING DOWN.", 0, halt_row);
        self.ring.push(Event::LineRendered {
            row: halt_row,
            timestamp_ms: *self.clock_ms,
        });
        stall(self.clock_ms, ERROR_HALT_MS);

        uefi::runtime::reset(
            uefi::runtime::ResetType::SHUTDOWN,
            uefi::Status::SUCCESS,
            None,
        )
    }
}

/// Outcome of one paste-capture pass; consumed by `run_blob_and_paste`'s
/// outer loop.
enum PasteOutcome {
    Correct(usize),
    Wrong(usize),
    Timeout,
}

/// Format a non-negative `u32` into `buf` as ASCII decimal digits.
/// Returns the number of bytes written. Does not write a NUL.
///
/// Local helper because `core::fmt` would pull in formatting
/// machinery for a per-glyph blit path that just needs the digits.
fn format_u32(mut n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        if buf.is_empty() {
            return 0;
        }
        buf[0] = b'0';
        return 1;
    }
    // Write digits least-significant-first, then reverse.
    let mut i = 0;
    while n > 0 && i < buf.len() {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    buf[..i].reverse();
    i
}
