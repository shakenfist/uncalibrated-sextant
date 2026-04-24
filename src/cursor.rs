// Cursor glyph variants, blink timing, and occasional glitch
// substitution for uncalibrated-sextant.
//
// Blink: 500 ms on, 500 ms off (1 Hz total).
// Glitch: every 6th blink-on transition picks a broken glyph variant
// from a four-element pool selected via a 16-bit Fibonacci LFSR.

/// Solid 8x16 block — the canonical, healthy cursor glyph.
/// All pixels lit; MSB is the leftmost pixel per row.
pub const GLYPH_CANONICAL: [u8; 16] = [0xFF; 16];

/// A 3x3 chunk cleared from the centre of the block — clearly visible
/// as a hole the eye can resolve at 1024x768.
pub const GLYPH_MISSING_PIXEL: [u8; 16] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, //
    0xC7, // row 6: 1100_0111 — bits 3, 4, 5 cleared
    0xC7, // row 7
    0xC7, // row 8
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// Right two columns entirely dark — a clearly damaged trailing edge.
/// 0xFC = 1111_1100.
pub const GLYPH_SMEARED_EDGE: [u8; 16] = [0xFC; 16];

/// Whole block shifted two pixels right — leftmost two columns dark.
/// 0x3F = 0011_1111: rightmost 6 columns lit, leftmost 2 dark.
pub const GLYPH_SHIFTED_COLUMN: [u8; 16] = [0x3F; 16];

/// Top three rows entirely dark — cursor has "slipped" down as if a
/// phosphor cell stuck low. The remaining 13 rows are a full block.
pub const GLYPH_PHOSPHOR_TRAIL: [u8; 16] = [
    0x00, 0x00, 0x00, //
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

/// The four broken-variant pool, indexed by LFSR mod 4.
const BROKEN_GLYPHS: [[u8; 16]; 4] = [
    GLYPH_MISSING_PIXEL,
    GLYPH_SMEARED_EDGE,
    GLYPH_SHIFTED_COLUMN,
    GLYPH_PHOSPHOR_TRAIL,
];

/// Blink half-period in milliseconds (500 ms on, 500 ms off).
const BLINK_HALF_MS: u64 = 500;

/// Number of blink-on transitions between glitch substitutions.
const GLITCH_EVERY_N: u32 = 6;

/// Whether the cursor is currently in the lit or dark half of a blink.
#[derive(Copy, Clone)]
enum BlinkPhase {
    On,
    Off,
}

/// Persistent cursor state: blink timing, glitch LFSR, variant selection.
pub struct CursorState {
    phase: BlinkPhase,
    /// Accumulated time in current blink half, in milliseconds.
    since_switch_ms: u64,
    /// Count of blink-on transitions since the last glitch substitution.
    blinks_since_glitch: u32,
    /// 16-bit Fibonacci LFSR state; seeded to 0xACE1, never zero.
    lfsr: u16,
    /// Current glyph bytes selected for this blink-on half.
    current_glyph: [u8; 16],
}

impl CursorState {
    /// Initialise in the On phase with the canonical glyph.
    pub fn new() -> Self {
        Self {
            phase: BlinkPhase::On,
            since_switch_ms: 0,
            blinks_since_glitch: 0,
            lfsr: 0xACE1,
            current_glyph: GLYPH_CANONICAL,
        }
    }

    /// Advance time by `elapsed_ms` milliseconds.
    ///
    /// Returns the glyph bytes to draw this frame, or `None` when the
    /// cursor is in the dark half of the blink cycle.
    pub fn tick(&mut self, elapsed_ms: u64) -> Option<[u8; 16]> {
        self.since_switch_ms += elapsed_ms;

        if self.since_switch_ms >= BLINK_HALF_MS {
            self.since_switch_ms -= BLINK_HALF_MS;
            self.phase = match self.phase {
                BlinkPhase::On => BlinkPhase::Off,
                BlinkPhase::Off => {
                    // Transitioning into the On phase: pick glyph.
                    self.blinks_since_glitch += 1;
                    if self.blinks_since_glitch >= GLITCH_EVERY_N {
                        self.blinks_since_glitch = 0;
                        let variant = (self.lfsr_next() % 4) as usize;
                        self.current_glyph = BROKEN_GLYPHS[variant];
                    } else {
                        self.current_glyph = GLYPH_CANONICAL;
                    }
                    BlinkPhase::On
                }
            };
        }

        match self.phase {
            BlinkPhase::On => Some(self.current_glyph),
            BlinkPhase::Off => None,
        }
    }

    /// Advance the Fibonacci LFSR one step and return the new state.
    /// Taps: bits 0, 2, 3, 5 of the current state (Galois-style).
    fn lfsr_next(&mut self) -> u16 {
        let bit = (self.lfsr ^ (self.lfsr >> 2) ^ (self.lfsr >> 3) ^ (self.lfsr >> 5)) & 0x0001;
        self.lfsr = (self.lfsr >> 1) | (bit << 15);
        self.lfsr
    }
}
