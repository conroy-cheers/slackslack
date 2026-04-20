use crate::state::{AppState, Focus, InputMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn render(frame: &mut Frame, state: &mut AppState, area: Rect) {
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
        InputMode::FilePath => {
            let status = state.upload_status.as_deref().unwrap_or("");
            let display = if status.is_empty() {
                state.file_path_input.as_str()
            } else {
                status
            };
            (
                " Upload file (enter path, Enter to send) ".to_string(),
                display,
                Style::default().fg(Color::Yellow),
                Color::Magenta,
            )
        }
        InputMode::EmojiPicker if state.emoji_picker_inline_colon_pos.is_some() => {
            if state.reply_to_thread {
                (
                    " Reply in thread ".to_string(),
                    state.input_text.as_str(),
                    Style::default().fg(Color::White),
                    Color::Magenta,
                )
            } else {
                (
                    format!(" Message {} ", channel_name),
                    state.input_text.as_str(),
                    Style::default().fg(Color::White),
                    Color::Magenta,
                )
            }
        }
        InputMode::EmojiPicker | InputMode::UserPicker | InputMode::MessageSearch | InputMode::Search | InputMode::GlobalSearch | InputMode::EmojiPreview => (
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

    // Show Tab toggle hint when thread is open and in insert mode
    let block = if state.thread_channel_id.is_some() && state.input_mode == InputMode::Insert {
        if state.reply_to_thread {
            block.title_bottom(" Tab → channel ")
        } else {
            block.title_bottom(" Tab → thread ")
        }
    } else {
        block
    };

    let inner_width = area.width.saturating_sub(2);
    let inner_height = area.height.saturating_sub(2) as u16;
    let (_, cursor_row) = cursor_display_position(display_text, state.input_cursor, inner_width);
    let scroll = if inner_height > 0 && cursor_row >= state.input_scroll + inner_height {
        cursor_row - inner_height + 1
    } else if cursor_row < state.input_scroll {
        cursor_row
    } else {
        state.input_scroll
    };
    state.input_scroll = scroll;

    let text = resolve_mention_text(display_text, text_style, &state.standard_emoji);
    let paragraph = Paragraph::new(text)
        .style(text_style)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

pub fn compute_input_height(text: &str, inner_width: u16) -> u16 {
    use unicode_width::UnicodeWidthChar;
    if inner_width == 0 {
        return 3;
    }
    let display = display_text_for_input(text);
    let mut lines: u16 = 1;
    let mut col: u16 = 0;
    for c in display.chars() {
        if c == '\n' {
            lines += 1;
            col = 0;
            continue;
        }
        let w = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
        if col + w > inner_width {
            lines += 1;
            col = 0;
        }
        col += w;
    }
    lines + 2 // +2 for borders
}

pub fn cursor_display_position(text: &str, char_cursor: usize, width: u16) -> (u16, u16) {
    use unicode_width::UnicodeWidthChar;
    let display = display_text_for_input(text);
    let mapped_cursor = raw_to_display_cursor(text, char_cursor);
    let width = width as u16;
    let mut col: u16 = 0;
    let mut row: u16 = 0;
    for (i, c) in display.chars().enumerate() {
        if i == mapped_cursor {
            return (col, row);
        }
        if c == '\n' {
            col = 0;
            row += 1;
            continue;
        }
        let w = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
        if width > 0 && col + w > width {
            col = 0;
            row += 1;
        }
        col += w;
    }
    (col, row)
}

fn display_text_for_input(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut result = String::with_capacity(raw.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '@' {
            if let Some(end_offset) = chars[i..].iter().position(|&c| c == '>') {
                let end = i + end_offset;
                let inner: String = chars[i + 2..end].iter().collect();
                if let Some((_uid, name)) = inner.split_once('|') {
                    result.push('@');
                    result.push_str(name);
                } else {
                    for c in &chars[i..=end] {
                        result.push(*c);
                    }
                }
                i = end + 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn raw_to_display_cursor(raw: &str, raw_cursor: usize) -> usize {
    let chars: Vec<char> = raw.chars().collect();
    let mut display_pos = 0;
    let mut i = 0;
    while i < chars.len() && i < raw_cursor {
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '@' {
            if let Some(end_offset) = chars[i..].iter().position(|&c| c == '>') {
                let end = i + end_offset;
                let inner: String = chars[i + 2..end].iter().collect();
                let display_len = if let Some((_uid, name)) = inner.split_once('|') {
                    1 + name.chars().count()
                } else {
                    end - i + 1
                };
                if raw_cursor <= end {
                    return display_pos + display_len;
                }
                display_pos += display_len;
                i = end + 1;
                continue;
            }
        }
        display_pos += 1;
        i += 1;
    }
    display_pos
}

fn resolve_mention_text(text: &str, base_style: Style, standard_emoji: &std::collections::HashMap<String, String>) -> Text<'static> {
    let lines: Vec<Line<'static>> = text
        .split('\n')
        .map(|line_str| resolve_formatted_line(line_str, base_style, standard_emoji))
        .collect();
    Text::from(lines)
}

fn resolve_formatted_line(text: &str, base_style: Style, standard_emoji: &std::collections::HashMap<String, String>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Mention: <@...> — display as @name (shorter than raw)
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '@' {
            let start = i;
            if let Some(end_offset) = chars[i..].iter().position(|&c| c == '>') {
                let end = i + end_offset;
                let inner: String = chars[i + 2..end].iter().collect();
                let display = if let Some((_uid, name)) = inner.split_once('|') {
                    format!("@{}", name)
                } else {
                    format!("<@{}>", inner)
                };
                let mention_style = base_style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
                spans.push(Span::styled(display, mention_style));
                i = end + 1;
                continue;
            }
            let _ = start;
        }

        // Backtick code: `...` — keep delimiters visible
        if chars[i] == '`' {
            if let Some(content) = extract_delimited(&chars, i, '`') {
                let code_style = base_style.fg(Color::Red);
                spans.push(Span::styled(format!("`{}`", content), code_style));
                i += content.chars().count() + 2;
                continue;
            }
        }

        // Emoji shortcode: :name: — style recognized emoji
        if chars[i] == ':' && i + 1 < chars.len() {
            if let Some((shortcode, _emoji_str)) = extract_emoji_shortcode(&chars, i, standard_emoji) {
                let delim_style = base_style.fg(Color::DarkGray);
                spans.push(Span::styled(":", delim_style));
                spans.push(Span::styled(shortcode.clone(), base_style.fg(Color::Yellow)));
                spans.push(Span::styled(":", delim_style));
                i += shortcode.chars().count() + 2;
                continue;
            }
        }

        // Bold: *...* — keep delimiters, apply style
        if chars[i] == '*' {
            if let Some(content) = extract_delimited(&chars, i, '*') {
                let bold_style = base_style.add_modifier(Modifier::BOLD);
                let delim_style = bold_style.fg(Color::DarkGray);
                spans.push(Span::styled("*", delim_style));
                spans.push(Span::styled(content.clone(), bold_style));
                spans.push(Span::styled("*", delim_style));
                i += content.chars().count() + 2;
                continue;
            }
        }

        // Italic: _..._ — keep delimiters, apply style
        if chars[i] == '_' {
            if let Some(content) = extract_delimited(&chars, i, '_') {
                let italic_style = base_style.add_modifier(Modifier::ITALIC);
                let delim_style = italic_style.fg(Color::DarkGray);
                spans.push(Span::styled("_", delim_style));
                spans.push(Span::styled(content.clone(), italic_style));
                spans.push(Span::styled("_", delim_style));
                i += content.chars().count() + 2;
                continue;
            }
        }

        // Strikethrough: ~...~ — keep delimiters, apply style
        if chars[i] == '~' {
            if let Some(content) = extract_delimited(&chars, i, '~') {
                let strike_style = base_style.add_modifier(Modifier::CROSSED_OUT);
                let delim_style = strike_style.fg(Color::DarkGray);
                spans.push(Span::styled("~", delim_style));
                spans.push(Span::styled(content.clone(), strike_style));
                spans.push(Span::styled("~", delim_style));
                i += content.chars().count() + 2;
                continue;
            }
        }

        // Plain text — accumulate until we hit a special char
        let start = i;
        while i < chars.len() && !matches!(chars[i], '<' | '`' | '*' | '_' | '~' | ':') {
            i += 1;
        }
        if i == start {
            spans.push(Span::styled(chars[i].to_string(), base_style));
            i += 1;
        } else {
            let plain: String = chars[start..i].iter().collect();
            spans.push(Span::styled(plain, base_style));
        }
    }

    if spans.is_empty() {
        return Line::from(Span::styled(String::new(), base_style));
    }
    Line::from(spans)
}

fn extract_emoji_shortcode(chars: &[char], start: usize, standard_emoji: &std::collections::HashMap<String, String>) -> Option<(String, String)> {
    if start + 2 >= chars.len() {
        return None;
    }
    for j in (start + 2)..chars.len().min(start + 66) {
        if chars[j] == ':' {
            let shortcode: String = chars[start + 1..j].iter().collect();
            if shortcode.is_empty() {
                return None;
            }
            if let Some(emoji) = crate::ui::emoji::emoji_for_runtime(&shortcode, standard_emoji) {
                return Some((shortcode, emoji.to_string()));
            }
            return None;
        }
        if !chars[j].is_ascii_alphanumeric() && chars[j] != '_' && chars[j] != '-' && chars[j] != '+' {
            return None;
        }
    }
    None
}

fn extract_delimited(chars: &[char], start: usize, delim: char) -> Option<String> {
    if start + 1 >= chars.len() {
        return None;
    }
    // Don't match if the next char is whitespace or the same delimiter
    if chars[start + 1] == delim || chars[start + 1].is_whitespace() {
        return None;
    }
    for j in (start + 2)..chars.len() {
        if chars[j] == delim && !chars[j - 1].is_whitespace() {
            let content: String = chars[start + 1..j].iter().collect();
            return Some(content);
        }
    }
    None
}
