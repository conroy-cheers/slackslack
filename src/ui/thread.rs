use crate::state::{AppState, Focus};
use crate::ui::emoji;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use unicode_width::UnicodeWidthStr;

pub fn render(frame: &mut Frame, state: &mut AppState, area: Rect) {
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

    let lines = build_thread_lines(state, width);
    let total_lines = lines.len();

    state.thread_max_scroll_offset = total_lines.saturating_sub(height);
    if state.thread_scroll_offset > state.thread_max_scroll_offset {
        state.thread_scroll_offset = state.thread_max_scroll_offset;
    }

    let scroll_y = state
        .thread_max_scroll_offset
        .saturating_sub(state.thread_scroll_offset);

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

fn build_thread_lines(state: &AppState, width: usize) -> Vec<Line<'static>> {
    let msgs = match state.thread_messages() {
        Some(msgs) if !msgs.is_empty() => msgs,
        _ => {
            return vec![Line::from(Span::styled(
                "  Loading thread...".to_string(),
                Style::default().fg(Color::DarkGray),
            ))];
        }
    };

    let mut result = Vec::new();
    let msg_count = msgs.len();

    for (i, msg) in msgs.iter().enumerate() {
        render_thread_message(msg, state, width, i == 0, &mut result);
        if i + 1 < msg_count {
            result.push(Line::from(""));
        }
    }

    result
}

fn render_thread_message(
    msg: &crate::slack::types::Message,
    state: &AppState,
    width: usize,
    is_parent: bool,
    out: &mut Vec<Line<'static>>,
) {
    let username = msg
        .user
        .as_ref()
        .map(|uid| state.user_display_name(uid).to_string())
        .or_else(|| msg.username.clone())
        .unwrap_or_else(|| "unknown".into());

    let time = format_timestamp(&msg.ts);

    let name_style = if is_parent {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };

    let header_spans: Vec<Span<'static>> = vec![
        Span::styled(username, name_style),
        Span::styled("  ", Style::default()),
        Span::styled(time, Style::default().fg(Color::DarkGray)),
    ];
    out.push(Line::from(header_spans));

    // Body text
    let text = emoji::replace_emoji_shortcodes(
        &super::messages::resolve_slack_markup_pub(&msg.text, state),
    );
    let usable = width.saturating_sub(2);
    for text_line in text.lines() {
        if usable == 0 {
            out.push(Line::from(format!("  {}", text_line)));
            continue;
        }
        for wrapped in wrap_line(text_line, usable) {
            out.push(Line::from(format!("  {}", wrapped)));
        }
    }

    // Reactions
    if !msg.reactions.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![Span::from(String::from("  "))];
        for (i, r) in msg.reactions.iter().enumerate() {
            if i > 0 {
                spans.push(Span::from(String::from(" ")));
            }
            let emoji_display = emoji::emoji_for(&r.name)
                .map(|e| e.to_string())
                .unwrap_or_else(|| format!(":{}:", r.name));
            spans.push(Span::styled(emoji_display, Style::default().fg(Color::Yellow)));
            spans.push(Span::styled(
                format!(" {}", r.count),
                Style::default().fg(Color::DarkGray),
            ));
        }
        out.push(Line::from(spans));
    }
}

fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![line.to_string()];
    }
    let line_width = UnicodeWidthStr::width(line);
    if line_width <= max_width {
        return vec![line.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    for word in line.split_inclusive(|c: char| c.is_whitespace()) {
        let word_width = UnicodeWidthStr::width(word);
        if current_width + word_width > max_width && current_width > 0 {
            lines.push(current.clone());
            current.clear();
            current_width = 0;
        }
        current.push_str(word);
        current_width += word_width;
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
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
