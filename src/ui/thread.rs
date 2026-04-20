use crate::state::{AppState, Focus, ImagePlacement, ThreadRenderInfo};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

pub fn render(frame: &mut Frame, state: &mut AppState, area: Rect) {
    state.thread_placements.clear();
    state.thread_render_info = None;

    let is_focused = state.focus == Focus::Thread;

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = " Thread ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    let width = inner.width as usize;
    let height = inner.height as usize;

    if width == 0 || height == 0 {
        frame.render_widget(block, area);
        return;
    }

    let (lines, placements, emoji_needed, avatars) = build_thread_lines(state, width);
    let total_lines = lines.len();

    state.thread_max_scroll_offset = total_lines.saturating_sub(height);
    if state.thread_scroll_offset > state.thread_max_scroll_offset {
        state.thread_scroll_offset = state.thread_max_scroll_offset;
    }

    let scroll_y = state
        .thread_max_scroll_offset
        .saturating_sub(state.thread_scroll_offset);

    state.thread_placements = placements;
    state.thread_render_info = Some(ThreadRenderInfo {
        inner_x: inner.x,
        inner_y: inner.y,
        inner_height: inner.height,
        scroll_y,
    });
    state.emoji_load_queue.extend(emoji_needed);

    // Convert avatar virtual line placements to screen coordinates
    let visible_end = scroll_y + inner.height as usize;
    for (user_id, vline) in &avatars {
        if *vline < scroll_y || *vline >= visible_end {
            continue;
        }
        let screen_row = inner.y + (*vline - scroll_y) as u16;
        let screen_col = inner.x;
        if state.avatar_images.contains_key(user_id.as_str()) {
            state.inline_emoji_placements.push(crate::state::InlineEmojiPlacement {
                emoji_key: format!("avatar:{}", user_id),
                screen_row,
                screen_col,
                display_cols: 2,
                display_rows: 1,
            });
        } else {
            state.request_avatar(user_id);
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll_y as u16, 0));

    frame.render_widget(paragraph, area);

    // Scrollbar
    if total_lines > height {
        let thumb_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(thumb_style)
            .track_style(Style::default().fg(Color::DarkGray));
        let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll_y);
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn build_thread_lines(
    state: &AppState,
    width: usize,
) -> (Vec<Line<'static>>, Vec<ImagePlacement>, Vec<String>, Vec<(String, usize)>) {
    let msgs = match state.thread_messages() {
        Some(msgs) if !msgs.is_empty() => msgs,
        _ => {
            return (
                vec![Line::from(Span::styled(
                    "  Loading thread...".to_string(),
                    Style::default().fg(Color::DarkGray),
                ))],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        }
    };

    let mut result = Vec::new();
    let mut placements = Vec::new();
    let mut emoji_needed = Vec::new();
    let mut avatars: Vec<(String, usize)> = Vec::new();
    let msg_count = msgs.len();

    for (i, msg) in msgs.iter().enumerate() {
        if let Some(uid) = &msg.user {
            avatars.push((uid.clone(), result.len()));
        }
        render_thread_message(msg, state, width, i == 0, &mut result, &mut placements, &mut emoji_needed);
        if i + 1 < msg_count {
            result.push(Line::from(""));
        }
    }

    (result, placements, emoji_needed, avatars)
}

fn render_thread_message(
    msg: &crate::slack::types::Message,
    state: &AppState,
    width: usize,
    is_parent: bool,
    out: &mut Vec<Line<'static>>,
    placements: &mut Vec<ImagePlacement>,
    emoji_needed: &mut Vec<String>,
) {
    let username = msg
        .user
        .as_ref()
        .map(|uid| state.user_display_name(uid).to_string())
        .or_else(|| msg.username.clone())
        .unwrap_or_else(|| "unknown".into());

    let time = format_timestamp(&msg.ts);

    let name_color = msg.user.as_ref()
        .map(|uid| state.user_color(uid))
        .unwrap_or(Color::Green);
    let name_style = if is_parent {
        Style::default()
            .fg(name_color)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(name_color)
            .add_modifier(Modifier::BOLD)
    };

    let header_spans: Vec<Span<'static>> = vec![
        Span::styled("  ".to_string(), Style::default()), // avatar placeholder
        Span::styled(username, name_style),
        Span::styled("  ", Style::default()),
        Span::styled(time, Style::default().fg(Color::DarkGray)),
    ];
    out.push(Line::from(header_spans));

    // Body text with rich formatting and custom emoji
    let body_lines = super::messages::render_rich_text_pub(&msg.text, state, width.saturating_sub(2));
    for line_spans in body_lines {
        let line_idx = out.len();
        let mut final_spans = vec![Span::styled("  ".to_string(), Style::default())];
        let mut col: u16 = 2;
        for span in line_spans {
            let processed = super::messages::replace_custom_emoji_in_span_pub(
                &span.content,
                span.style,
                line_idx,
                &mut col,
                state,
                placements,
                emoji_needed,
            );
            final_spans.extend(processed);
        }
        out.push(Line::from(final_spans));
    }

    // Reactions
    if !msg.reactions.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![Span::from(String::from("  "))];
        let mut col: u16 = 2;
        let reaction_line = out.len();
        for (i, r) in msg.reactions.iter().enumerate() {
            if i > 0 {
                spans.push(Span::from(String::from(" ")));
                col += 1;
            }
            super::messages::render_reaction_emoji(&r.name, state, reaction_line, &mut col, &mut spans, placements, emoji_needed);
            let count_str = format!(" {}", r.count);
            col += count_str.len() as u16;
            spans.push(Span::styled(count_str, Style::default().fg(Color::DarkGray)));
        }
        out.push(Line::from(spans));
    }
}

fn format_timestamp(ts: &str) -> String {
    let unix_secs: f64 = ts.parse().unwrap_or(0.0);
    if unix_secs == 0.0 {
        return ts.to_string();
    }
    let dt = chrono::DateTime::from_timestamp(unix_secs as i64, 0);
    match dt {
        Some(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            let now = chrono::Local::now();
            if local.date_naive() == now.date_naive() {
                local.format("%H:%M").to_string()
            } else {
                local.format("%b %d %H:%M").to_string()
            }
        }
        None => ts.to_string(),
    }
}
