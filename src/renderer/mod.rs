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
use qrcodegen_no_heap::{QrCode, QrCodeEcc, Version};
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

/// Visual on-screen digest region geometry.
///
/// The digest is a QR code rendered into the bottom-right corner of the
/// framebuffer, sized for the smallest supported GOP mode (640x480).
/// QR Version 5 (37x37 modules) at 4 pixels per module, with the
/// qrcodegen-default 4-module quiet zone rendered inside the matrix,
/// gives a 180x180 px region. At 640x480 this sits at (444, 268),
/// above the bottom toast row and below the top-right logo so the
/// y-ranges of digest and logo are disjoint.
///
/// `DIGEST_REGION_X` and `DIGEST_REGION_Y` are the 640x480 baseline
/// only — they exist so the compile-time `assert!` block below can
/// prove the region fits without colliding with the logo, AWAITING
/// cursor, or toast row at the smallest supported mode. At runtime
/// `draw_digest` right-anchors the region using the actual
/// `(self.width, self.height)`, so on larger modes (the default
/// 1024x768, the 1920x1080 cycle, etc.) the digest stays glued to
/// the bottom-right corner rather than floating at fixed pixel
/// coordinates in the middle of the screen.
///
/// The compile-time `assert!` block below proves the region fits at
/// 640x480 without colliding with the logo, the AWAITING cursor
/// (top-left), or the bottom-row toast.
///
/// `Renderer::draw_digest` consumes these constants directly; the
/// fit-assertion block below pins their relationships at compile time.
pub(crate) const DIGEST_QR_VERSION: usize = 5;
pub(crate) const DIGEST_QR_MODULES: usize = 37;
pub(crate) const DIGEST_QR_BORDER: usize = 4;
pub(crate) const DIGEST_MODULE_PX: usize = 4;
pub(crate) const DIGEST_REGION_PX: usize =
    (DIGEST_QR_MODULES + 2 * DIGEST_QR_BORDER) * DIGEST_MODULE_PX;
// Load-bearing for the compile-time `assert!` block below — those
// uses do not count against the dead-code lint for `pub(crate)`
// consts, but the constants document the 640x480 baseline geometry
// the assertions enforce. `draw_digest` derives its actual origin
// from `self.width` / `self.height` at runtime instead.
#[allow(dead_code)]
pub(crate) const DIGEST_REGION_X: usize = 640 - MARGIN_X - DIGEST_REGION_PX;
#[allow(dead_code)]
pub(crate) const DIGEST_REGION_Y: usize = 480 - MARGIN_Y - CELL_H - DIGEST_REGION_PX;

/// Version 5 pinned for `qrcodegen-no-heap`. Kept `const` so the
/// buffer sizing below resolves at compile time.
const DIGEST_QR_VERSION_V: Version = Version::new(DIGEST_QR_VERSION as u8);

/// Buffer length required by `qrcodegen-no-heap` for Version 5 codewords
/// plus the in-place mask/temp work area.
const DIGEST_QR_BUFFER_LEN: usize = DIGEST_QR_VERSION_V.buffer_len();

const _: () = {
    // Region pixel width must be a whole number of modules.
    assert!(DIGEST_REGION_PX % DIGEST_MODULE_PX == 0);
    // Region must exactly span (MODULES + 2 * BORDER) modules.
    assert!(DIGEST_REGION_PX == (DIGEST_QR_MODULES + 2 * DIGEST_QR_BORDER) * DIGEST_MODULE_PX);
    // Region must not overlap the bottom-row toast at 640x480.
    assert!(DIGEST_REGION_Y + DIGEST_REGION_PX + MARGIN_Y <= 480);
    // Region must not overlap the top-right logo at 640x480: the logo
    // occupies y in [16, 144); the digest sits below that y-range.
    assert!(DIGEST_REGION_Y >= 144);
    // Region must fit horizontally at 640 px wide.
    assert!(DIGEST_REGION_X + DIGEST_REGION_PX + MARGIN_X <= 640);
    // Asserting DIGEST_QR_VERSION is referenced so the constant stays
    // load-bearing for the encoder constructor in step 1c.
    assert!(DIGEST_QR_VERSION == 5);
};

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

    /// Blit an 8x16 glyph from raw bitmap bytes to the cell at `(col, row)`.
    ///
    /// Composes the `BltPixel` scratch buffer from `bytes` (each bit is a
    /// foreground pixel, MSB-leftmost per BDF convention) and issues exactly
    /// one `BltOp::BufferToVideo` call (principle 6).
    ///
    /// Out-of-bounds `(col, row)` is the **caller's contract**: this helper
    /// does not bounds-check. The guards on `draw_glyph` and
    /// `draw_cursor_glyph` enforce the invariant for all callers that come
    /// through those public entry points.
    fn blit_glyph_bytes(&mut self, bytes: &[u8; 16], col: usize, row: usize) {
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

    /// Render a single glyph at the given text cell column and row.
    ///
    /// Unknown or out-of-range characters fall back to `'?'`.
    /// Issues exactly one `BltOp::BufferToVideo` call (principle 6).
    pub fn draw_glyph(&mut self, ch: char, col: usize, row: usize) {
        if col >= self.screen_cols() || row >= self.screen_rows() {
            return;
        }
        let index = if (ch as u32) < 0x80 {
            ch as usize
        } else {
            b'?' as usize
        };
        self.blit_glyph_bytes(&font::FONT_8X16[index], col, row);
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
        if row >= self.screen_rows() {
            return;
        }
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
        if col >= self.screen_cols() || row >= self.screen_rows() {
            return;
        }
        self.blit_glyph_bytes(bytes, col, row);
    }

    /// Clear a single text cell to the background colour.
    ///
    /// Used to erase the cursor during the dark half of a blink cycle.
    /// Issues one `BltOp::VideoFill` call.
    pub fn clear_cell(&mut self, col: usize, row: usize) {
        if col >= self.screen_cols() || row >= self.screen_rows() {
            return;
        }
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

    /// Number of whole 8x16 text cells that fit between the top
    /// and bottom overscan margins on the current screen.
    pub fn screen_rows(&self) -> usize {
        (self.height.saturating_sub(2 * MARGIN_Y)) / CELL_H
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

    /// Encode `payload` as a QR code and render it into the digest
    /// region in the bottom-right of the framebuffer.
    ///
    /// Issues one `BltOp::BufferToVideo` call per QR module (Principle 6
    /// — the repeated 4x4 tile size lets the SPICE server's GLZ
    /// dictionary match aggressively). At `DIGEST_MODULE_PX = 4`, each
    /// module is a 4x4 buffer of either `FG` or `BG`. The full grid is
    /// `(DIGEST_QR_MODULES + 2 * DIGEST_QR_BORDER) = 45` modules square,
    /// yielding 2025 BltOp calls per invocation.
    ///
    /// The encoder is pinned to Version 5 and ECC level Low. Per the
    /// QR Code 2005 spec, Table 7, the byte-mode capacity for V5 by ECC
    /// level is L=106, M=84, Q=60, H=46. We pick L to match
    /// `DIGEST_PAYLOAD_CAPACITY = 106`; Medium would cap the payload at
    /// 84 bytes and any larger encoded record set would panic in
    /// `encode_binary` below with `DataTooLong`. The ECC level here and
    /// the capacity constant in `digest.rs` MUST agree — the
    /// `const _: () = assert!(...)` block in `digest.rs` enforces this
    /// at compile time. If you change one, change the other.
    ///
    /// Low ECC tolerates only ~7% of modules being unreadable (vs. ~15%
    /// for Medium), so the future CRT-scruff overlay needs to keep its
    /// damage budget conservative.
    ///
    /// Oversized payloads panic — `draw_digest` is a debug / smoke
    /// instrument and silent truncation would corrupt the decoded data
    /// without warning. Callers must size payloads to the Version 5 /
    /// Low capacity (106 bytes).
    ///
    /// `#[cfg_attr(not(feature = "digest-smoke"), allow(dead_code))]`
    /// is the transitive root suppression for the `DIGEST_*` constants
    /// this method consumes. The `digest-smoke` cargo feature provides
    /// the (sole) call site in `Scene::run_awaiting`; when that feature
    /// is on, the method and its constants are reachable and the
    /// suppression self-cleans. The suppression lives only here; the
    /// constants themselves carry no allow attribute.
    #[cfg_attr(not(feature = "digest-smoke"), allow(dead_code))]
    pub(crate) fn draw_digest(&mut self, payload: &[u8]) {
        // qrcodegen-no-heap's encode_binary expects the payload to sit
        // at the front of `dataandtempbuffer`, with the rest reserved
        // for scratch. Copy the caller's slice in, then encode.
        let mut data_and_temp = [0u8; DIGEST_QR_BUFFER_LEN];
        let mut out_buf = [0u8; DIGEST_QR_BUFFER_LEN];
        assert!(
            payload.len() <= data_and_temp.len(),
            "digest payload exceeds Version 5 buffer",
        );
        data_and_temp[..payload.len()].copy_from_slice(payload);

        let qr = QrCode::encode_binary(
            &mut data_and_temp,
            payload.len(),
            &mut out_buf,
            QrCodeEcc::Low,
            DIGEST_QR_VERSION_V, // min version: pinned to 5
            DIGEST_QR_VERSION_V, // max version: pinned to 5
            None,                // mask: auto
            false,               // boost_ecl: keep ECC at exactly Low
        )
        .expect("digest payload exceeds Version 5 / Low capacity (106 bytes)");

        const SIZE: usize = DIGEST_QR_MODULES;
        const BORDER: usize = DIGEST_QR_BORDER;
        const M: usize = DIGEST_MODULE_PX;
        let grid: usize = SIZE + 2 * BORDER;

        // Right-anchor at runtime: at 640x480 this matches the
        // DIGEST_REGION_X / DIGEST_REGION_Y constants exactly; at
        // larger modes the digest tracks the bottom-right corner
        // instead of floating at fixed pixel coordinates.
        let origin_x = self.width.saturating_sub(MARGIN_X + DIGEST_REGION_PX);
        let origin_y = self
            .height
            .saturating_sub(MARGIN_Y + CELL_H + DIGEST_REGION_PX);

        let mut tile = [BG; M * M];
        for grid_y in 0..grid {
            let mod_y = grid_y as i32 - BORDER as i32;
            for grid_x in 0..grid {
                let mod_x = grid_x as i32 - BORDER as i32;
                let colour = if qr.get_module(mod_x, mod_y) { FG } else { BG };
                tile.fill(colour);
                let dest_x = origin_x + grid_x * M;
                let dest_y = origin_y + grid_y * M;
                let _ = self.gop.blt(BltOp::BufferToVideo {
                    buffer: &tile,
                    src: BltRegion::Full,
                    dest: (dest_x, dest_y),
                    dims: (M, M),
                });
            }
        }
    }

    /// Compute CRC32C of every framebuffer pixel **outside** the
    /// right-anchored digest region.
    ///
    /// Path A of the phase-2 measurement
    /// (see `PLAN-visual-digest-phase-02-payload.md` *Outcome*):
    /// reads the framebuffer back one scanline at a time via
    /// `BltOp::VideoToBltBuffer` (the uefi-rs 0.37 spelling of the
    /// UEFI 2.10 `VideoToBuffer` op) and hashes every byte outside
    /// the digest region into a single CRC32C. The digest region
    /// itself is excluded so the hash does not depend on itself —
    /// the QR encodes a hash of everything-except-the-QR. Origin
    /// math mirrors `draw_digest` (same `saturating_sub`) so the
    /// excluded rectangle tracks the digest position on every mode.
    ///
    /// Per-row stack scratch is sized for the largest supported
    /// mode (1920 px wide → 7680 bytes of `BltPixel`), well within
    /// the UEFI stack. One `BltOp::VideoToBltBuffer` call per
    /// scanline avoids any framebuffer-sized allocation.
    ///
    /// Measured cost under OVMF+QXL at 1024x768: ~21.5M cycles per
    /// call (median over five runs), ~7 ms at 3 GHz. Phase 2 calls
    /// this three times per boot from `Scene::refresh_digest`, so
    /// the steady-state cost is ~21 ms of refresh-pause time per
    /// boot — concentrated at scene-phase boundaries rather than
    /// smeared across every paint site, which is the property that
    /// drove the A-over-B choice.
    #[cfg(feature = "digest-smoke")]
    pub(crate) fn crc32c_framebuffer_excluding_digest(&mut self) -> u32 {
        use crate::digest::CRC32C;

        // Per-row stack buffer, sized for the largest supported
        // mode. `BltPixel` is `#[repr(C)]` with four u8 fields
        // (blue, green, red, reserved) in uefi-rs 0.37, so
        // reinterpreting as `&[u8]` is sound.
        const MAX_WIDTH: usize = 1920;
        let mut row_buf: [BltPixel; MAX_WIDTH] = [BG; MAX_WIDTH];

        // Runtime right-anchored digest region — same math as
        // `draw_digest` so the excluded rectangle matches the
        // painted rectangle exactly.
        let origin_x = self.width.saturating_sub(MARGIN_X + DIGEST_REGION_PX);
        let origin_y = self
            .height
            .saturating_sub(MARGIN_Y + CELL_H + DIGEST_REGION_PX);
        let digest_x_end = origin_x + DIGEST_REGION_PX;
        let digest_y_end = origin_y + DIGEST_REGION_PX;

        let mut digester = CRC32C.digest();
        let row_width = self.width.min(MAX_WIDTH);

        for y in 0..self.height {
            // Read one scanline back from the framebuffer.
            let dest_slice = &mut row_buf[..row_width];
            let _ = self.gop.blt(BltOp::VideoToBltBuffer {
                buffer: dest_slice,
                src: (0, y),
                dest: BltRegion::Full,
                dims: (row_width, 1),
            });

            // Reinterpret as raw bytes.
            // SAFETY: BltPixel is `#[repr(C)]` and contains only
            // four u8 fields; the bit pattern is well-defined and
            // safe to read as a byte slice for the row's lifetime.
            let byte_slice = unsafe {
                core::slice::from_raw_parts(
                    row_buf.as_ptr() as *const u8,
                    row_width * core::mem::size_of::<BltPixel>(),
                )
            };

            if y < origin_y || y >= digest_y_end {
                // Whole row sits outside the digest band — hash
                // everything.
                digester.update(byte_slice);
            } else {
                // Row intersects the digest band — split around
                // the digest x-range so the digest pixels are
                // excluded.
                let px_size = core::mem::size_of::<BltPixel>();
                let left_end = origin_x.min(row_width);
                let right_start = digest_x_end.min(row_width);
                digester.update(&byte_slice[..left_end * px_size]);
                if right_start < row_width {
                    digester.update(&byte_slice[right_start * px_size..]);
                }
            }
        }

        digester.finalize()
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
