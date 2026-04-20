use crate::state::AppState;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Search Slack (Enter to search, ↑↓ to navigate) ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);

    // Query input line
    let query_line = if state.global_search_loading {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Yellow)),
            Span::raw(&state.global_search_query),
            Span::styled("  searching...", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
        ])
    } else {
        let suffix = if !state.global_search_results.is_empty() {
            format!(
                "  [{}/{}]",
                state.global_search_selected + 1,
                state.global_search_total
            )
        } else if !state.global_search_query.is_empty() && state.global_search_total == 0 && !state.global_search_loading {
            String::new()
        } else {
            String::new()
        };
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Yellow)),
            Span::raw(&state.global_search_query),
            Span::styled(suffix, Style::default().fg(Color::DarkGray)),
        ])
    };
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // Cursor position
    let cursor_x = chunks[0].x + 2 + state.global_search_query.len() as u16;
    let cursor_y = chunks[0].y;
    frame.set_cursor_position((cursor_x, cursor_y));

    // Results list
    if state.global_search_results.is_empty() {
        if !state.global_search_query.is_empty() && !state.global_search_loading {
            let hint = if state.global_search_total == 0 && !state.global_search_results.is_empty() {
                "No results"
            } else {
                "Press Enter to search"
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    hint,
                    Style::default().fg(Color::DarkGray),
                ))),
                chunks[1],
            );
        }
        return;
    }

    let width = chunks[1].width as usize;
    let items: Vec<ListItem> = state
        .global_search_results
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let channel_name = m
                .channel
                .as_ref()
                .and_then(|c| c.name.as_deref())
                .unwrap_or("?");
            let username = m
                .username
                .as_deref()
                .or(m.user.as_deref())
                .unwrap_or("?");

            let prefix = format!("#{} @{}: ", channel_name, username);
            let text_budget = width.saturating_sub(prefix.len()).max(1);
            let formatted = super::messages::resolve_slack_markup_pub(&m.text, state);
            let text_preview: String = formatted
                .chars()
                .take(text_budget)
                .map(|c| if c == '\n' { ' ' } else { c })
                .collect();

            let style = if i == state.global_search_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(text_preview, style),
            ]))
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(state.global_search_selected));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);
}

pub fn overlay_rect(area: Rect) -> Rect {
    centered_rect(60, 70, area)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
