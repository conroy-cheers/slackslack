use crate::state::{AppState, Focus, ImagePlacement, MessagesRenderInfo};
use crate::ui::{emoji, images};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use unicode_width::UnicodeWidthStr;

pub fn render(frame: &mut Frame, state: &mut AppState, area: Rect) {
    // Clear image placements from previous frame
    state.image_placements.clear();

    let is_focused = state.focus == Focus::Messages;

    let channel_name = state
        .active_channel()
        .map(|c| {
            if c.is_im {
                c.user
                    .as_ref()
                    .map(|uid| state.user_display_name(uid).to_string())
                    .unwrap_or_else(|| c.display_name().to_string())
            } else {
                c.display_name().to_string()
            }
        })
        .unwrap_or_else(|| "No channel".into());

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title_text = if state.active_channel().map(|c| c.is_im).unwrap_or(false) {
        format!(" {} ", channel_name)
    } else {
        format!(" #{} ", channel_name)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            title_text,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    let width = inner.width as usize;
    let height = inner.height as usize;

    if width == 0 || height == 0 {
        frame.render_widget(block, area);
        return;
    }

    // Build all lines as owned data (Line<'static>) so we don't borrow state
    let msg_count = state.message_count();
    let selected_idx = msg_count.saturating_sub(1).saturating_sub(state.selected_message_idx);
    let (lines, placements, msg_line_starts, emoji_needed, avatars, content_line_count) = build_lines(state, width, selected_idx);
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(height);

    let scroll_y = if msg_line_starts.is_empty() || total_lines <= height {
        0
    } else if let Some(override_scroll) = state.messages_scroll_override {
        override_scroll.min(max_scroll)
    } else {
        let selected_line = msg_line_starts
            .get(selected_idx)
            .copied()
            .unwrap_or(total_lines);
        let ideal = selected_line.saturating_sub(height / 3);
        ideal.min(max_scroll)
    };

    state.max_scroll_offset = max_scroll;
    state.message_line_starts = msg_line_starts;

    // Store render info for kitty image positioning
    state.image_placements = placements;
    state.emoji_load_queue.extend(emoji_needed);
    state.messages_render_info = Some(MessagesRenderInfo {
        inner_x: inner.x,
        inner_y: inner.y,
        inner_height: inner.height,
        scroll_y,
    });

    // Convert avatar virtual line placements to screen coordinates
    let visible_end = scroll_y + inner.height as usize;
    for (user_id, vline) in &avatars {
        if *vline < scroll_y || *vline >= visible_end {
            continue;
        }
        let screen_row = inner.y + (*vline - scroll_y) as u16;
        // col 1 = after the marker character
        let screen_col = inner.x + 1;
        if state.avatar_images.contains_key(user_id.as_str()) {
            state.inline_emoji_placements.push(crate::state::InlineEmojiPlacement {
                emoji_key: format!("avatar:{}", user_id),
                screen_row,
                screen_col,
                display_cols: 2,
                display_rows: 1,
            });
        } else {
            state.request_avatar(user_id);
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll_y as u16, 0));

    frame.render_widget(paragraph, area);

    // Scrollbar (based on message content, excluding ephemeral typing indicator)
    if content_line_count > height {
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
        let sb_max = content_line_count.saturating_sub(height);
        let mut scrollbar_state = ScrollbarState::new(content_line_count).position(scroll_y.min(sb_max));
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn build_lines(
    state: &AppState,
    width: usize,
    selected_idx: usize,
) -> (Vec<Line<'static>>, Vec<ImagePlacement>, Vec<usize>, Vec<String>, Vec<(String, usize)>, usize) {
    let messages = state.channel_messages();

    let mut placements = Vec::new();
    let mut msg_line_starts = Vec::new();
    let mut emoji_needed = Vec::new();
    let mut avatars: Vec<(String, usize)> = Vec::new();

    let is_loading_older = state
        .active_channel_id()
        .and_then(|id| state.channel_data.get(&id))
        .map(|cd| cd.loading_more_history)
        .unwrap_or(false);

    let mut result = match messages {
        Some(msgs) if !msgs.is_empty() => {
            let mut lines = Vec::new();
            if is_loading_older {
                lines.push(Line::from(Span::styled(
                    "  Loading older messages...".to_string(),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )));
                lines.push(Line::from(""));
            }
            let msg_count = msgs.len();
            for (i, msg) in msgs.iter().enumerate() {
                msg_line_starts.push(lines.len());
                if let Some(uid) = &msg.user {
                    avatars.push((uid.clone(), lines.len()));
                }
                let is_selected = i == selected_idx;
                let is_search_match = state.message_search_active
                    && state.message_search_results_set.contains(&i);
                render_message(
                    msg,
                    state,
                    width,
                    is_selected,
                    is_search_match,
                    &mut lines,
                    &mut placements,
                    &mut emoji_needed,
                );
                if i + 1 < msg_count {
                    lines.push(Line::from(""));
                }
            }
            lines
        }
        _ => {
            let hint = if state.active_channel_id().is_some() {
                "Loading messages..."
            } else {
                "Select a channel and press Enter"
            };
            vec![Line::from(Span::styled(
                format!("  {}", hint),
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };

    let content_line_count = result.len();

    // Typing indicator at the bottom
    if let Some(typing) = state.typing_display() {
        result.push(Line::from(""));
        result.push(Line::from(Span::styled(
            format!("  {}", typing),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    (result, placements, msg_line_starts, emoji_needed, avatars, content_line_count)
}

fn render_message(
    msg: &crate::slack::types::Message,
    state: &AppState,
    width: usize,
    is_selected: bool,
    is_search_match: bool,
    out: &mut Vec<Line<'static>>,
    placements: &mut Vec<ImagePlacement>,
    emoji_needed: &mut Vec<String>,
) {
    let username = msg
        .user
        .as_ref()
        .map(|uid| state.user_display_name(uid).to_string())
        .or_else(|| msg.username.clone())
        .unwrap_or_else(|| "unknown".into());

    let time = format_timestamp(&msg.ts);

    // Selection / search marker
    let marker = if is_selected {
        "▎"
    } else if is_search_match {
        "│"
    } else {
        " "
    };
    let marker_style = if is_selected {
        Style::default().fg(Color::Cyan)
    } else if is_search_match {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    // Header: "▎ [avatar] username  HH:MM"
    let name_color = msg.user.as_ref()
        .map(|uid| state.user_color(uid))
        .unwrap_or(Color::Green);
    let mut header_spans: Vec<Span<'static>> = vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled("  ".to_string(), Style::default()), // avatar placeholder
        Span::styled(
            username,
            Style::default()
                .fg(name_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(time, Style::default().fg(Color::DarkGray)),
    ];

    if let Some(count) = msg.reply_count.filter(|&c| c > 0) {
        let label = if count == 1 {
            "1 reply".to_string()
        } else {
            format!("{} replies", count)
        };
        header_spans.push(Span::styled(
            format!("  {}", label),
            Style::default().fg(Color::Blue),
        ));
    }

    if msg.edited.is_some() {
        header_spans.push(Span::styled(
            String::from(" (edited)"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    out.push(Line::from(header_spans));

    // Body text — render Slack markup with rich text formatting
    let body_lines = render_rich_text(&msg.text, state, width.saturating_sub(2));
    let prefix = if is_selected { "▎ " } else { "  " };
    let prefix_style = if is_selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let prefix_width = UnicodeWidthStr::width(prefix) as u16;

    for line_spans in body_lines {
        let line_idx = out.len();
        let mut final_spans = vec![Span::styled(prefix.to_string(), prefix_style)];
        let mut col = prefix_width;

        for span in line_spans {
            let processed = replace_custom_emoji_in_span(
                &span.content,
                span.style,
                line_idx,
                &mut col,
                state,
                placements,
                emoji_needed,
            );
            final_spans.extend(processed);
        }
        out.push(Line::from(final_spans));
    }

    // Reactions
    if !msg.reactions.is_empty() {
        if is_selected {
            for r in &msg.reactions {
                let mut spans: Vec<Span<'static>> = vec![Span::styled("▎ ".to_string(), prefix_style)];
                let reaction_line = out.len();
                let mut col: u16 = 2;
                render_reaction_emoji(&r.name, state, reaction_line, &mut col, &mut spans, placements, emoji_needed);
                let name_str = format!(" :{}:", r.name);
                spans.push(Span::styled(name_str, Style::default().fg(Color::DarkGray)));
                let is_self = r.users.contains(&state.self_user_id);
                let count_str = format!(" {}", r.count);
                spans.push(Span::styled(count_str, if is_self {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                }));
                if is_self {
                    spans.push(Span::styled(" ✓", Style::default().fg(Color::Green)));
                }
                let user_names: Vec<&str> = r.users.iter()
                    .map(|uid| state.user_display_name(uid))
                    .collect();
                let users_str = format!("  {}", user_names.join(", "));
                spans.push(Span::styled(users_str, Style::default().fg(Color::DarkGray)));
                out.push(Line::from(spans));
            }
        } else {
            let mut col: u16 = 2;
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                "  ".to_string(),
                prefix_style,
            )];
            let reaction_line = out.len();
            for (i, r) in msg.reactions.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::from(String::from(" ")));
                    col += 1;
                }
                render_reaction_emoji(&r.name, state, reaction_line, &mut col, &mut spans, placements, emoji_needed);
                let count_str = format!(" {}", r.count);
                col += count_str.len() as u16;
                spans.push(Span::styled(count_str, Style::default().fg(Color::DarkGray)));
            }
            out.push(Line::from(spans));
        }
    }

    // Image file attachments
    for file in &msg.files {
        if !file.is_image() {
            continue;
        }
        let url = match file.best_thumb_url() {
            Some(u) => u.to_string(),
            None => continue,
        };

        // Show filename label
        out.push(Line::from(vec![
            Span::styled(prefix.to_string(), prefix_style),
            Span::styled(
                format!("[image: {}]", file.name),
                Style::default().fg(Color::Blue),
            ),
        ]));

        if let Some(cached) = state.image_cache.get(&url) {
            let max_cols = (width as u16).saturating_sub(4);
            let (display_cols, display_rows) =
                images::compute_display_size(cached.width, cached.height, max_cols);

            // Record placement at current line position
            placements.push(ImagePlacement {
                url,
                line: out.len(),
                col: 2, // indented to match message body prefix
                display_cols,
                display_rows,
            });

            // Reserve placeholder lines for the image
            for _ in 0..display_rows {
                out.push(Line::from(""));
            }
        }
    }
}

/// Scan a span's text for `:custom_emoji:` patterns that have cached images.
/// Replaces them with 2-space placeholders and records image placements.
fn replace_custom_emoji_in_span(
    text: &str,
    style: Style,
    line_idx: usize,
    col: &mut u16,
    state: &AppState,
    placements: &mut Vec<ImagePlacement>,
    emoji_needed: &mut Vec<String>,
) -> Vec<Span<'static>> {
    if !text.contains(':') || state.custom_emoji.is_empty() {
        *col += UnicodeWidthStr::width(text) as u16;
        return vec![Span::styled(text.to_string(), style)];
    }

    let mut result = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b':' {
            let start = i;
            let mut j = i + 1;
            let limit = len.min(j + 64);
            let mut found_key = None;
            let mut is_known = false;

            while j < limit {
                let b = bytes[j];
                if b == b':' {
                    let name = &text[start + 1..j];
                    if !name.is_empty() {
                        if let Some(key) = state.resolve_emoji_key(name) {
                            if state.custom_emoji_images.contains_key(&key) {
                                found_key = Some(key);
                            } else {
                                is_known = true;
                            }
                        }
                    }
                    break;
                } else if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'+' {
                    j += 1;
                } else {
                    break;
                }
            }

            if let Some(key) = found_key {
                placements.push(ImagePlacement {
                    url: key,
                    line: line_idx,
                    col: *col,
                    display_cols: 2,
                    display_rows: 1,
                });
                result.push(Span::styled("  ".to_string(), Style::default()));
                *col += 2;
                i = j + 1;
            } else if is_known {
                let name = &text[start + 1..j];
                emoji_needed.push(name.to_string());
                let display = format!(":{}:", name);
                *col += UnicodeWidthStr::width(display.as_str()) as u16;
                result.push(Span::styled(display, Style::default().fg(Color::Yellow)));
                i = j + 1;
            } else {
                *col += 1;
                result.push(Span::styled(":".to_string(), style));
                i = start + 1;
            }
        } else {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b':' {
                i += 1;
            }
            let chunk = &text[start..i];
            *col += UnicodeWidthStr::width(chunk) as u16;
            result.push(Span::styled(chunk.to_string(), style));
        }
    }

    if result.is_empty() {
        result.push(Span::styled(String::new(), style));
    }

    result
}

/// Render Slack markup into styled spans, handling rich text formatting.
fn render_rich_text(text: &str, state: &AppState, usable_width: usize) -> Vec<Vec<Span<'static>>> {
    // First pass: resolve Slack markup (<@U>, <#C>, URLs, entities) and emoji
    let resolved = resolve_slack_markup(text, state);

    let mut output_lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut in_code_block = false;

    for line in resolved.split('\n') {
        // Handle code block fences
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            if !in_code_block {
                // Closing fence — skip the line
                continue;
            }
            // Opening fence — skip the line, set flag
            continue;
        }

        if in_code_block {
            // Render as code (no formatting, distinct color)
            let spans = vec![Span::styled(
                line.to_string(),
                Style::default().fg(Color::Yellow),
            )];
            if usable_width == 0 {
                output_lines.push(spans);
            } else {
                for wrapped in wrap_spans(&spans, usable_width) {
                    output_lines.push(wrapped);
                }
            }
            continue;
        }

        // Blockquote
        if line.starts_with('>') || line.starts_with("&gt;") {
            let content = line
                .strip_prefix("&gt;")
                .or_else(|| line.strip_prefix('>'))
                .unwrap_or(line)
                .trim_start();
            let spans = vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    content.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::ITALIC),
                ),
            ];
            if usable_width == 0 {
                output_lines.push(spans);
            } else {
                for wrapped in wrap_spans(&spans, usable_width) {
                    output_lines.push(wrapped);
                }
            }
            continue;
        }

        // Normal line: parse inline formatting
        let spans = parse_inline_formatting(line);
        if usable_width == 0 {
            output_lines.push(spans);
        } else {
            for wrapped in wrap_spans(&spans, usable_width) {
                output_lines.push(wrapped);
            }
        }
    }

    output_lines
}

/// Public wrapper for thread.rs to use.
pub fn resolve_slack_markup_pub(text: &str, state: &AppState) -> String {
    resolve_slack_markup(text, state)
}

pub fn render_reaction_emoji(
    name: &str,
    state: &AppState,
    line: usize,
    col: &mut u16,
    spans: &mut Vec<Span<'static>>,
    placements: &mut Vec<ImagePlacement>,
    emoji_needed: &mut Vec<String>,
) {
    if let Some(e) = emoji::emoji_for_runtime(name, &state.standard_emoji) {
        let display = e.to_string();
        *col += UnicodeWidthStr::width(display.as_str()) as u16;
        spans.push(Span::styled(display, Style::default().fg(Color::Yellow)));
    } else if state.has_emoji_image(name) {
        let key = state.resolve_emoji_key(name).unwrap();
        placements.push(ImagePlacement {
            url: key,
            line,
            col: *col,
            display_cols: 2,
            display_rows: 1,
        });
        spans.push(Span::styled("  ".to_string(), Style::default()));
        *col += 2;
    } else {
        if state.custom_emoji.contains_key(name) || state.resolve_emoji_key(name).is_some() {
            emoji_needed.push(name.to_string());
        }
        let display = format!(":{}:", name);
        *col += UnicodeWidthStr::width(display.as_str()) as u16;
        spans.push(Span::styled(display, Style::default().fg(Color::Yellow)));
    }
}

pub fn render_rich_text_pub(text: &str, state: &AppState, usable_width: usize) -> Vec<Vec<Span<'static>>> {
    render_rich_text(text, state, usable_width)
}

pub fn replace_custom_emoji_in_span_pub(
    text: &str,
    style: Style,
    line_idx: usize,
    col: &mut u16,
    state: &AppState,
    placements: &mut Vec<ImagePlacement>,
    emoji_needed: &mut Vec<String>,
) -> Vec<Span<'static>> {
    replace_custom_emoji_in_span(text, style, line_idx, col, state, placements, emoji_needed)
}

/// Resolve Slack special markup: <@U>, <#C>, <!cmd>, <url|label>, &entities;, :emoji:
fn resolve_slack_markup(text: &str, state: &AppState) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut inner = String::new();
            for c in chars.by_ref() {
                if c == '>' {
                    break;
                }
                inner.push(c);
            }
            if let Some(uid) = inner.strip_prefix("@") {
                let id = uid.split('|').next().unwrap_or(uid);
                let name = state.user_display_name(id);
                result.push('@');
                result.push_str(name);
            } else if inner.starts_with("#") {
                if let Some((_id, name)) = inner[1..].split_once('|') {
                    result.push('#');
                    result.push_str(name);
                } else {
                    result.push_str(&inner);
                }
            } else if inner.starts_with("!") {
                let cmd = inner[1..].split('|').next().unwrap_or(&inner[1..]);
                result.push('@');
                result.push_str(cmd);
            } else if let Some((_url, label)) = inner.split_once('|') {
                result.push_str(label);
            } else {
                result.push_str(&inner);
            }
        } else if ch == '&' {
            let mut entity = String::new();
            for c in chars.by_ref() {
                if c == ';' {
                    break;
                }
                entity.push(c);
            }
            match entity.as_str() {
                "amp" => result.push('&'),
                "lt" => result.push('<'),
                "gt" => result.push('>'),
                "nbsp" => result.push(' '),
                _ => {
                    result.push('&');
                    result.push_str(&entity);
                    result.push(';');
                }
            }
        } else {
            result.push(ch);
        }
    }

    // Replace emoji shortcodes
    emoji::replace_emoji_shortcodes_with_map(&result, &state.standard_emoji)
}

/// Parse inline formatting: *bold*, _italic_, ~strikethrough~, `code`
fn parse_inline_formatting(line: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Inline code: `...`
        if ch == '`' {
            if !current.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut current)));
            }
            i += 1;
            let mut code = String::new();
            while i < len && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1; // skip closing `
            }
            spans.push(Span::styled(code, Style::default().fg(Color::Yellow)));
            continue;
        }

        // Bold: *...*
        if ch == '*' && i + 1 < len && chars[i + 1] != ' ' {
            if let Some(end) = find_closing(&chars, i + 1, '*') {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let content: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                i = end + 1;
                continue;
            }
        }

        // Italic: _..._
        if ch == '_' && i + 1 < len && chars[i + 1] != ' ' {
            if let Some(end) = find_closing(&chars, i + 1, '_') {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let content: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }

        // Strikethrough: ~...~
        if ch == '~' && i + 1 < len && chars[i + 1] != ' ' {
            if let Some(end) = find_closing(&chars, i + 1, '~') {
                if !current.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current)));
                }
                let content: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    Style::default().fg(Color::DarkGray),
                ));
                i = end + 1;
                continue;
            }
        }

        current.push(ch);
        i += 1;
    }

    if !current.is_empty() {
        spans.push(Span::raw(current));
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }

    spans
}

/// Find the position of a closing delimiter character.
fn find_closing(chars: &[char], start: usize, delim: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == delim && (i == start || chars[i - 1] != ' ') {
            return Some(i);
        }
    }
    None
}

/// Wrap a line of styled spans to fit within max_width columns.
fn wrap_spans(spans: &[Span<'static>], max_width: usize) -> Vec<Vec<Span<'static>>> {
    if max_width == 0 {
        return vec![spans.to_vec()];
    }

    // Calculate total width
    let total: usize = spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    if total <= max_width {
        return vec![spans.to_vec()];
    }

    // Need to wrap — flatten into (char, style) pairs, then wrap
    let mut result: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;
    let mut current_width = 0;

    for span in spans {
        let style = span.style;

        for word in span.content.split_inclusive(|c: char| c.is_whitespace()) {
            let word_width = UnicodeWidthStr::width(word);

            if current_width + word_width > max_width && current_width > 0 {
                // Flush current span
                if !current_text.is_empty() {
                    current_spans.push(Span::styled(
                        std::mem::take(&mut current_text),
                        current_style.unwrap_or_default(),
                    ));
                }
                result.push(std::mem::take(&mut current_spans));
                current_width = 0;
                current_style = Some(style);
            }

            if current_style != Some(style) && !current_text.is_empty() {
                current_spans.push(Span::styled(
                    std::mem::take(&mut current_text),
                    current_style.unwrap_or_default(),
                ));
            }

            current_text.push_str(word);
            current_style = Some(style);
            current_width += word_width;
        }
    }

    // Flush remaining
    if !current_text.is_empty() {
        current_spans.push(Span::styled(current_text, current_style.unwrap_or_default()));
    }
    if !current_spans.is_empty() {
        result.push(current_spans);
    }

    if result.is_empty() {
        result.push(vec![Span::raw(String::new())]);
    }

    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::types::{Channel, Message, SlackFile};
    use crate::state::CachedImage;

    fn msg(text: &str, ts: &str) -> Message {
        Message {
            user: Some("U1".into()),
            text: text.into(),
            ts: ts.into(),
            thread_ts: None,
            reply_count: None,
            reactions: Vec::new(),
            edited: None,
            subtype: None,
            bot_id: None,
            username: None,
            files: Vec::new(),
        }
    }

    fn msg_with_image(text: &str, ts: &str, url: &str) -> Message {
        let mut m = msg(text, ts);
        m.files.push(SlackFile {
            id: "F1".into(),
            name: "photo.png".into(),
            mimetype: Some("image/png".into()),
            filetype: Some("png".into()),
            url_private: None,
            thumb_360: Some(url.into()),
            thumb_480: None,
            thumb_160: None,
            thumb_360_w: 360,
            thumb_360_h: 240,
        });
        m
    }

    fn state_with(messages: Vec<Message>, cached_urls: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.channels = vec![Channel {
            id: "C1".into(),
            name: Some("test".into()),
            is_channel: true,
            is_im: false,
            is_mpim: false,
            is_private: false,
            is_member: true,
            user: None,
            topic: None,
            purpose: None,
            last_read: None,
            unread_count: 0,
            unread_count_display: 0,
        }];
        state.selected_channel_idx = 0;
        let cd = state.channel_data.entry("C1".into()).or_insert_with(crate::state::ChannelData::new);
        cd.messages = messages.into();
        for url in cached_urls {
            state.image_cache.insert(
                url.to_string(),
                CachedImage {
                    png_data: vec![0; 8],
                    width: 360,
                    height: 240,
                },
            );
        }
        state
    }

    /// Image placement `line` values must point within the line buffer,
    /// and their placeholder rows must be accounted for.
    #[test]
    fn image_placements_within_total_lines() {
        let messages = vec![
            msg("hello", "1000.0"),
            msg_with_image("check this", "1001.0", "http://img1"),
            msg("nice", "1002.0"),
            msg_with_image("another", "1003.0", "http://img2"),
        ];
        let state = state_with(messages, &["http://img1", "http://img2"]);

        let (lines, placements, starts, _, _, _) = build_lines(&state, 80, 3);


        for p in &placements {
            assert!(
                p.line <= lines.len(),
                "placement line {} > total lines {}",
                p.line,
                lines.len()
            );
            let end = p.line + p.display_rows as usize;
            assert!(
                end <= lines.len(),
                "placeholder end {} > total lines {}",
                end,
                lines.len()
            );
        }

        // msg_line_starts must be monotonically increasing
        for w in starts.windows(2) {
            assert!(w[0] < w[1], "msg_line_starts not ascending: {:?}", starts);
        }

        // Must have one start per message
        assert_eq!(starts.len(), 4);
    }

    /// Sweep every selected_message_idx — invariants must hold regardless of selection.
    #[test]
    fn all_selections_produce_valid_placements() {
        let mut messages = Vec::new();
        for i in 0..20 {
            if i % 4 == 0 {
                messages.push(msg_with_image(
                    &format!("img msg {}", i),
                    &format!("{}.0", 1000 + i),
                    &format!("http://img{}", i),
                ));
            } else {
                messages.push(msg(
                    &format!("text msg {}", i),
                    &format!("{}.0", 1000 + i),
                ));
            }
        }

        let cached: Vec<String> = (0..20).step_by(4).map(|i| format!("http://img{}", i)).collect();
        let refs: Vec<&str> = cached.iter().map(|s| s.as_str()).collect();
        let state = state_with(messages, &refs);

        for sel_idx in 0..20 {
            let (lines, placements, starts, _, _, _) = build_lines(&state, 80, sel_idx);

            for p in &placements {
                assert!(
                    p.line + (p.display_rows as usize) <= lines.len(),
                    "sel={}: placement at line {} + rows {} > total {}",
                    sel_idx,
                    p.line,
                    p.display_rows,
                    lines.len()
                );
            }

            assert_eq!(starts.len(), 20, "sel={}: wrong start count", sel_idx);
        }
    }

    /// Empty channel produces no placements and a hint line.
    #[test]
    fn empty_channel_no_placements() {
        let state = state_with(vec![], &[]);
        let (lines, placements, starts, _, _, _) = build_lines(&state, 80, 0);

        assert!(placements.is_empty());
        assert!(starts.is_empty());
        assert!(!lines.is_empty(), "should have a hint line");
    }

    /// Messages with uncached images should not produce placements.
    #[test]
    fn uncached_images_no_placements() {
        let messages = vec![msg_with_image("hi", "1000.0", "http://not_cached")];
        let state = state_with(messages, &[]); // no cached URLs

        let (_, placements, _, _, _, _) = build_lines(&state, 80, 0);
        assert!(placements.is_empty());
    }

    /// Various widths should not panic or produce invalid placements.
    #[test]
    fn narrow_and_wide_widths() {
        let messages = vec![
            msg("a long message that might need wrapping in narrow views", "1000.0"),
            msg_with_image("image", "1001.0", "http://img"),
        ];
        let state = state_with(messages, &["http://img"]);

        for width in [1, 5, 20, 80, 200, 400] {
            let (lines, placements, starts, _, _, _) = build_lines(&state, width, 1);

            for p in &placements {
                assert!(
                    p.line + (p.display_rows as usize) <= lines.len(),
                    "width={}: placement overflow",
                    width
                );
            }
            assert_eq!(starts.len(), 2, "width={}: wrong start count", width);
        }
    }
}
