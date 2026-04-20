use crate::state::{AppState, ChannelListEntry, Focus, InputMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState};

const DM_TRUNCATE_LIMIT: usize = 10;

pub fn render(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let is_focused = state.focus == Focus::ChannelList;
    let is_searching = state.input_mode == InputMode::Search;
    let max_name_width = area.width.saturating_sub(5) as usize;

    let mut items: Vec<ListItem> = Vec::new();
    let mut list_entries: Vec<ChannelListEntry> = Vec::new();
    let mut custom_emoji_headers: Vec<(usize, String)> = Vec::new(); // (visual_idx, emoji_name)

    if is_searching {
        // Flat filtered list during search
        let visible_indices = state.filtered_channel_indices();
        for &ch_idx in &visible_indices {
            let vis_idx = items.len();
            let item = render_channel_item(state, ch_idx, vis_idx, max_name_width, is_focused, is_searching);
            items.push(item);
            list_entries.push(ChannelListEntry::Channel(ch_idx));
        }
    } else {
        // Section-based layout
        let sections = state.channels_by_section();

        for (sec_idx, (section_name, section_id, ch_indices, section_emoji)) in sections.iter().enumerate() {
            // Spacer between sections (except first)
            if sec_idx > 0 {
                items.push(ListItem::new(Line::from("")));
                list_entries.push(ChannelListEntry::Spacer);
            }

            let is_dm_section = section_id.is_none()
                && ch_indices
                    .first()
                    .map(|&i| {
                        let ch = &state.channels[i];
                        ch.is_im || ch.is_mpim
                    })
                    .unwrap_or(false);

            let is_collapsed = section_id
                .as_ref()
                .map(|id| state.collapsed_sections.contains(id))
                .unwrap_or(false);

            // Section header
            let collapse_indicator = if section_id.is_some() {
                if is_collapsed { "\u{25B8} " } else { "\u{25BE} " } // ▸ / ▾
            } else {
                ""
            };
            let header_visual_idx = items.len();
            let header_selected = header_visual_idx == state.selected_visual_idx && is_focused;
            let header_style = if header_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" {}{}", collapse_indicator, section_name),
                header_style,
            ))));
            if let Some(emoji_name) = section_emoji {
                if crate::ui::emoji::emoji_for_runtime(emoji_name, &state.standard_emoji).is_none() {
                    custom_emoji_headers.push((header_visual_idx, emoji_name.clone()));
                }
            }
            list_entries.push(if let Some(id) = section_id {
                ChannelListEntry::SectionHeader(id.clone())
            } else if is_dm_section {
                ChannelListEntry::SectionHeader("__dm__".to_string())
            } else {
                ChannelListEntry::SectionHeader("__channels__".to_string())
            });

            // Skip channels if collapsed
            if is_collapsed {
                continue;
            }

            // DM truncation
            let (visible_channels, hidden_count) = if is_dm_section && !state.dm_list_expanded {
                let limit = DM_TRUNCATE_LIMIT.min(ch_indices.len());
                (&ch_indices[..limit], ch_indices.len().saturating_sub(limit))
            } else {
                (&ch_indices[..], 0)
            };

            for &ch_idx in visible_channels {
                let vis_idx = items.len();
                let item = render_channel_item(state, ch_idx, vis_idx, max_name_width, is_focused, is_searching);
                items.push(item);
                list_entries.push(ChannelListEntry::Channel(ch_idx));
            }

            if hidden_count > 0 {
                let more_visual_idx = items.len();
                let more_selected = more_visual_idx == state.selected_visual_idx && is_focused;
                let more_style = if more_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {} more...", hidden_count),
                    more_style,
                ))));
                list_entries.push(ChannelListEntry::DmMore);
            }
        }
    }

    // Store the visual map for event handler navigation
    state.channel_list_items = list_entries;

    let border_style = if is_focused || is_searching {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if is_searching {
        format!(" /{} ", state.channel_filter)
    } else if state.team_name.is_empty() {
        " Channels ".to_string()
    } else {
        format!(" {} ", state.team_name)
    };

    let items_len = items.len();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    // Clamp visual index to valid range after list rebuild
    let visual_idx = state.selected_visual_idx.min(items_len.saturating_sub(1));

    // Adjust scroll offset so viewport only moves when selection enters top/bottom quarter
    let inner_height = area.height.saturating_sub(2) as usize;
    if inner_height > 0 && items_len > 0 {
        let quarter = inner_height / 4;
        let top_edge = state.channel_list_offset + quarter;
        let bottom_edge = state.channel_list_offset + inner_height.saturating_sub(quarter.max(1));
        if visual_idx < top_edge {
            state.channel_list_offset = visual_idx.saturating_sub(quarter);
        } else if visual_idx >= bottom_edge {
            state.channel_list_offset = (visual_idx + quarter.max(1))
                .saturating_sub(inner_height)
                .min(items_len.saturating_sub(inner_height));
        }
        state.channel_list_offset = state
            .channel_list_offset
            .min(items_len.saturating_sub(inner_height));
    }

    let mut list_state = ListState::default();
    list_state.select(None);
    *list_state.offset_mut() = state.channel_list_offset;

    frame.render_stateful_widget(list, area, &mut list_state);

    // Persist ratatui's computed offset back (it may clamp)
    state.channel_list_offset = list_state.offset();

    // Scrollbar
    if items_len > 0 {
        let thumb_style = if is_focused || is_searching {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(thumb_style)
            .track_style(Style::default().fg(Color::DarkGray));
        let mut scrollbar_state = ScrollbarState::new(items_len).position(visual_idx);
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }

    // Place custom emoji images in section headers
    let inner_y = area.y + 1; // account for border
    let inner_x = area.x + 1;
    let inner_height = area.height.saturating_sub(2) as usize;
    for (vis_idx, emoji_name) in &custom_emoji_headers {
        if *vis_idx < state.channel_list_offset {
            continue;
        }
        let row_in_view = vis_idx - state.channel_list_offset;
        if row_in_view >= inner_height {
            continue;
        }
        let screen_row = inner_y + row_in_view as u16;
        // Emoji goes after the collapse indicator: " ▾ " = 4 chars, or " " = 1 char for no-indicator
        let emoji_col = inner_x + 3;
        state.place_inline_emoji(emoji_name, screen_row, emoji_col);
    }
}

fn render_channel_item<'a>(
    state: &AppState,
    ch_idx: usize,
    visual_idx: usize,
    max_name_width: usize,
    is_focused: bool,
    is_searching: bool,
) -> ListItem<'a> {
    let ch = &state.channels[ch_idx];

    let (prefix, name, name_color) = if ch.is_im {
        let (name, color) = ch
            .user
            .as_ref()
            .map(|uid| (state.user_display_name(uid).to_string(), Some(state.user_color(uid))))
            .unwrap_or_else(|| (ch.display_name().to_string(), None));
        ("  ", name, color)
    } else if ch.is_mpim {
        let name = state.mpim_display_name(ch);
        ("  ", name, None)
    } else if ch.is_private {
        ("  \u{1F512} ", ch.display_name().to_string(), None)
    } else {
        ("  # ", ch.display_name().to_string(), None)
    };

    let name = truncate_str(&name, max_name_width.saturating_sub(prefix.len()));

    let is_selected = visual_idx == state.selected_visual_idx;
    let has_unread = ch.unread_count_display > 0;

    let style = if is_selected {
        Style::default()
            .fg(Color::Black)
            .bg(if is_focused || is_searching {
                Color::Cyan
            } else {
                Color::DarkGray
            })
            .add_modifier(Modifier::BOLD)
    } else if has_unread {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else if let Some(color) = name_color {
        Style::default().fg(color)
    } else {
        Style::default().fg(Color::Gray)
    };

    let mut spans = vec![
        Span::styled(prefix.to_string(), style),
        Span::styled(name, style),
    ];

    if has_unread && !is_selected {
        spans.push(Span::styled(
            format!(" {}", ch.unread_count_display),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 1 {
        format!("{}~", &s[..max - 1])
    } else {
        s[..max].to_string()
    }
}
