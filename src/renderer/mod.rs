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

/// Horizontal overscan margin in pixels. All content is inset from
/// the left and right edges by this much so the screen does not
/// feel "hard up against the bezel".
pub const MARGIN_X: usize = 16;

/// Vertical overscan margin in pixels. All content is inset from
/// the top and bottom edges by this much.
pub const MARGIN_Y: usize = 16;

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

        let px = MARGIN_X + col * CELL_W;
        let py = MARGIN_Y + row * CELL_H;
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

    /// Render raw glyph bytes at the given text-cell position.
    ///
    /// Used by the cursor subsystem to draw canonical and broken-variant
    /// cursor glyphs without routing them through the ASCII font table.
    /// Each bit is interpreted identically to `draw_glyph`: bit 7 of
    /// each row byte is the leftmost pixel.
    /// Issues exactly one `BltOp::BufferToVideo` call (principle 6).
    pub fn draw_cursor_glyph(&mut self, bytes: &[u8; 16], col: usize, row: usize) {
        let mut buf = [BG; CELL_W * CELL_H];
        for (r, &byte) in bytes.iter().enumerate() {
            for c in 0..CELL_W {
                if byte & (0x80 >> c) != 0 {
                    buf[r * CELL_W + c] = FG;
                }
            }
        }

        let px = MARGIN_X + col * CELL_W;
        let py = MARGIN_Y + row * CELL_H;
        let _ = self.gop.blt(BltOp::BufferToVideo {
            buffer: &buf,
            src: BltRegion::Full,
            dest: (px, py),
            dims: (CELL_W, CELL_H),
        });
    }

    /// Clear a single text cell to the background colour.
    ///
    /// Used to erase the cursor during the dark half of a blink cycle.
    /// Issues one `BltOp::VideoFill` call.
    pub fn clear_cell(&mut self, col: usize, row: usize) {
        let px = MARGIN_X + col * CELL_W;
        let py = MARGIN_Y + row * CELL_H;
        let _ = self.gop.blt(BltOp::VideoFill {
            color: BG,
            dest: (px, py),
            dims: (CELL_W, CELL_H),
        });
    }

    /// Number of whole 8x16 text cells that fit between the left and
    /// right overscan margins on the current screen.
    pub fn screen_cols(&self) -> usize {
        (self.width.saturating_sub(2 * MARGIN_X)) / CELL_W
    }

    /// Draw a packed 1-bit-per-pixel bitmap as a grid of 8x16 tiles
    /// at text-cell position (`col`, `row`). Set bits render as
    /// foreground, clear bits as background.
    ///
    /// The bitmap must be row-major, MSB-leftmost, with each row
    /// padded to the next byte boundary. Width and height in pixels
    /// may be any positive value; trailing partial tiles render
    /// whatever bits the byte padding supplies (the vendor scripts
    /// arrange those to be zeros). One `BltOp::BufferToVideo` per
    /// tile (principle 6 — the repeated tile size lets the SPICE
    /// server dictionary-match).
    pub fn draw_text_bitmap(
        &mut self,
        bitmap: &[u8],
        width_px: usize,
        height_px: usize,
        col: usize,
        row: usize,
    ) {
        let bitmap_row_bytes = width_px.div_ceil(CELL_W);
        let tiles_x = bitmap_row_bytes;
        let tiles_y = height_px.div_ceil(CELL_H);
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let mut buf = [BG; CELL_W * CELL_H];
                for ly in 0..CELL_H {
                    let src_row = ty * CELL_H + ly;
                    if src_row >= height_px {
                        break;
                    }
                    let byte = bitmap[src_row * bitmap_row_bytes + tx];
                    for lx in 0..CELL_W {
                        if byte & (0x80 >> lx) != 0 {
                            buf[ly * CELL_W + lx] = FG;
                        }
                    }
                }
                let px = MARGIN_X + (col + tx) * CELL_W;
                let py = MARGIN_Y + (row + ty) * CELL_H;
                let _ = self.gop.blt(BltOp::BufferToVideo {
                    buffer: &buf,
                    src: BltRegion::Full,
                    dest: (px, py),
                    dims: (CELL_W, CELL_H),
                });
            }
        }
    }

    /// Render a hybrid language-probe line: bitmap label, ASCII dot
    /// leader ending at `DOT_LEADER_COL`, bitmap status starting at
    /// `DOT_LEADER_COL + 1`.
    ///
    /// Label and status bitmaps may be any pixel width; their
    /// rendered cell extent rounds up to the next byte boundary.
    /// The dot leader and the single space separator render via the
    /// per-glyph ASCII path so column alignment with the rest of
    /// the boot transcript is preserved (principle 6 throughout).
    pub fn draw_probe_line(
        &mut self,
        label_bitmap: &[u8],
        label_width_px: usize,
        status_bitmap: &[u8],
        status_width_px: usize,
        row: usize,
    ) {
        self.draw_text_bitmap(label_bitmap, label_width_px, CELL_H, 0, row);

        let label_end_cell = label_width_px.div_ceil(CELL_W);
        let dot_start = label_end_cell + 1;
        let dot_end = DOT_LEADER_COL;
        if dot_start < dot_end {
            self.draw_glyph(' ', label_end_cell, row);
            for col in dot_start..dot_end {
                self.draw_glyph('.', col, row);
            }
        }

        let status_col = DOT_LEADER_COL + 1;
        self.draw_text_bitmap(status_bitmap, status_width_px, CELL_H, status_col, row);
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
