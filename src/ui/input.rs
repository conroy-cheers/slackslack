use crate::state::{AppState, Focus, InputMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let is_focused = state.focus == Focus::Input;

    let channel_name = state
        .active_channel()
        .map(|c| {
            if c.is_im {
                c.user
                    .as_ref()
                    .map(|uid| state.user_display_name(uid).to_string())
                    .unwrap_or_else(|| c.display_name().to_string())
            } else {
                format!("#{}", c.display_name())
            }
        })
        .unwrap_or_default();

    let (title, display_text, text_style, border_color) = match state.input_mode {
        InputMode::Insert => {
            if state.reply_to_thread {
                (
                    " Reply in thread ".to_string(),
                    state.input_text.as_str(),
                    Style::default().fg(Color::White),
                    Color::Blue,
                )
            } else {
                (
                    format!(" Message {} ", channel_name),
                    state.input_text.as_str(),
                    Style::default().fg(Color::White),
                    Color::Cyan,
                )
            }
        }
        InputMode::Reaction => (
            " React with emoji (type name, Enter to confirm) ".to_string(),
            state.reaction_input.as_str(),
            Style::default().fg(Color::Yellow),
            Color::Magenta,
        ),
        InputMode::EmojiPicker | InputMode::UserPicker | InputMode::MessageSearch | InputMode::Search => (
            " i to type | / search | ? help ".to_string(),
            "",
            Style::default().fg(Color::DarkGray),
            Color::DarkGray,
        ),
        InputMode::Normal => (
            " i to type | / search | ? help ".to_string(),
            state.input_text.as_str(),
            if is_focused {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            },
            if is_focused { Color::Cyan } else { Color::DarkGray },
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    // Show thread indicator in the title when thread is open but not replying to it
    let block = if state.thread_channel_id.is_some()
        && state.input_mode == InputMode::Insert
        && !state.reply_to_thread
    {
        block.title_bottom(" Tab → thread ")
    } else {
        block
    };

    let paragraph = Paragraph::new(display_text)
        .style(text_style)
        .block(block);

    frame.render_widget(paragraph, area);
}
