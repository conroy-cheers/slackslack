mod channels;
pub mod emoji;
mod emoji_picker;
mod help;
pub mod images;
mod input;
pub mod messages;
mod status;
mod thread;
mod user_picker;

use crate::state::{AppState, InputMode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

pub fn render(frame: &mut Frame, state: &mut AppState) {
    state.inline_emoji_placements.clear();
    state.occlusion_rects.clear();
    let size = frame.area();

    // Outer layout: main area + status bar (full width at very bottom)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // main area
            Constraint::Length(1), // status bar
        ])
        .split(size);

    // Sidebar width: ~18% of terminal, clamped to [20, 50]
    let sidebar_width = ((size.width as usize) * 18 / 100).clamp(20, 50) as u16;

    // Main area: sidebar | content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width),
            Constraint::Min(0),
        ])
        .split(outer[0]);

    // Content area: optional search bar, messages (+thread), input
    let content_area = main_chunks[1];
    let has_search_bar =
        state.input_mode == InputMode::MessageSearch || state.message_search_active;

    let content_constraints: Vec<Constraint> = if has_search_bar {
        vec![
            Constraint::Length(1), // search bar
            Constraint::Min(0),    // messages (+thread)
            Constraint::Length(3), // input
        ]
    } else {
        vec![
            Constraint::Min(0),    // messages (+thread)
            Constraint::Length(3), // input
        ]
    };
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(content_constraints)
        .split(content_area);

    // Determine chunk indices depending on whether search bar is present
    let (search_bar_area, messages_area, input_area) = if has_search_bar {
        (Some(content_chunks[0]), content_chunks[1], content_chunks[2])
    } else {
        (None, content_chunks[0], content_chunks[1])
    };

    // Render search bar if present
    if let Some(area) = search_bar_area {
        render_search_bar(frame, state, area);
    }

    // If thread is open, split the messages area horizontally
    let thread_open = state.thread_channel_id.is_some();
    if thread_open {
        let msg_thread_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(55),
                Constraint::Percentage(45),
            ])
            .split(messages_area);

        state.messages_area = msg_thread_chunks[0];
        state.thread_area = Some(msg_thread_chunks[1]);

        channels::render(frame, state, main_chunks[0]);
        messages::render(frame, state, msg_thread_chunks[0]);
        thread::render(frame, state, msg_thread_chunks[1]);
    } else {
        state.messages_area = messages_area;
        state.thread_area = None;

        channels::render(frame, state, main_chunks[0]);
        messages::render(frame, state, messages_area);
    }

    state.channel_list_area = main_chunks[0];
    state.input_area = input_area;
    input::render(frame, state, input_area);
    status::render(frame, state, outer[1]);

    // Set cursor position based on current mode
    match state.input_mode {
        InputMode::Insert => {
            let x = input_area.x + 1 + state.input_cursor as u16;
            let y = input_area.y + 1;
            frame.set_cursor_position((x, y));
        }
        InputMode::Reaction => {
            let x = input_area.x + 1 + state.reaction_input.len() as u16;
            let y = input_area.y + 1;
            frame.set_cursor_position((x, y));
        }
        InputMode::Search => {
            let x = main_chunks[0].x + 2 + state.channel_filter.len() as u16;
            let y = main_chunks[0].y;
            frame.set_cursor_position((x, y));
        }
        InputMode::MessageSearch => {
            if let Some(area) = search_bar_area {
                // Cursor after the "/" prefix
                let x = area.x + 1 + state.message_search_query.len() as u16;
                let y = area.y;
                frame.set_cursor_position((x, y));
            }
        }
        InputMode::Normal | InputMode::EmojiPicker | InputMode::UserPicker => {}
    }

    // Help overlay (rendered last so it's on top)
    if state.show_help {
        help::render(frame);
        state.occlusion_rects.push(help::overlay_rect(frame.area()));
    }

    // Emoji picker overlay (on top of everything)
    if state.input_mode == InputMode::EmojiPicker {
        emoji_picker::render(frame, state);
        state.occlusion_rects.push(emoji_picker::overlay_rect(frame.area()));
    }

    // User picker overlay (on top of everything)
    if state.input_mode == InputMode::UserPicker {
        user_picker::render(frame, state);
        state.occlusion_rects.push(user_picker::overlay_rect(frame.area()));
    }

}

#[cfg(test)]
mod tests;

fn render_search_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let count = state.message_search_results.len();
    let suffix = if state.input_mode == InputMode::MessageSearch {
        if count > 0 {
            format!(
                " [{}/{}]",
                state.message_search_idx + 1,
                count
            )
        } else if !state.message_search_query.is_empty() {
            " [no matches]".to_string()
        } else {
            String::new()
        }
    } else if state.message_search_active && count > 0 {
        format!(
            " [{}/{}] (n/N to navigate, Esc to clear)",
            state.message_search_idx + 1,
            count
        )
    } else {
        String::new()
    };

    let line = vec![
        Span::styled("/", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(
            state.message_search_query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled(suffix, Style::default().fg(Color::DarkGray)),
    ];

    let paragraph = Paragraph::new(ratatui::text::Line::from(line))
        .style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
