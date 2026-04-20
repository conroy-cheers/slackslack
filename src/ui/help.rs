use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn render(frame: &mut Frame) {
    let area = centered_rect(60, 80, frame.area());

    frame.render_widget(Clear, area);

    let sections: Vec<(&str, Vec<(&str, &str)>)> = vec![
        (
            "Navigation",
            vec![
                ("j / k", "Select next / previous message or channel"),
                ("G / g", "Jump to newest / oldest"),
                ("Ctrl+d / Ctrl+u", "Half-page down / up"),
                ("Ctrl+f / Ctrl+b", "Full page down / up"),
                ("PgDn / PgUp", "Page down / up"),
                ("Home / End", "Jump to top / bottom"),
                ("Enter / l", "Open channel or thread"),
                ("h / Esc", "Go back (close panel)"),
                ("Tab", "Cycle focus between panes"),
                ("[ / ]", "Previous / next channel"),
                ("{ / }", "Previous / next unread channel"),
            ],
        ),
        (
            "Actions",
            vec![
                ("i / a", "Enter insert mode (type message)"),
                ("R", "Reply in thread"),
                ("r", "Pick emoji reaction"),
                ("y", "Copy selected message to clipboard"),
                ("Space", "Open context menu for message"),
                ("/ (channels)", "Search channels"),
                ("/ (messages)", "Search messages"),
                ("n / N", "Next / previous search result"),
                ("S", "Global Slack search"),
            ],
        ),
        (
            "Insert Mode",
            vec![
                ("Esc", "Back to normal mode"),
                ("Enter", "Send message"),
                ("Shift+Enter", "Insert newline"),
                ("Tab", "Toggle reply target (channel/thread)"),
                ("@", "Open @mention user picker"),
                ("Ctrl+w", "Delete word backward"),
                ("Ctrl+u", "Clear line before cursor"),
                ("Ctrl+k", "Delete to end of line"),
                ("Ctrl+y", "Yank (paste) last killed text"),
                ("Ctrl+a / Ctrl+e", "Jump to start / end (or emoji picker)"),
                ("Alt+f / Alt+b", "Word forward / backward"),
                ("Alt+d", "Delete word forward"),
                ("Ctrl+o", "Upload file"),
                ("Alt+x", "Edit in $EDITOR"),
                ("Up / Down", "Browse input history"),
            ],
        ),
        (
            "General",
            vec![
                ("?", "Toggle this help"),
                ("F", "Toggle frame time counter"),
                ("q", "Quit"),
                ("Ctrl+c", "Force quit"),
            ],
        ),
    ];

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, (title, binds)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!(" {}", title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        for (key, desc) in binds {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:20}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(Color::White)),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Keybindings — press any key to close ");

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().bg(Color::Black));

    frame.render_widget(paragraph, area);
}

pub fn overlay_rect(area: Rect) -> Rect {
    centered_rect(60, 80, area)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
