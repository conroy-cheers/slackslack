//! Kitty graphics protocol implementation for inline image display.
//!
//! After ratatui renders the TUI frame, this module writes APC escape sequences
//! to display images at positions recorded during render.

use crate::state::{AppState, CachedImage};
use base64::Engine;
use ratatui::layout::Rect;
use std::io::Write;

const CHUNK_SIZE: usize = 4096;

/// Delete all kitty images from the terminal.
pub fn clear_images(writer: &mut impl Write) -> std::io::Result<()> {
    write!(writer, "\x1b_Ga=d,d=A,q=2\x1b\\")?;
    Ok(())
}

/// Render all visible image placements using the kitty graphics protocol.
pub fn render_visible_images(writer: &mut impl Write, state: &AppState) -> std::io::Result<()> {
    let has_messages = state.messages_render_info.is_some() && !state.image_placements.is_empty();
    let has_thread = state.thread_render_info.is_some() && !state.thread_placements.is_empty();
    let has_inline = !state.inline_emoji_placements.is_empty();

    if !has_messages && !has_thread && !has_inline {
        return Ok(());
    }

    write!(writer, "\x1b[s")?;

    if let Some(info) = &state.messages_render_info {
        render_placements(
            writer,
            &state.image_placements,
            info.inner_x,
            info.inner_y,
            info.inner_height,
            info.scroll_y,
            state,
        )?;
    }

    if let Some(info) = &state.thread_render_info {
        render_placements(
            writer,
            &state.thread_placements,
            info.inner_x,
            info.inner_y,
            info.inner_height,
            info.scroll_y,
            state,
        )?;
    }

    render_inline_emoji(writer, state)?;

    write!(writer, "\x1b[u")?;
    writer.flush()?;
    Ok(())
}

fn render_inline_emoji(writer: &mut impl Write, state: &AppState) -> std::io::Result<()> {
    for p in &state.inline_emoji_placements {
        if is_occluded(
            p.screen_row,
            p.screen_col,
            p.display_rows,
            p.display_cols,
            &state.occlusion_rects,
        ) {
            continue;
        }
        let cached = if let Some(uid) = p.emoji_key.strip_prefix("avatar:") {
            match state.avatar_images.get(uid) {
                Some(c) => c,
                None => continue,
            }
        } else {
            match state.custom_emoji_images.get(&p.emoji_key) {
                Some(c) => c,
                None => continue,
            }
        };
        display_image(
            writer,
            p.screen_row,
            p.screen_col,
            p.display_cols,
            p.display_rows,
            &cached.png_data,
            None,
        )?;
    }
    Ok(())
}

fn render_placements(
    writer: &mut impl Write,
    placements: &[crate::state::ImagePlacement],
    inner_x: u16,
    inner_y: u16,
    inner_height: u16,
    scroll_y: usize,
    state: &AppState,
) -> std::io::Result<()> {
    let visible_end = scroll_y + inner_height as usize;

    for placement in placements {
        let line = placement.line;
        let img_end = line + placement.display_rows as usize;

        if img_end <= scroll_y || line >= visible_end {
            continue;
        }

        let cached = match state
            .image_cache
            .get(&placement.url)
            .or_else(|| state.custom_emoji_images.get(&placement.url))
        {
            Some(c) => c,
            None => continue,
        };

        let top_crop_rows = if line < scroll_y {
            (scroll_y - line) as u16
        } else {
            0
        };
        let offset_in_view = if line >= scroll_y {
            (line - scroll_y) as u16
        } else {
            0
        };
        let screen_row = inner_y + offset_in_view;
        let screen_col = inner_x + placement.col;

        let rows_remaining = inner_height.saturating_sub(offset_in_view);
        let visible_rows = (placement.display_rows - top_crop_rows).min(rows_remaining);

        if visible_rows == 0 {
            continue;
        }

        if is_occluded(
            screen_row,
            screen_col,
            visible_rows,
            placement.display_cols,
            &state.occlusion_rects,
        ) {
            continue;
        }

        let crop = if top_crop_rows > 0 || visible_rows < placement.display_rows {
            Some(VerticalCrop {
                top_crop_rows,
                visible_rows,
                total_rows: placement.display_rows,
                img_height: cached.height,
            })
        } else {
            None
        };

        display_image(
            writer,
            screen_row,
            screen_col,
            placement.display_cols,
            visible_rows,
            &cached.png_data,
            crop,
        )?;
    }
    Ok(())
}

struct VerticalCrop {
    top_crop_rows: u16,
    visible_rows: u16,
    total_rows: u16,
    img_height: u32,
}

fn is_occluded(row: u16, col: u16, rows: u16, cols: u16, rects: &[Rect]) -> bool {
    for r in rects {
        if col < r.x + r.width && col + cols > r.x && row < r.y + r.height && row + rows > r.y {
            return true;
        }
    }
    false
}

/// Display a single image at the given screen position using kitty protocol.
/// If `crop` is provided, the source image is re-encoded with vertical cropping.
fn display_image(
    writer: &mut impl Write,
    row: u16,
    col: u16,
    cols: u16,
    rows: u16,
    png_data: &[u8],
    crop: Option<VerticalCrop>,
) -> std::io::Result<()> {
    write!(writer, "\x1b[{};{}H", row + 1, col + 1)?;

    let cropped;
    let data: &[u8] = if let Some(crop) = crop {
        cropped = crop_image_vertical(png_data, &crop);
        cropped.as_deref().unwrap_or(png_data)
    } else {
        png_data
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let total_len = b64.len();

    if total_len <= CHUNK_SIZE {
        write!(
            writer,
            "\x1b_Ga=T,f=100,t=d,c={},r={},q=2;{}\x1b\\",
            cols, rows, &b64
        )?;
    } else {
        let mut offset = 0;
        let mut first = true;
        while offset < total_len {
            let end = (offset + CHUNK_SIZE).min(total_len);
            let chunk = &b64[offset..end];
            let more = if end < total_len { 1 } else { 0 };

            if first {
                write!(
                    writer,
                    "\x1b_Ga=T,f=100,t=d,c={},r={},q=2,m={};{}\x1b\\",
                    cols, rows, more, chunk
                )?;
                first = false;
            } else {
                write!(writer, "\x1b_Gm={};{}\x1b\\", more, chunk)?;
            }
            offset = end;
        }
    }

    Ok(())
}

fn crop_image_vertical(png_data: &[u8], crop: &VerticalCrop) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png_data).ok()?;
    let h = img.height();
    let w = img.width();
    let pixels_per_row = h as f64 / crop.total_rows as f64;
    let y_start = (crop.top_crop_rows as f64 * pixels_per_row).round() as u32;
    let y_height = (crop.visible_rows as f64 * pixels_per_row).round() as u32;
    let y_height = y_height.min(h.saturating_sub(y_start)).max(1);
    let cropped = img.crop_imm(0, y_start, w, y_height);
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    cropped
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}

/// Encode raw image bytes (JPEG, PNG, etc.) into PNG for kitty protocol.
/// Returns the PNG bytes and (width, height).
pub fn encode_as_png(data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(data).ok()?;
    let width = img.width();
    let height = img.height();

    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;

    Some((png_bytes, width, height))
}

/// Query actual terminal cell pixel dimensions via TIOCGWINSZ ioctl.
/// Returns (cell_width, cell_height) in pixels, or None if unavailable.
fn get_cell_pixel_size() -> Option<(f64, f64)> {
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    let mut ws = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // TIOCGWINSZ = 0x5413 on Linux
    let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 || ws.ws_col == 0 || ws.ws_row == 0 {
        return None;
    }
    let cell_w = ws.ws_xpixel as f64 / ws.ws_col as f64;
    let cell_h = ws.ws_ypixel as f64 / ws.ws_row as f64;
    Some((cell_w, cell_h))
}

/// Compute display cell dimensions for an image, maintaining aspect ratio.
/// Uses actual terminal cell pixel size when available. Limits image width
/// to a reasonable size rather than stretching across the full pane.
pub fn compute_display_size(img_w: u32, img_h: u32, max_cols: u16) -> (u16, u16) {
    if img_w == 0 || img_h == 0 {
        return (max_cols.min(40), 6);
    }

    let (cell_pw, cell_ph) = get_cell_pixel_size().unwrap_or((8.0, 16.0));
    let cell_aspect = cell_ph / cell_pw;

    // How many columns would the image naturally occupy at 1:1 pixel mapping?
    let natural_cols = (img_w as f64 / cell_pw).ceil() as u16;
    let cols = natural_cols.min(max_cols).max(4);

    let aspect = img_h as f64 / img_w as f64;
    let rows = (cols as f64 * aspect / cell_aspect).round() as u16;
    let rows = rows.clamp(2, 30);
    (cols, rows)
}

/// Store a downloaded image in the cache.
pub fn cache_image(state: &mut AppState, url: String, png_data: Vec<u8>, width: u32, height: u32) {
    state.pending_images.remove(&url);
    state.image_cache.insert(
        url,
        CachedImage {
            png_data,
            width,
            height,
        },
    );
    state.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ImagePlacement, MessagesRenderInfo};

    /// Parse \x1b[row;colH cursor-position sequences from raw terminal output.
    fn extract_cursor_positions(data: &[u8]) -> Vec<(u16, u16)> {
        let s = String::from_utf8_lossy(data);
        let bytes = s.as_bytes();
        let mut positions = Vec::new();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
                i += 2;
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'H' {
                    let params = &s[start..i];
                    if let Some((row_s, col_s)) = params.split_once(';') {
                        if let (Ok(row), Ok(col)) = (row_s.parse::<u16>(), col_s.parse::<u16>()) {
                            positions.push((row, col));
                        }
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        positions
    }

    /// Parse kitty protocol r= (rows) parameter from escape sequences.
    fn extract_kitty_rows(data: &[u8]) -> Vec<u16> {
        let s = String::from_utf8_lossy(data);
        let mut rows = Vec::new();
        for part in s.split("r=") {
            if let Some(end) = part.find(|c: char| !c.is_ascii_digit()) {
                if let Ok(n) = part[..end].parse::<u16>() {
                    if n > 0 {
                        rows.push(n);
                    }
                }
            }
        }
        rows
    }

    fn make_state(
        placements: Vec<ImagePlacement>,
        info: MessagesRenderInfo,
        urls: &[&str],
    ) -> AppState {
        let mut state = AppState::new();
        state.image_placements = placements;
        state.messages_render_info = Some(info);
        for url in urls {
            state.image_cache.insert(
                url.to_string(),
                CachedImage {
                    png_data: vec![0x89, 0x50, 0x4E, 0x47], // PNG header stub
                    width: 100,
                    height: 100,
                },
            );
        }
        state
    }

    #[test]
    fn image_at_top_of_viewport() {
        let info = MessagesRenderInfo {
            inner_x: 5,
            inner_y: 2,
            inner_height: 20,
            scroll_y: 0,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 0,
            col: 2,
            display_cols: 10,
            display_rows: 3,
        }];
        let state = make_state(placements, info, &["img"]);

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 1);
        // screen_row = inner_y + 0 = 2, terminal writes row+1 = 3
        assert_eq!(positions[0].0, 3);
    }

    #[test]
    fn image_at_bottom_is_clipped() {
        let info = MessagesRenderInfo {
            inner_x: 0,
            inner_y: 0,
            inner_height: 20,
            scroll_y: 0,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 18,
            col: 2,
            display_cols: 10,
            display_rows: 10,
        }];
        let state = make_state(placements, info, &["img"]);

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        // Image at line 18, viewport height 20 → only 2 rows remain
        let rows = extract_kitty_rows(&buf);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], 2, "image should be clipped to 2 rows");
    }

    #[test]
    fn offscreen_images_not_rendered() {
        let info = MessagesRenderInfo {
            inner_x: 0,
            inner_y: 0,
            inner_height: 20,
            scroll_y: 50,
        };
        let placements = vec![
            ImagePlacement {
                url: "before".into(),
                line: 10,
                col: 2,
                display_cols: 10,
                display_rows: 3,
            },
            ImagePlacement {
                url: "after".into(),
                line: 80,
                col: 2,
                display_cols: 10,
                display_rows: 3,
            },
        ];
        let state = make_state(placements, info, &["before", "after"]);

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 0, "offscreen images should not render");
    }

    #[test]
    fn scrolled_image_at_correct_position() {
        let info = MessagesRenderInfo {
            inner_x: 5,
            inner_y: 2,
            inner_height: 20,
            scroll_y: 100,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 110,
            col: 2,
            display_cols: 10,
            display_rows: 3,
        }];
        let state = make_state(placements, info, &["img"]);

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 1);
        // offset_in_view = 110 - 100 = 10, screen_row = 2 + 10 = 12, terminal = 13
        assert_eq!(positions[0].0, 13);
    }

    /// Sweep every scroll position — no image should ever render outside the viewport.
    #[test]
    fn all_scroll_positions_produce_valid_coordinates() {
        let total_lines: usize = 200;
        let viewport_height: u16 = 20;
        let inner_y: u16 = 3;

        // Images every 15 lines
        let urls: Vec<String> = (0..total_lines)
            .step_by(15)
            .map(|i| format!("img_{}", i))
            .collect();
        let url_refs: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();

        for scroll_y in 0..=total_lines {
            let placements: Vec<_> = (0..total_lines)
                .step_by(15)
                .map(|line| ImagePlacement {
                    url: format!("img_{}", line),
                    line,
                    col: 2,
                    display_cols: 10,
                    display_rows: 5,
                })
                .collect();

            let info = MessagesRenderInfo {
                inner_x: 5,
                inner_y,
                inner_height: viewport_height,
                scroll_y,
            };
            let state = make_state(placements, info, &url_refs);

            let mut buf = Vec::new();
            render_visible_images(&mut buf, &state).unwrap();

            let positions = extract_cursor_positions(&buf);
            let min_row = inner_y + 1; // 1-indexed
            let max_row = inner_y + viewport_height; // 1-indexed

            for (row, _col) in &positions {
                assert!(
                    *row >= min_row && *row <= max_row,
                    "scroll_y={}: row {} outside viewport [{}, {}]",
                    scroll_y,
                    row,
                    min_row,
                    max_row
                );
            }

            // Verify clipping: no kitty r= value exceeds remaining rows
            let kitty_rows = extract_kitty_rows(&buf);
            for (i, (row, _)) in positions.iter().enumerate() {
                if let Some(&r) = kitty_rows.get(i) {
                    let remaining = (inner_y + viewport_height + 1).saturating_sub(*row);
                    assert!(
                        r <= remaining,
                        "scroll_y={}: kitty rows {} exceeds remaining {} at row {}",
                        scroll_y,
                        r,
                        remaining,
                        row
                    );
                }
            }
        }
    }

    /// Regression: the old code cast visible_end to u16, which wraps for large scroll_y.
    #[test]
    fn large_scroll_offset_no_overflow() {
        let info = MessagesRenderInfo {
            inner_x: 0,
            inner_y: 0,
            inner_height: 20,
            scroll_y: 60000,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 60005,
            col: 2,
            display_cols: 10,
            display_rows: 3,
        }];
        let state = make_state(placements, info, &["img"]);

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 1);
        // offset = 5, screen_row = 0 + 5 = 5, terminal = 6
        assert_eq!(positions[0].0, 6);

        let rows = extract_kitty_rows(&buf);
        assert_eq!(rows[0], 3, "image should not be clipped at offset 5 of 20");
    }

    #[test]
    fn is_occluded_no_rects() {
        assert!(!is_occluded(5, 5, 3, 10, &[]));
    }

    #[test]
    fn is_occluded_full_overlap() {
        let rects = vec![Rect::new(0, 0, 80, 40)];
        assert!(is_occluded(5, 5, 3, 10, &rects));
    }

    #[test]
    fn is_occluded_partial_overlap() {
        let rects = vec![Rect::new(10, 10, 20, 20)];
        // Image at (5, 8) with 3 rows, 10 cols → extends to (8, 18), overlaps rect starting at (10, 10)
        assert!(is_occluded(5, 8, 6, 10, &rects));
    }

    #[test]
    fn is_occluded_no_overlap() {
        let rects = vec![Rect::new(50, 50, 20, 20)];
        assert!(!is_occluded(5, 5, 3, 10, &rects));
    }

    #[test]
    fn is_occluded_adjacent_not_overlapping() {
        // Rect ends exactly where image starts
        let rects = vec![Rect::new(0, 0, 10, 10)];
        assert!(!is_occluded(10, 10, 3, 5, &rects));
    }

    #[test]
    fn occluded_image_not_rendered() {
        let info = MessagesRenderInfo {
            inner_x: 0,
            inner_y: 0,
            inner_height: 40,
            scroll_y: 0,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 5,
            col: 5,
            display_cols: 10,
            display_rows: 3,
        }];
        let mut state = make_state(placements, info, &["img"]);
        // Overlay covering the image area
        state.occlusion_rects.push(Rect::new(0, 0, 40, 20));

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 0, "occluded image should not render");
    }

    #[test]
    fn non_occluded_image_still_renders() {
        let info = MessagesRenderInfo {
            inner_x: 0,
            inner_y: 0,
            inner_height: 40,
            scroll_y: 0,
        };
        let placements = vec![ImagePlacement {
            url: "img".into(),
            line: 5,
            col: 5,
            display_cols: 10,
            display_rows: 3,
        }];
        let mut state = make_state(placements, info, &["img"]);
        // Overlay in a different area
        state.occlusion_rects.push(Rect::new(50, 50, 20, 20));

        let mut buf = Vec::new();
        render_visible_images(&mut buf, &state).unwrap();

        let positions = extract_cursor_positions(&buf);
        assert_eq!(positions.len(), 1, "non-occluded image should still render");
    }
}
