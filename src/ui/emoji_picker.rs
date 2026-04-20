use crate::state::AppState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = centered_rect(50, 60, frame.area());
    frame.render_widget(Clear, area);

    let title = if matches!(state.emoji_picker_source, crate::state::EmojiPickerSource::Reaction)
        && !state.emoji_picker_message_reactions.is_empty()
    {
        " React — ↑↓ existing · type to search "
    } else {
        " Emoji — type to search "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Split inner into search line + results list
    let chunks = Layout::vertical([
        Constraint::Length(1), // search input
        Constraint::Length(1), // separator
        Constraint::Min(0),    // results
    ])
    .split(inner);

    // Search input
    let search_line = Line::from(vec![
        Span::styled(" /", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(
            state.emoji_picker_query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            if state.emoji_picker_results.is_empty() && !state.emoji_picker_query.is_empty() {
                " (no matches)".to_string()
            } else if !state.emoji_picker_results.is_empty() {
                format!(" [{}/{}]", state.emoji_picker_selected + 1, state.emoji_picker_results.len())
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
        .emoji_picker_results
        .iter()
        .enumerate()
        .map(|(i, (name, display, is_custom))| {
            let selected = i == state.emoji_picker_selected;
            let style = if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let emoji_display = if *is_custom {
                if state.has_emoji_image(name) {
                    "  ".to_string()
                } else {
                    display.clone()
                }
            } else {
                display.clone()
            };

            let mut spans = vec![
                Span::styled(format!(" {} ", emoji_display), style),
                Span::styled(format!(":{}: ", name), if selected { style } else { Style::default().fg(Color::DarkGray) }),
            ];

            if *is_custom && !selected {
                spans.push(Span::styled("[custom]", Style::default().fg(Color::Blue)));
            }

            if let Some((_, user_reacted)) = state.emoji_picker_message_reactions
                .iter()
                .find(|(n, _)| n == name)
            {
                if *user_reacted {
                    spans.push(Span::styled(" ✓ added", if selected {
                        style
                    } else {
                        Style::default().fg(Color::Green)
                    }));
                } else {
                    spans.push(Span::styled(" on message", if selected {
                        style
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }));
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let item_count = items.len();
    let mut list_state = ListState::default();
    list_state.select(Some(state.emoji_picker_selected));

    let list = List::new(items);
    frame.render_stateful_widget(list, results_area, &mut list_state);

    // Use ratatui's actual scroll offset for correct image placement
    let scroll_offset = list_state.offset();

    // Place inline emoji images for visible custom emoji
    let visible_names: Vec<(usize, String)> = state
        .emoji_picker_results
        .iter()
        .enumerate()
        .filter(|(i, (_, _, is_custom))| {
            *is_custom && *i >= scroll_offset && *i < scroll_offset + max_visible
        })
        .map(|(i, (name, _, _))| (i, name.clone()))
        .collect();

    for (i, name) in visible_names {
        let row_in_list = (i - scroll_offset) as u16;
        state.place_inline_emoji(&name, results_area.y + row_in_list, results_area.x + 1);
    }

    // Scrollbar for results
    if item_count > max_visible {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(Color::Magenta))
            .track_style(Style::default().fg(Color::DarkGray));
        let mut scrollbar_state = ScrollbarState::new(item_count).position(state.emoji_picker_selected);
        frame.render_stateful_widget(scrollbar, results_area, &mut scrollbar_state);
    }

    // Set cursor position for search input
    let cursor_x = chunks[0].x + 2 + state.emoji_picker_query.len() as u16;
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
