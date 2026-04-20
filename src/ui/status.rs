use crate::state::{AppState, InputMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let mode = match state.input_mode {
        InputMode::Normal => " NORMAL ",
        InputMode::Insert => " INSERT ",
        InputMode::Search => " SEARCH ",
        InputMode::MessageSearch => " SEARCH ",
        InputMode::Reaction => " REACT ",
        InputMode::EmojiPicker => " EMOJI ",
        InputMode::UserPicker => " @USER ",
        InputMode::GlobalSearch => " SEARCH ",
        InputMode::FilePath => " UPLOAD ",
    };
    let mode_style = match state.input_mode {
        InputMode::Normal => Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        InputMode::Insert => Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD),
        InputMode::Search | InputMode::MessageSearch | InputMode::GlobalSearch => Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        InputMode::Reaction | InputMode::EmojiPicker | InputMode::UserPicker => Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        InputMode::FilePath => Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    };

    let conn = if state.connected {
        Span::styled(" connected", Style::default().fg(Color::Green))
    } else {
        Span::styled(" disconnected", Style::default().fg(Color::Red))
    };

    let channel_info = state
        .active_channel()
        .map(|c| {
            let prefix = if c.is_im { "" } else { "#" };
            format!(" | {}{}", prefix, c.display_name())
        })
        .unwrap_or_default();

    // Message position indicator
    let msg_count = state.message_count();
    let scroll_info = if msg_count > 0 {
        let pos = msg_count.saturating_sub(state.selected_message_idx);
        format!(" | {}/{}", pos, msg_count)
    } else {
        String::new()
    };

    // Search indicator
    let search_info = if state.message_search_active && !state.message_search_results.is_empty() {
        format!(
            " | /{} [{}/{}]",
            state.message_search_query,
            state.message_search_idx + 1,
            state.message_search_results.len()
        )
    } else {
        String::new()
    };

    // Thread indicator
    let thread_info = if state.thread_channel_id.is_some() {
        " | thread"
    } else {
        ""
    };

    let mut spans = vec![
        Span::styled(mode, mode_style),
        conn,
        Span::styled(channel_info, Style::default().fg(Color::White)),
        Span::styled(scroll_info, Style::default().fg(Color::DarkGray)),
        Span::styled(search_info, Style::default().fg(Color::Yellow)),
        Span::styled(thread_info.to_string(), Style::default().fg(Color::Blue)),
    ];

    // Typing indicator in status bar
    if let Some(typing) = state.typing_display() {
        spans.push(Span::styled(
            format!(" | {}", typing),
            Style::default().fg(Color::DarkGray),
        ));
    }

    if let Some(err) = &state.last_error {
        spans.push(Span::styled(
            format!(" | {}", err),
            Style::default().fg(Color::Red),
        ));
    }

    // FPS counter (right-aligned)
    if state.show_fps {
        let ft = state.last_frame_time;
        let ft_ms = ft.as_secs_f64() * 1000.0;
        let fps_text = format!(" {:.1}ms  #{} ", ft_ms, state.frame_count);

        // Calculate padding to right-align
        let left_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let pad = (area.width as usize).saturating_sub(left_width + fps_text.len());
        if pad > 0 {
            spans.push(Span::styled(
                " ".repeat(pad),
                Style::default().bg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            fps_text,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
