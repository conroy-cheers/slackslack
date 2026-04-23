use crate::state::AppState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState,
};

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = centered_rect(50, 60, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(" @mention — type to search ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(1), // search input
        Constraint::Length(1), // separator
        Constraint::Min(0),    // results
    ])
    .split(inner);

    // Search input
    let search_line = Line::from(vec![
        Span::styled(
            " @",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            state.user_picker_query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            if state.user_picker_results.is_empty() && !state.user_picker_query.is_empty() {
                " (no matches)".to_string()
            } else if !state.user_picker_results.is_empty() {
                format!(
                    " [{}/{}]",
                    state.user_picker_selected + 1,
                    state.user_picker_results.len()
                )
            } else {
                String::new()
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(search_line), chunks[0]);

    // Separator
    let sep = "─".repeat(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );

    // Results list
    let results_area = chunks[2];
    let max_visible = results_area.height as usize;

    let items: Vec<ListItem> = state
        .user_picker_results
        .iter()
        .enumerate()
        .map(|(i, (_user_id, display_name))| {
            let selected = i == state.user_picker_selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![Span::styled(
                format!(" {} ", display_name),
                style,
            )]))
        })
        .collect();

    let item_count = items.len();
    let mut list_state = ListState::default();
    list_state.select(Some(state.user_picker_selected));

    let list = List::new(items);
    frame.render_stateful_widget(list, results_area, &mut list_state);

    // Scrollbar
    if item_count > max_visible {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(Color::Blue))
            .track_style(Style::default().fg(Color::DarkGray));
        let mut scrollbar_state =
            ScrollbarState::new(item_count).position(state.user_picker_selected);
        frame.render_stateful_widget(scrollbar, results_area, &mut scrollbar_state);
    }

    // Set cursor position for search input
    let cursor_x = chunks[0].x + 2 + state.user_picker_query.len() as u16;
    let cursor_y = chunks[0].y;
    frame.set_cursor_position((cursor_x, cursor_y));
}

pub fn overlay_rect(area: Rect) -> Rect {
    centered_rect(50, 60, area)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
