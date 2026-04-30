// Renderer module for uncalibrated-sextant.
//
// Owns the GOP framebuffer handle, implements per-glyph BitBlt drawing
// with a fixed phosphor-green-on-black palette, and provides dot-leader
// text layout for telemetry lines.
//
// Principle 6 (from DESIGN.md): every glyph — including dots — is sent
// to the firmware via a separate BltOp::BufferToVideo call. No monolithic
// framebuffer memcpy.

extern crate alloc;

mod font;

use alloc::vec::Vec;
use uefi::boot::ScopedProtocol;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};

/// Pick the available mode whose resolution is nearest to `req`,
/// scored as `|dw| + |dh|`. Ties are broken by preferring the
/// smaller width; if widths are equal, the smaller height wins.
/// Both tie-break rules are applied in that order, deterministically.
///
/// Returns the index into `available` of the chosen mode.
/// Returns `None` only when `available` is empty (which would be a
/// firmware bug — GOP guarantees at least one mode).
fn nearest_mode(req: (usize, usize), available: &[(usize, usize)]) -> Option<usize> {
    if available.is_empty() {
        return None;
    }
    let (rw, rh) = req;
    let score = |(w, h): (usize, usize)| -> usize { rw.abs_diff(w) + rh.abs_diff(h) };
    let mut best_idx = 0;
    let mut best_score = score(available[0]);
    for (i, &candidate) in available.iter().enumerate().skip(1) {
        let s = score(candidate);
        let better = s < best_score
            || (s == best_score && candidate.0 < available[best_idx].0)
            || (s == best_score
                && candidate.0 == available[best_idx].0
                && candidate.1 < available[best_idx].1);
        if better {
            best_idx = i;
            best_score = s;
        }
    }
    Some(best_idx)
}

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
    /// Acquire GOP, switch to the mode nearest to 1024x768 (via
    /// `set_mode`), and clear the screen.
    ///
    /// Previously this method searched for an exact 1024x768 match and
    /// kept whatever GOP came up in if the mode was absent. Now it
    /// delegates to `set_mode(1024, 768)`, which routes through
    /// `nearest_mode` and switches to the closest available resolution.
    /// On real OVMF, 1024x768 is always present so behaviour is
    /// observably identical; on firmware that omits it the new code
    /// switches to the nearest alternative instead of doing nothing.
    pub fn new() -> uefi::Result<Self> {
        let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>()?;
        let gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle)?;

        // Construct with a placeholder resolution; set_mode will update
        // width and height by querying back from GOP after the switch.
        let info = gop.current_mode_info();
        let (width, height) = info.resolution();
        let mut renderer = Self { gop, width, height };

        renderer.set_mode(1024, 768);
        renderer.clear();
        Ok(renderer)
    }

    /// Switch GOP to the mode nearest to the requested `(req_w, req_h)`,
    /// re-query GOP for the actually-applied resolution, update the
    /// cached `width` and `height`, and return the applied dimensions.
    ///
    /// The applied resolution may differ from the request when the
    /// requested mode is not exposed by the firmware; the caller is
    /// responsible for surfacing that difference.
    ///
    /// Per UEFI 2.10 §12.9, `set_mode` invalidates the framebuffer.
    /// The caller must redraw before this method returns control to a
    /// scene that expects pixels intact.
    ///
    /// Errors from `gop.set_mode` are ignored (`let _ =`) — continuing
    /// at whatever mode the firmware ended up in is better than panicking
    /// on a firmware refusal. The post-switch query ensures `width` and
    /// `height` always reflect reality.
    pub fn set_mode(&mut self, req_w: usize, req_h: usize) -> (usize, usize) {
        // Collect all available (width, height) pairs.
        let resolutions: Vec<(usize, usize)> =
            self.gop.modes().map(|m| m.info().resolution()).collect();

        if let Some(idx) = nearest_mode((req_w, req_h), &resolutions) {
            // Re-walk modes() to get the Mode value at the chosen index.
            // modes() is an iterator so we cannot index it cheaply; zip
            // with the resolution slice to find the matching Mode.
            let chosen_mode = self.gop.modes().nth(idx);
            if let Some(mode) = chosen_mode {
                let _ = self.gop.set_mode(&mode);
            }
        }

        // Always query back — never trust (req_w, req_h) as the truth.
        let (w, h) = self.gop.current_mode_info().resolution();
        self.width = w;
        self.height = h;
        (w, h)
    }

    /// All `(width, height)` modes the current GOP exposes, in the
    /// order GOP returns them.
    ///
    /// Returns an owned `Vec` rather than a borrowed iterator to avoid
    /// threading the GOP lifetime through callers. The allocation is
    /// small (a handful of `(usize, usize)` pairs) and is only made
    /// when the caller needs the list (boot-time serial dump, mode
    /// selection). An iterator form would require `&mut self` because
    /// `gop.modes()` takes `&mut GraphicsOutput` internally, which
    /// makes a shared-borrow return shape impractical; the owned vec
    /// avoids that problem entirely.
    pub fn available_modes(&mut self) -> Vec<(usize, usize)> {
        self.gop.modes().map(|m| m.info().resolution()).collect()
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

    /// Render a string of text starting at the given text-cell column
    /// on `row`. Per-glyph (principle 6); calls `draw_glyph` once per
    /// character.
    pub fn draw_text_at(&mut self, text: &str, col: usize, row: usize) {
        for (i, ch) in text.chars().enumerate() {
            self.draw_glyph(ch, col + i, row);
        }
    }

    /// Clear an entire text row, edge to edge between the horizontal
    /// margins, to background. One BltOp::VideoFill per call.
    pub fn clear_row(&mut self, row: usize) {
        let px = MARGIN_X;
        let py = MARGIN_Y + row * CELL_H;
        let dims = (self.width.saturating_sub(2 * MARGIN_X), CELL_H);
        let _ = self.gop.blt(BltOp::VideoFill {
            color: BG,
            dest: (px, py),
            dims,
        });
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
