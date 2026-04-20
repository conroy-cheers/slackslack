pub(crate) mod common;
mod cpu;
pub(crate) mod gpu;

use crate::state::AppState;
use common::Texture;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub enum BillboardRenderer {
    Gpu(gpu::GpuRenderer),
    Cpu,
}

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = overlay_rect(frame.area());
    frame.render_widget(Clear, area);

    let title = format!(
        " {} :{}:  — Esc to close ",
        state.emoji_preview_char, state.emoji_preview_name
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height < 4 {
        return;
    }

    if state.emoji_preview_pending {
        let msg = Line::from(Span::styled(
            "  Loading emoji image...",
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(Paragraph::new(vec![msg]), inner);
        return;
    }

    if state.emoji_preview_frames.is_empty() {
        let msg = Line::from(Span::styled(
            "  No image available",
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(Paragraph::new(vec![msg]), inner);
        return;
    }

    let frame_idx = current_frame_index(state);
    let tex = Texture {
        pixels: &state.emoji_preview_frames[frame_idx],
        width: state.emoji_preview_tex_w,
        height: state.emoji_preview_tex_h,
    };

    let w = inner.width as usize;
    let h = inner.height as usize;
    let tick = state.emoji_preview_tick;

    let lines = match &mut state.billboard_renderer {
        BillboardRenderer::Gpu(gpu) => gpu.render_billboard(&tex, w, h, tick),
        BillboardRenderer::Cpu => cpu::render_billboard(&tex, w, h, tick),
    };

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn current_frame_index(state: &AppState) -> usize {
    let n = state.emoji_preview_frames.len();
    if n <= 1 {
        return 0;
    }
    let elapsed_ms = state.emoji_preview_tick * 50;
    let total_duration: u64 = state
        .emoji_preview_frame_delays
        .iter()
        .map(|&d| d.max(20) as u64)
        .sum();
    if total_duration == 0 {
        return 0;
    }
    let pos = elapsed_ms % total_duration;
    let mut accum = 0u64;
    for (i, &delay) in state.emoji_preview_frame_delays.iter().enumerate() {
        accum += delay.max(20) as u64;
        if pos < accum {
            return i;
        }
    }
    n - 1
}

pub fn overlay_rect(area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(90)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(90)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

pub fn decode_emoji_frames(
    data: &[u8],
) -> Option<(Vec<Vec<[u8; 4]>>, Vec<u32>, u32, u32)> {
    if data.len() >= 6 && (&data[0..4] == b"GIF8") {
        if let Ok(decoder) = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data)) {
            use image::AnimationDecoder;
            let frames: Vec<image::Frame> = decoder.into_frames().filter_map(|f| f.ok()).collect();
            if frames.len() > 1 {
                let w = frames[0].buffer().width();
                let h = frames[0].buffer().height();
                let mut rgba_frames = Vec::with_capacity(frames.len());
                let mut delays = Vec::with_capacity(frames.len());
                for f in &frames {
                    let buf = f.buffer();
                    let resized = if buf.width() != w || buf.height() != h {
                        image::imageops::resize(buf, w, h, image::imageops::FilterType::Nearest)
                    } else {
                        buf.clone()
                    };
                    rgba_frames.push(resized.pixels().map(|p| p.0).collect());
                    let (numer, denom) = f.delay().numer_denom_ms();
                    delays.push(if denom == 0 { numer } else { numer / denom });
                }
                return Some((rgba_frames, delays, w, h));
            }
        }
    }

    let img = image::load_from_memory(data).ok()?;
    let rgba = img.to_rgba8();
    let w = rgba.width();
    let h = rgba.height();
    let pixels: Vec<[u8; 4]> = rgba.pixels().map(|p| p.0).collect();
    Some((vec![pixels], vec![0], w, h))
}
