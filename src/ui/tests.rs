use crate::slack::types::{Channel, ChannelIdsPage, ChannelSection, Message, SlackFile};
use crate::state::{AppState, CachedImage, EmojiPickerSource, InputMode};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn msg(text: &str, ts: &str) -> Message {
    Message {
        user: Some("U1".into()),
        text: text.into(),
        ts: ts.into(),
        thread_ts: None,
        reply_count: None,
        reactions: Vec::new(),
        edited: None,
        subtype: None,
        bot_id: None,
        username: None,
        files: Vec::new(),
    }
}

fn msg_with_image(text: &str, ts: &str, url: &str) -> Message {
    let mut m = msg(text, ts);
    m.files.push(SlackFile {
        id: "F1".into(),
        name: "photo.png".into(),
        title: None,
        mimetype: Some("image/png".into()),
        filetype: Some("png".into()),
        pretty_type: None,
        user: None,
        url_private_download: None,
        url_private: None,
        thumb_360: Some(url.into()),
        thumb_480: None,
        thumb_160: None,
        thumb_360_w: 360,
        thumb_360_h: 240,
        size: None,
        created: None,
        timestamp: None,
        channels: Vec::new(),
    });
    m
}

fn test_channel() -> Channel {
    Channel {
        id: "C1".into(),
        name: Some("general".into()),
        is_channel: true,
        is_im: false,
        is_mpim: false,
        is_private: false,
        is_member: true,
        user: None,
        topic: None,
        purpose: None,
        last_read: None,
        unread_count: 0,
        unread_count_display: 0,
    }
}

fn state_with_image_messages(count: usize, image_every: usize) -> AppState {
    let mut state = AppState::new();
    state.channels = vec![test_channel()];
    state.selected_channel_idx = 0;
    state.focus = crate::state::Focus::Messages;

    let mut messages = Vec::new();
    let mut cached_urls = Vec::new();
    for i in 0..count {
        if image_every > 0 && i % image_every == 0 {
            let url = format!("http://img/{}", i);
            messages.push(msg_with_image(
                &format!("Image message {}", i),
                &format!("{}.000000", 1700000000 + i),
                &url,
            ));
            cached_urls.push(url);
        } else {
            messages.push(msg(
                &format!("Text message {}", i),
                &format!("{}.000000", 1700000000 + i),
            ));
        }
    }

    let cd = state
        .channel_data
        .entry("C1".into())
        .or_insert_with(crate::state::ChannelData::new);
    cd.messages = messages.into();
    for url in &cached_urls {
        state.image_cache.insert(
            url.clone(),
            CachedImage {
                png_data: vec![0; 8],
                width: 360,
                height: 240,
            },
        );
    }
    state
}

/// After a full render, all image placements must have valid screen coordinates.
fn assert_placements_valid(state: &AppState) {
    let info = match &state.messages_render_info {
        Some(info) => info,
        None => return,
    };

    for (i, p) in state.image_placements.iter().enumerate() {
        let visible_start = info.scroll_y;
        let visible_end = info.scroll_y + info.inner_height as usize;

        // Placement line must be in the renderable range
        assert!(p.display_rows > 0, "placement[{}]: zero display_rows", i);
        assert!(p.display_cols > 0, "placement[{}]: zero display_cols", i);

        // If the placement would be visible, verify coordinates
        if p.line >= visible_start && p.line < visible_end {
            let offset = (p.line - visible_start) as u16;
            let screen_row = info.inner_y + offset;

            // Must not extend past viewport
            let max_visible_row = info.inner_y + info.inner_height;
            assert!(
                screen_row < max_visible_row,
                "placement[{}]: screen_row {} >= viewport bottom {}",
                i,
                screen_row,
                max_visible_row
            );

            // Clipped rows must fit
            let rows_remaining = info.inner_height - offset;
            let effective_rows = p.display_rows.min(rows_remaining);
            assert!(
                screen_row + effective_rows <= max_visible_row,
                "placement[{}]: image extends to row {} past viewport bottom {}",
                i,
                screen_row + effective_rows,
                max_visible_row
            );
        }
    }
}

/// Render a full frame and validate image placement invariants.
#[test]
fn full_frame_image_placements_valid() {
    let mut state = state_with_image_messages(30, 5);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();

    assert_placements_valid(&state);
}

/// Scroll through every message position and validate after each render.
#[test]
fn scroll_all_positions_placements_valid() {
    let msg_count = 30;
    let mut state = state_with_image_messages(msg_count, 3);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    for sel in 0..msg_count {
        state.selected_message_idx = sel;
        state.dirty = true;

        terminal
            .draw(|frame| super::render(frame, &mut state))
            .unwrap();

        assert_placements_valid(&state);
    }
}

/// Multiple terminal sizes should not produce invalid placements.
#[test]
fn various_terminal_sizes() {
    for (w, h) in [(40, 10), (80, 24), (120, 40), (200, 60)] {
        let mut state = state_with_image_messages(20, 4);
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| super::render(frame, &mut state))
            .unwrap();

        assert_placements_valid(&state);
    }
}

/// Tiny terminal where messages area is very small.
#[test]
fn tiny_terminal_no_panic() {
    let mut state = state_with_image_messages(10, 2);
    let backend = TestBackend::new(30, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    // Should not panic even if viewport is too small for images
    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();

    assert_placements_valid(&state);
}

/// Full render with channel sections populated — no panics.
#[test]
fn channel_sections_render_no_panic() {
    let mut state = state_with_image_messages(10, 0);
    state.channels.push(Channel {
        id: "C2".into(),
        name: Some("random".into()),
        is_channel: true,
        is_im: false,
        is_mpim: false,
        is_private: false,
        is_member: true,
        user: None,
        topic: None,
        purpose: None,
        last_read: None,
        unread_count: 0,
        unread_count_display: 0,
    });
    state.channel_sections = vec![ChannelSection {
        channel_section_id: "S1".into(),
        name: "Important".into(),
        emoji: "\u{1F525}".into(),
        channel_ids_page: ChannelIdsPage {
            channel_ids: vec!["C1".into()],
        },
        is_collapsed: false,
        sort_order: 0,
    }];

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();
}

/// Full render with emoji picker open — no panics.
#[test]
fn emoji_picker_render_no_panic() {
    let mut state = state_with_image_messages(5, 0);
    state.open_emoji_picker(EmojiPickerSource::Reaction, vec![]);
    assert_eq!(state.input_mode, InputMode::EmojiPicker);

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();
}

/// Full render with emoji 3D preview open — no panics.
#[test]
fn emoji_preview_render_no_panic() {
    let mut state = state_with_image_messages(5, 0);
    state.input_mode = InputMode::EmojiPreview;
    state.emoji_preview_char = "\u{1F525}".into();
    state.emoji_preview_name = "fire".into();
    state.emoji_preview_tick = 0;

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();

    // Advance a few ticks and re-render
    for t in 1..10 {
        state.emoji_preview_tick = t;
        state.dirty = true;
        terminal
            .draw(|frame| super::render(frame, &mut state))
            .unwrap();
    }
}

/// Scrollbar rendering does not affect image placement coordinates (regression).
#[test]
fn scrollbar_does_not_affect_image_placements() {
    let mut state = state_with_image_messages(30, 5);
    state.focus = crate::state::Focus::Messages;
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| super::render(frame, &mut state))
        .unwrap();

    assert_placements_valid(&state);
}

/// End-to-end: render, then simulate render_visible_images output
/// and verify all escape sequence coordinates are in bounds.
#[test]
fn end_to_end_escape_coordinates() {
    let mut state = state_with_image_messages(40, 5);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    for sel in (0..40).step_by(3) {
        state.selected_message_idx = sel;
        state.dirty = true;

        terminal
            .draw(|frame| super::render(frame, &mut state))
            .unwrap();

        let mut buf = Vec::new();
        super::images::render_visible_images(&mut buf, &state).unwrap();

        // Parse cursor positions from escape sequences
        let s = String::from_utf8_lossy(&buf);
        let bytes = s.as_bytes();
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
                        let row: u16 = row_s.parse().unwrap();
                        let col: u16 = col_s.parse().unwrap();
                        // Terminal is 120x40 (1-indexed: rows 1-40, cols 1-120)
                        assert!(
                            row >= 1 && row <= 40,
                            "sel={}: row {} out of terminal bounds",
                            sel,
                            row
                        );
                        assert!(
                            col >= 1 && col <= 120,
                            "sel={}: col {} out of terminal bounds",
                            sel,
                            col
                        );
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }
}
