// Renderer module for uncalibrated-sextant.
//
// Owns the GOP framebuffer handle, implements per-glyph BitBlt drawing
// with a fixed phosphor-green-on-black palette, and provides dot-leader
// text layout for telemetry lines.
//
// Principle 6 (from DESIGN.md): every glyph — including dots — is sent
// to the firmware via a separate BltOp::BufferToVideo call. No monolithic
// framebuffer memcpy.

mod font;

use uefi::boot::ScopedProtocol;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};

/// Target column for the dot-leader / status field separator.
pub const DOT_LEADER_COL: usize = 40;

/// Glyph cell width in pixels.
pub const CELL_W: usize = 8;

/// Glyph cell height in pixels.
pub const CELL_H: usize = 16;

/// Phosphor-green foreground. Dimmed from full-bright (51, 255, 51)
/// toward (51, 150, 51) to read as aged CRT rather than brand-new.
const FG: BltPixel = BltPixel::new(51, 150, 51);

/// Pure black background.
const BG: BltPixel = BltPixel::new(0, 0, 0);

/// GOP-backed text renderer.
///
/// Owns the `ScopedProtocol` for `GraphicsOutput`; dropping the
/// `Renderer` closes the protocol handle.
pub struct Renderer {
    gop: ScopedProtocol<GraphicsOutput>,
    width: usize,
    height: usize,
}

impl Renderer {
    /// Acquire GOP, attempt to switch to 1024x768, clear the screen.
    ///
    /// Falls back to the current mode if 1024x768 is not available or
    /// if mode-switching fails.
    pub fn new() -> uefi::Result<Self> {
        let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>()?;
        let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle)?;

        // Try to switch to 1024x768; ignore errors and use current mode.
        let target = gop.modes().find(|m| m.info().resolution() == (1024, 768));
        if let Some(mode) = target {
            let _ = gop.set_mode(&mode);
        }

        let info = gop.current_mode_info();
        let (width, height) = info.resolution();

        let mut renderer = Self { gop, width, height };
        renderer.clear();
        Ok(renderer)
    }

    /// Fill the entire screen with the background colour.
    pub fn clear(&mut self) {
        let _ = self.gop.blt(BltOp::VideoFill {
            color: BG,
            dest: (0, 0),
            dims: (self.width, self.height),
        });
    }

    /// Render a single glyph at the given text cell column and row.
    ///
    /// Unknown or out-of-range characters fall back to `'?'`.
    /// Issues exactly one `BltOp::BufferToVideo` call (principle 6).
    pub fn draw_glyph(&mut self, ch: char, col: usize, row: usize) {
        let index = if (ch as u32) < 0x80 {
            ch as usize
        } else {
            b'?' as usize
        };
        let bitmap = &font::FONT_8X16[index];

        // Build an 8x16 BltPixel scratch buffer from the glyph bitmap.
        let mut buf = [BG; CELL_W * CELL_H];
        for (r, &byte) in bitmap.iter().enumerate() {
            for c in 0..CELL_W {
                // BDF: bit 7 is the leftmost pixel (MSB-leftmost).
                if byte & (0x80 >> c) != 0 {
                    buf[r * CELL_W + c] = FG;
                }
            }
        }

        let px = col * CELL_W;
        let py = row * CELL_H;
        let _ = self.gop.blt(BltOp::BufferToVideo {
            buffer: &buf,
            src: BltRegion::Full,
            dest: (px, py),
            dims: (CELL_W, CELL_H),
        });
    }

    /// Render a string of text starting at column 0 on the given row.
    ///
    /// Calls `draw_glyph` once per character.
    pub fn draw_line(&mut self, text: &str, row: usize) {
        for (col, ch) in text.chars().enumerate() {
            self.draw_glyph(ch, col, row);
        }
    }

    /// Render a telemetry line: `LABEL .......... STATUS`.
    ///
    /// The label is drawn from column 0. A space follows the label, then
    /// dots fill up to `DOT_LEADER_COL`, then a space, then the status.
    ///
    /// Every character — including each dot — is a separate `draw_glyph`
    /// call (principle 6).
    pub fn draw_telemetry_line(&mut self, label: &str, status: &str, row: usize) {
        // Draw label.
        for (col, ch) in label.chars().enumerate() {
            self.draw_glyph(ch, col, row);
        }

        let label_end = label.len();

        // Space after label, then dots up to DOT_LEADER_COL.
        let dot_start = label_end + 1;
        let dot_end = DOT_LEADER_COL;
        if dot_start < dot_end {
            self.draw_glyph(' ', label_end, row);
            for col in dot_start..dot_end {
                self.draw_glyph('.', col, row);
            }
        }

        // Status starts at DOT_LEADER_COL + 1 (one space separator).
        let status_start = DOT_LEADER_COL + 1;
        for (i, ch) in status.chars().enumerate() {
            self.draw_glyph(ch, status_start + i, row);
        }
    }
}
