use crate::terminal_renderer::{TerminalGrid, TERM_COLS, TERM_ROWS};

const GREEN: [u8; 4] = [0, 204, 0, 255];
const BRIGHT: [u8; 4] = [0, 255, 0, 255];
const DIM: [u8; 4] = [0, 102, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const GRAY: [u8; 4] = [160, 160, 160, 255];
const YELLOW: [u8; 4] = [255, 212, 64, 255];
const DIM_GRAY: [u8; 4] = [96, 96, 96, 255];
const BG: [u8; 4] = [0, 0, 0, 255];
const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

pub struct EmojiEntry {
    pub name: String,
}

pub struct Gallery {
    entries: Vec<EmojiEntry>,
    selected: usize,
    search: String,
    preview_index: Option<usize>,
    preview_mix: f32,
    preview_target: f32,
    channel_switch: f32,
    channel_switch_dir: f32,
    preview_reset_nonce: u32,
}

pub enum KeyAction {
    Up,
    Down,
    Enter,
    Escape,
    Char(char),
    Backspace,
}

impl Gallery {
    pub fn with_entries<I, S>(entries: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            entries: entries
                .into_iter()
                .map(|name| EmojiEntry { name: name.into() })
                .collect(),
            selected: 0,
            search: String::new(),
            preview_index: None,
            preview_mix: 0.0,
            preview_target: 0.0,
            channel_switch: 0.0,
            channel_switch_dir: 0.0,
            preview_reset_nonce: 0,
        }
    }

    pub fn is_previewing(&self) -> bool {
        self.preview_index.is_some()
    }

    pub fn preview_mix(&self) -> f32 {
        self.preview_mix
    }

    pub fn preview_index(&self) -> Option<usize> {
        self.preview_index
    }

    pub fn channel_switch(&self) -> f32 {
        self.channel_switch
    }

    pub fn channel_switch_dir(&self) -> f32 {
        self.channel_switch_dir
    }

    pub fn preview_reset_nonce(&self) -> u32 {
        self.preview_reset_nonce
    }

    pub fn current_entry_name(&self) -> Option<&str> {
        let filtered = self.filtered_entries();
        if filtered.is_empty() {
            return None;
        }
        let current_filtered_index = if self.preview_target > 0.0 {
            self.preview_index()
                .and_then(|preview_index| {
                    filtered
                        .iter()
                        .position(|(real_index, _)| *real_index == preview_index)
                })
                .unwrap_or(self.selected.min(filtered.len().saturating_sub(1)))
        } else {
            self.selected.min(filtered.len().saturating_sub(1))
        };
        filtered
            .get(current_filtered_index)
            .map(|(_, entry)| entry.name.as_str())
    }

    pub fn set_entries<I, S>(&mut self, entries: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let current_name = self.current_entry_name().map(str::to_owned);
        self.entries = entries
            .into_iter()
            .map(|name| EmojiEntry { name: name.into() })
            .collect();

        let filtered = self.filtered_entries();
        if filtered.is_empty() {
            self.selected = 0;
            self.preview_index = None;
            self.preview_target = 0.0;
            self.preview_mix = 0.0;
            self.channel_switch = 0.0;
            self.channel_switch_dir = 0.0;
            return;
        }

        let next_index = current_name
            .as_deref()
            .and_then(|name| {
                filtered
                    .iter()
                    .position(|(_, entry)| entry.name.as_str() == name)
            })
            .unwrap_or_else(|| self.selected.min(filtered.len().saturating_sub(1)));
        let next_preview_index = if self.preview_index.is_some() {
            filtered.get(next_index).map(|(real_index, _)| *real_index)
        } else {
            None
        };
        self.selected = next_index;
        self.preview_index = next_preview_index;
    }

    pub fn tick(&mut self, dt_secs: f32) {
        let speed = 6.5;
        let delta = (dt_secs * speed).clamp(0.0, 1.0);
        if self.preview_mix < self.preview_target {
            self.preview_mix = (self.preview_mix + delta).min(self.preview_target);
        } else if self.preview_mix > self.preview_target {
            self.preview_mix = (self.preview_mix - delta).max(self.preview_target);
        }

        if self.preview_target <= 0.0 && self.preview_mix <= 0.0 {
            self.preview_index = None;
        }

        let switch_decay = (dt_secs * 8.5).clamp(0.0, 1.0);
        if self.channel_switch > 0.0 {
            self.channel_switch = (self.channel_switch - switch_decay).max(0.0);
        }
    }

    pub fn handle_key(&mut self, action: KeyAction) {
        if self.is_previewing() {
            match action {
                KeyAction::Up => self.move_preview_selection(-1),
                KeyAction::Down => self.move_preview_selection(1),
                KeyAction::Escape | KeyAction::Backspace => {
                    self.preview_target = 0.0;
                }
                _ => {}
            }
            return;
        }

        match action {
            KeyAction::Up => self.move_selection(-1),
            KeyAction::Down => self.move_selection(1),
            KeyAction::Enter => {
                let filtered = self.filtered_entries();
                if let Some(&(real_index, _)) = filtered.get(self.selected) {
                    self.preview_index = Some(real_index);
                    self.preview_target = 1.0;
                    self.preview_reset_nonce = self.preview_reset_nonce.wrapping_add(1);
                }
            }
            KeyAction::Char(c) => {
                self.search.push(c);
                self.selected = 0;
            }
            KeyAction::Backspace => {
                self.search.pop();
                self.selected = 0;
            }
            KeyAction::Escape => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.selected = 0;
                }
            }
        }
    }

    pub fn enter_preview_immediate(&mut self) {
        let filtered = self.filtered_entries();
        if let Some(&(real_index, _)) = filtered.get(self.selected) {
            self.preview_index = Some(real_index);
            self.preview_target = 1.0;
            self.preview_mix = 1.0;
            self.preview_reset_nonce = self.preview_reset_nonce.wrapping_add(1);
        }
    }

    pub fn new() -> Self {
        Self::with_entries(
            [
                "thumbsup",
                "heart",
                "fire",
                "rocket",
                "tada",
                "eyes",
                "wave",
                "100",
                "sparkles",
                "pray",
                "muscle",
                "sunglasses",
                "thinking_face",
                "laughing",
                "sob",
                "clap",
                "raised_hands",
                "ok_hand",
                "point_up",
                "star",
                "zap",
                "rainbow",
                "pizza",
                "coffee",
                "beer",
                "skull",
                "ghost",
                "robot_face",
                "alien",
                "unicorn",
                "penguin",
                "cat",
                "dog",
                "parrot",
                "crab",
            ]
            .into_iter()
            .map(str::to_owned),
        )
    }

    fn move_selection(&mut self, delta: isize) {
        let filtered = self.filtered_entries();
        if filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let current = self.selected as isize;
        let len = filtered.len() as isize;
        let next = ((current + delta) % len + len) % len;
        self.selected = next as usize;
    }

    fn move_preview_selection(&mut self, delta: isize) {
        let filtered = self.filtered_entries();
        if filtered.is_empty() {
            return;
        }
        let current = self
            .preview_index
            .and_then(|preview_index| filtered.iter().position(|(real_index, _)| *real_index == preview_index))
            .unwrap_or(self.selected.min(filtered.len().saturating_sub(1)));
        let max_index = filtered.len().saturating_sub(1) as isize;
        let next = (current as isize + delta).clamp(0, max_index) as usize;
        if next == current {
            return;
        }
        let next_real_index = filtered[next].0;
        self.selected = next;
        self.preview_index = Some(next_real_index);
        self.channel_switch = 1.0;
        self.channel_switch_dir = if delta < 0 { -1.0 } else { 1.0 };
        self.preview_reset_nonce = self.preview_reset_nonce.wrapping_add(1);
    }

    pub fn billboard_cell_rect(&self, area_width: u16, area_height: u16) -> Option<CellRect> {
        if !self.is_previewing() || area_width < 2 || area_height < 2 {
            return None;
        }
        Some(CellRect {
            x: 0,
            y: 0,
            width: area_width,
            height: area_height,
        })
    }

    fn filtered_entries(&self) -> Vec<(usize, &EmojiEntry)> {
        let search = self.search.to_ascii_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                search.is_empty() || entry.name.to_ascii_lowercase().contains(&search)
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub fn render_to_grid(grid: &mut TerminalGrid, gallery: &Gallery, time_secs: f64) {
    if show_preview_overlay(gallery) {
        grid.clear(TRANSPARENT);
        draw_preview_overlay(grid, gallery);
    } else {
        grid.clear(BG);
        draw_gallery(grid, gallery, time_secs);
    }
}

pub fn show_preview_overlay(gallery: &Gallery) -> bool {
    gallery.is_previewing() && gallery.preview_mix() >= 0.5
}

pub fn cursor_blink_on(time_secs: f64) -> bool {
    ((time_secs * 2.0) as u64) % 2 == 0
}

fn ascii_rule(width: u16) -> String {
    "-".repeat(width as usize)
}

fn put_segments(grid: &mut TerminalGrid, mut x: u16, y: u16, segments: &[(&str, [u8; 4])]) {
    for (text, color) in segments {
        grid.put_text(x, y, text, *color, BG);
        x = x.saturating_add(text.chars().count() as u16);
        if x >= TERM_COLS {
            break;
        }
    }
}

fn put_segments_bg(
    grid: &mut TerminalGrid,
    mut x: u16,
    y: u16,
    bg: [u8; 4],
    segments: &[(&str, [u8; 4])],
) {
    for (text, color) in segments {
        grid.put_text(x, y, text, *color, bg);
        x = x.saturating_add(text.chars().count() as u16);
        if x >= TERM_COLS {
            break;
        }
    }
}

fn draw_gallery(grid: &mut TerminalGrid, gallery: &Gallery, time_secs: f64) {
    draw_header(grid, time_secs);
    draw_emoji_list(grid, gallery);
    draw_footer(grid, gallery);
}

fn draw_header(grid: &mut TerminalGrid, time_secs: f64) {
    let cursor = if cursor_blink_on(time_secs) { "_" } else { " " };
    put_segments(
        grid,
        0,
        0,
        &[(" EMOJI BILLBOARD", BRIGHT), (cursor, GREEN)],
    );
}

fn draw_emoji_list(grid: &mut TerminalGrid, gallery: &Gallery) {
    let filtered = gallery.filtered_entries();
    let count = filtered.len();
    let title = format!(" EMOJI {:>3} ", count);
    let mut rule = ascii_rule(TERM_COLS);
    let title_len = title.len().min(rule.len());
    rule.replace_range(0..title_len, &title[..title_len]);
    grid.put_text(0, 1, &rule, DIM, BG);

    let list_top = 2u16;
    let list_height = TERM_ROWS.saturating_sub(4) as usize;
    if count == 0 {
        grid.put_centered(list_top + 1, "NO EMOJI LOADED", DIM_GRAY, BG);
        grid.put_centered(list_top + 3, "SIGN IN WITH SLACK", DIM, BG);
        let bottom_rule = ascii_rule(TERM_COLS);
        grid.put_text(0, TERM_ROWS - 2, &bottom_rule, DIM, BG);
        return;
    }
    let max_scroll = count.saturating_sub(list_height);
    let scroll = gallery
        .selected
        .saturating_sub(list_height / 2)
        .min(max_scroll);

    for row in 0..list_height {
        let idx = scroll + row;
        if idx >= count {
            break;
        }
        let y = list_top + row as u16;
        let (_, entry) = filtered[idx];
        let selected = idx == gallery.selected;
        draw_entry(grid, y, &entry.name, selected);
    }

    let bottom_rule = ascii_rule(TERM_COLS);
    grid.put_text(0, TERM_ROWS - 2, &bottom_rule, DIM, BG);
}

fn draw_entry(grid: &mut TerminalGrid, y: u16, name: &str, selected: bool) {
    let prefix = if selected { ">" } else { " " };
    let prefix_color = if selected { BRIGHT } else { DIM };
    let name_color = if selected { BRIGHT } else { GREEN };
    put_segments(
        grid,
        0,
        y,
        &[
            (prefix, prefix_color),
            (" :", DIM),
            (name, name_color),
            (":", DIM),
        ],
    );
}

fn draw_footer(grid: &mut TerminalGrid, gallery: &Gallery) {
    if !gallery.search.is_empty() {
        put_segments(
            grid,
            0,
            TERM_ROWS - 1,
            &[(" >", BRIGHT), (&gallery.search, GREEN), ("_", BRIGHT)],
        );
    } else {
        put_segments(
            grid,
            0,
            TERM_ROWS - 1,
            &[
                (" UP/DN", BRIGHT),
                (" MOVE  ", DIM),
                ("ENTER", BRIGHT),
                (" VIEW  ", DIM),
                ("TYPE", BRIGHT),
                (" SEARCH", DIM),
            ],
        );
    }
}

fn draw_preview_overlay(grid: &mut TerminalGrid, gallery: &Gallery) {
    let filtered = gallery.filtered_entries();
    if filtered.is_empty() {
        return;
    }
    let current_filtered_index = gallery
        .preview_index()
        .and_then(|preview_index| {
            filtered
                .iter()
                .position(|(real_index, _)| *real_index == preview_index)
        })
        .unwrap_or(gallery.selected.min(filtered.len().saturating_sub(1)));
    let current_name = filtered
        .get(current_filtered_index)
        .map(|(_, entry)| entry.name.as_str())
        .unwrap_or("?");
    let prev_name = current_filtered_index
        .checked_sub(1)
        .and_then(|index| filtered.get(index))
        .map(|(_, entry)| entry.name.as_str());
    let next_name = filtered
        .get(current_filtered_index + 1)
        .map(|(_, entry)| entry.name.as_str());

    let up_line = format!("UP  :{}:", prev_name.unwrap_or("----"));
    let dn_line = format!("DN  :{}:", next_name.unwrap_or("----"));
    let up_x = ((TERM_COLS as usize).saturating_sub(up_line.len())) / 2;
    let dn_x = ((TERM_COLS as usize).saturating_sub(dn_line.len())) / 2;
    put_segments_bg(
        grid,
        up_x as u16,
        1,
        TRANSPARENT,
        &[
            ("UP", if prev_name.is_some() { YELLOW } else { DIM_GRAY }),
            ("  :", DIM_GRAY),
            (prev_name.unwrap_or("----"), DIM_GRAY),
            (":", DIM_GRAY),
        ],
    );
    grid.put_centered(3, &format!(":{current_name}:"), WHITE, TRANSPARENT);
    put_segments_bg(
        grid,
        dn_x as u16,
        5,
        TRANSPARENT,
        &[
            ("DN", if next_name.is_some() { YELLOW } else { DIM_GRAY }),
            ("  :", DIM_GRAY),
            (next_name.unwrap_or("----"), DIM_GRAY),
            (":", DIM_GRAY),
        ],
    );

    let help = "PRESS ESC TO GO BACK";
    let start_x = ((TERM_COLS as usize).saturating_sub(help.len())) / 2;
    put_segments(
        grid,
        start_x as u16,
        TERM_ROWS - 2,
        &[("PRESS ", GRAY), ("ESC", WHITE), (" TO GO BACK", GRAY)],
    );
}
