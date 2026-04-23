use crate::state::AppState;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub const MENU_ITEMS: &[(&str, &str)] = &[
    ("Open thread", "Enter"),
    ("Reply in thread", "R"),
    ("React with emoji", "r"),
    ("Copy message", "y"),
];

pub fn render(frame: &mut Frame, state: &AppState, messages_area: Rect) {
    if state.selected_message().is_none() {
        return;
    }

    let area = overlay_rect(state, messages_area);

    frame.render_widget(Clear, area);

    let lines: Vec<Line<'static>> = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (label, key))| {
            let selected = i == state.context_menu_selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let key_style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(vec![
                Span::styled(format!(" {:20}", label), style),
                Span::styled(format!(" {:>5} ", key), key_style),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Actions ");

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().bg(Color::Black));

    frame.render_widget(paragraph, area);
}

pub fn overlay_rect(state: &AppState, messages_area: Rect) -> Rect {
    let item_count = MENU_ITEMS.len() as u16;
    let menu_height = item_count + 2;
    let menu_width = 30u16;

    let sel_line = state
        .message_line_starts
        .get(
            state
                .message_count()
                .saturating_sub(1)
                .saturating_sub(state.selected_message_idx),
        )
        .copied()
        .unwrap_or(0);
    let scroll_y = state
        .messages_render_info
        .as_ref()
        .map(|r| r.scroll_y)
        .unwrap_or(0);
    let screen_line = sel_line.saturating_sub(scroll_y) as u16;

    let inner_y = messages_area.y + 1;
    let inner_bottom = messages_area.y + messages_area.height;

    let menu_y = (inner_y + screen_line + 1).min(inner_bottom.saturating_sub(menu_height));
    let menu_x = messages_area
        .x
        .saturating_add(4)
        .min(messages_area.x + messages_area.width - menu_width);

    Rect::new(
        menu_x,
        menu_y,
        menu_width.min(messages_area.width),
        menu_height.min(messages_area.height),
    )
}
