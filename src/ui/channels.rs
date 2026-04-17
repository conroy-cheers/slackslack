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

    if is_searching {
        // Flat filtered list during search
        let visible_indices = state.filtered_channel_indices();
        for &ch_idx in &visible_indices {
            let item = render_channel_item(state, ch_idx, max_name_width, is_focused, is_searching);
            items.push(item);
            list_entries.push(ChannelListEntry::Channel(ch_idx));
        }
    } else {
        // Section-based layout
        let sections = state.channels_by_section();

        for (sec_idx, (section_name, section_id, ch_indices)) in sections.iter().enumerate() {
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
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" {}{}", collapse_indicator, section_name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))));
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
                let item = render_channel_item(state, ch_idx, max_name_width, is_focused, is_searching);
                items.push(item);
                list_entries.push(ChannelListEntry::Channel(ch_idx));
            }

            if hidden_count > 0 {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  {} more...", hidden_count),
                    Style::default().fg(Color::DarkGray),
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

    // Find the visual index for the selected channel
    let visual_idx = state
        .channel_list_items
        .iter()
        .position(|entry| matches!(entry, ChannelListEntry::Channel(idx) if *idx == state.selected_channel_idx))
        .unwrap_or(0);

    let mut list_state = ListState::default();
    list_state.select(Some(visual_idx));

    frame.render_stateful_widget(list, area, &mut list_state);

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
}

fn render_channel_item<'a>(
    state: &AppState,
    ch_idx: usize,
    max_name_width: usize,
    is_focused: bool,
    is_searching: bool,
) -> ListItem<'a> {
    let ch = &state.channels[ch_idx];

    let (prefix, name) = if ch.is_im {
        let name = ch
            .user
            .as_ref()
            .map(|uid| state.user_display_name(uid).to_string())
            .unwrap_or_else(|| ch.display_name().to_string());
        ("  ", name)
    } else if ch.is_mpim {
        let name = state.mpim_display_name(ch);
        ("  ", name)
    } else if ch.is_private {
        ("  \u{1F512} ", ch.display_name().to_string())
    } else {
        ("  # ", ch.display_name().to_string())
    };

    let name = truncate_str(&name, max_name_width.saturating_sub(prefix.len()));

    let is_selected = ch_idx == state.selected_channel_idx;
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
