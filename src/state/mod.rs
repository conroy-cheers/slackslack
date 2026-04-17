pub mod cache;

use crate::slack::types::{Channel, Message, User};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;
use tokio::sync::mpsc;

pub struct ChannelData {
    pub messages: VecDeque<Message>,
    pub has_more_history: bool,
    pub loading_more_history: bool,
    pub last_activity: String,
    /// thread_parent_ts -> replies (ordered oldest-first, includes parent as first element)
    pub threads: HashMap<String, Vec<Message>>,
}

impl ChannelData {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            has_more_history: false,
            loading_more_history: false,
            last_activity: String::new(),
            threads: HashMap::new(),
        }
    }

    pub fn touch_activity(&mut self, ts: &str) {
        if ts > self.last_activity.as_str() {
            self.last_activity = ts.to_string();
        }
    }

    pub fn push_message(&mut self, msg: Message) {
        self.touch_activity(&msg.ts);

        if let Some(ref parent_ts) = msg.thread_ts {
            if parent_ts != &msg.ts {
                let replies = self.threads.entry(parent_ts.clone()).or_default();
                if !replies.iter().any(|m| m.ts == msg.ts) {
                    replies.push(msg.clone());
                }
            }
        }

        if !self.messages.iter().any(|m| m.ts == msg.ts) {
            self.messages.push_back(msg);
        }

        while self.messages.len() > 500 {
            self.messages.pop_front();
        }
    }

    pub fn set_thread_replies(&mut self, parent_ts: &str, messages: Vec<Message>) {
        self.threads.insert(parent_ts.to_string(), messages);
    }

    pub fn thread_replies(&self, parent_ts: &str) -> Option<&[Message]> {
        self.threads.get(parent_ts).map(|v| v.as_slice())
    }

    pub fn add_reaction(&mut self, ts: &str, reaction: &str, user: &str) {
        Self::add_reaction_to_iter(self.messages.iter_mut(), ts, reaction, user);
        for replies in self.threads.values_mut() {
            Self::add_reaction_to_iter(replies.iter_mut(), ts, reaction, user);
        }
    }

    pub fn remove_reaction(&mut self, ts: &str, reaction: &str, user: &str) {
        Self::remove_reaction_from_iter(self.messages.iter_mut(), ts, reaction, user);
        for replies in self.threads.values_mut() {
            Self::remove_reaction_from_iter(replies.iter_mut(), ts, reaction, user);
        }
    }

    fn add_reaction_to_iter<'a>(iter: impl Iterator<Item = &'a mut Message>, ts: &str, reaction: &str, user: &str) {
        for msg in iter {
            if msg.ts == ts {
                if let Some(r) = msg.reactions.iter_mut().find(|r| r.name == reaction) {
                    if !r.users.contains(&user.to_string()) {
                        r.users.push(user.to_string());
                        r.count += 1;
                    }
                } else {
                    msg.reactions.push(crate::slack::types::Reaction {
                        name: reaction.to_string(),
                        count: 1,
                        users: vec![user.to_string()],
                    });
                }
                return;
            }
        }
    }

    fn remove_reaction_from_iter<'a>(iter: impl Iterator<Item = &'a mut Message>, ts: &str, reaction: &str, user: &str) {
        for msg in iter {
            if msg.ts == ts {
                if let Some(r) = msg.reactions.iter_mut().find(|r| r.name == reaction) {
                    r.users.retain(|u| u != user);
                    r.count = r.count.saturating_sub(1);
                    if r.count == 0 {
                        msg.reactions.retain(|r| r.name != reaction);
                    }
                }
                return;
            }
        }
    }
}

pub struct AppState {
    // Connection
    pub connected: bool,
    pub self_user_id: String,
    pub team_id: String,
    pub team_name: String,

    // Channels
    pub channels: Vec<Channel>,
    pub selected_channel_idx: usize,
    /// channel_id -> timestamp of most recent activity (for sorting)
    pub channel_activity: HashMap<String, String>,

    // Channel search
    pub channel_filter: String,
    pub channel_filter_active: bool,

    // Per-channel message data
    pub channel_data: HashMap<String, ChannelData>,

    // Message view
    pub scroll_offset: usize,
    pub max_scroll_offset: usize,
    pub selected_message_idx: usize, // 0 = newest, increases going older

    // Thread (view pointers — data lives in channel_data.threads)
    pub thread_channel_id: Option<String>,
    pub thread_parent_ts: Option<String>,
    pub thread_scroll_offset: usize,
    pub thread_max_scroll_offset: usize,

    // Typing indicators: channel_id -> [(user_id, when)]
    pub typing_users: HashMap<String, Vec<(String, Instant)>>,

    // Users
    pub user_cache: HashMap<String, User>,

    // Input
    pub input_text: String,
    pub input_cursor: usize,
    pub input_mode: InputMode,
    pub input_history: Vec<String>,
    pub input_history_idx: Option<usize>,
    pub input_stash: String,

    // Reply target (true = thread, false = channel)
    pub reply_to_thread: bool,

    // Reaction
    pub reaction_input: String,

    // Message search
    pub message_search_query: String,
    pub message_search_active: bool,
    pub message_search_results: Vec<usize>, // msg array indices that match
    pub message_search_results_set: HashSet<usize>, // O(1) lookup for render
    pub message_search_idx: usize,

    // Help overlay
    pub show_help: bool,

    // Clipboard (pending OSC 52 write)
    pub clipboard_pending: Option<String>,

    // Message line starts (set during render for scroll tracking)
    pub message_line_starts: Vec<usize>,

    // Images (kitty protocol)
    pub image_cache: HashMap<String, CachedImage>,
    pub pending_images: HashSet<String>,
    pub image_placements: Vec<ImagePlacement>,
    pub messages_render_info: Option<MessagesRenderInfo>,
    pub thread_placements: Vec<ImagePlacement>,
    pub thread_render_info: Option<ThreadRenderInfo>,
    pub inline_emoji_placements: Vec<InlineEmojiPlacement>,

    // Custom emoji
    pub custom_emoji: HashMap<String, String>, // name -> resolved URL or "alias:other"
    pub custom_emoji_images: HashMap<String, CachedImage>,
    pub pending_emoji_images: HashSet<String>,
    pub emoji_load_queue: Vec<String>,

    // Channel sections
    pub channel_sections: Vec<crate::slack::types::ChannelSection>,
    pub collapsed_sections: HashSet<String>,
    pub dm_list_expanded: bool,

    // Emoji picker
    pub emoji_picker_query: String,
    pub emoji_picker_selected: usize,
    pub emoji_picker_results: Vec<(String, String, bool)>, // (name, display, is_custom)
    pub emoji_picker_source: EmojiPickerSource,

    // Channel list visual map (populated during render, read by event handler)
    pub channel_list_items: Vec<ChannelListEntry>,

    // Channel sort
    pub channels_need_resort: bool,

    // Performance overlay
    pub show_fps: bool,
    pub last_frame_time: std::time::Duration,
    pub frame_count: u64,

    // UI
    pub focus: Focus,
    pub dirty: bool,
    pub last_error: Option<String>,

    // WebSocket writer
    pub ws_writer: Option<mpsc::UnboundedSender<String>>,
}

/// Cached image data ready for kitty protocol display.
pub struct CachedImage {
    pub png_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Records where an image should be rendered after terminal.draw().
pub struct ImagePlacement {
    pub url: String,
    pub line: usize,       // virtual line number within the message list
    pub col: u16,          // column offset from inner_x (0-indexed)
    pub display_cols: u16,
    pub display_rows: u16,
}

/// An emoji image placed at absolute screen coordinates.
/// Unlike ImagePlacement (which uses virtual line numbers within a scrollable
/// panel), this uses final screen row/col — usable by any widget.
pub struct InlineEmojiPlacement {
    pub emoji_key: String,
    pub screen_row: u16,
    pub screen_col: u16,
    pub display_cols: u16,
    pub display_rows: u16,
}

/// Rendering metadata for a panel, used for image positioning.
pub struct MessagesRenderInfo {
    pub inner_x: u16,
    pub inner_y: u16,
    pub inner_height: u16,
    pub scroll_y: usize,
}

pub struct ThreadRenderInfo {
    pub inner_x: u16,
    pub inner_y: u16,
    pub inner_height: u16,
    pub scroll_y: usize,
}

#[derive(Clone, Debug)]
pub enum ChannelListEntry {
    Channel(usize),           // index into state.channels
    SectionHeader(String),    // section_id
    DmMore,                   // "N more..." indicator
    Spacer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Insert,
    Search,        // channel search
    MessageSearch, // message content search
    Reaction,
    EmojiPicker,
}

#[derive(Clone, Copy, PartialEq)]
pub enum EmojiPickerSource {
    Reaction,
    Insert,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    ChannelList,
    Messages,
    Input,
    Thread,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connected: false,
            self_user_id: String::new(),
            team_id: String::new(),
            team_name: String::new(),
            channels: Vec::new(),
            selected_channel_idx: 0,
            channel_activity: HashMap::new(),
            channel_filter: String::new(),
            channel_filter_active: false,
            channel_data: HashMap::new(),
            scroll_offset: 0,
            max_scroll_offset: 0,
            selected_message_idx: 0,
            thread_channel_id: None,
            thread_parent_ts: None,
            thread_scroll_offset: 0,
            thread_max_scroll_offset: 0,
            typing_users: HashMap::new(),
            user_cache: HashMap::new(),
            input_text: String::new(),
            input_cursor: 0,
            input_mode: InputMode::Normal,
            input_history: Vec::new(),
            input_history_idx: None,
            input_stash: String::new(),
            reply_to_thread: false,
            reaction_input: String::new(),
            message_search_query: String::new(),
            message_search_active: false,
            message_search_results: Vec::new(),
            message_search_results_set: HashSet::new(),
            message_search_idx: 0,
            show_help: false,
            clipboard_pending: None,
            message_line_starts: Vec::new(),
            image_cache: HashMap::new(),
            pending_images: HashSet::new(),
            image_placements: Vec::new(),
            messages_render_info: None,
            thread_placements: Vec::new(),
            thread_render_info: None,
            inline_emoji_placements: Vec::new(),
            custom_emoji: HashMap::new(),
            custom_emoji_images: HashMap::new(),
            pending_emoji_images: HashSet::new(),
            emoji_load_queue: Vec::new(),
            channel_sections: Vec::new(),
            collapsed_sections: HashSet::new(),
            dm_list_expanded: false,
            emoji_picker_query: String::new(),
            emoji_picker_selected: 0,
            emoji_picker_results: Vec::new(),
            emoji_picker_source: EmojiPickerSource::Reaction,
            channel_list_items: Vec::new(),
            channels_need_resort: false,
            show_fps: false,
            last_frame_time: std::time::Duration::ZERO,
            frame_count: 0,
            focus: Focus::ChannelList,
            dirty: true,
            last_error: None,
            ws_writer: None,
        }
    }

    pub fn active_channel_id(&self) -> Option<String> {
        self.channels
            .get(self.selected_channel_idx)
            .map(|c| c.id.clone())
    }

    pub fn active_channel(&self) -> Option<&Channel> {
        self.channels.get(self.selected_channel_idx)
    }

    pub fn set_channels(&mut self, mut channels: Vec<Channel>) {
        self.sort_channels(&mut channels);
        self.channels = channels;
    }

    /// Sort channels: unread first, then by recent activity, then alphabetical.
    fn sort_channels(&self, channels: &mut Vec<Channel>) {
        let activity = &self.channel_activity;
        channels.sort_by(|a, b| {
            // Unread channels float to top
            let a_unread = a.unread_count_display > 0;
            let b_unread = b.unread_count_display > 0;
            if a_unread != b_unread {
                return b_unread.cmp(&a_unread);
            }
            // Then by recent activity (descending — most recent first)
            let a_ts = activity.get(&a.id).map(|s| s.as_str()).unwrap_or("");
            let b_ts = activity.get(&b.id).map(|s| s.as_str()).unwrap_or("");
            if !a_ts.is_empty() || !b_ts.is_empty() {
                let ord = b_ts.cmp(a_ts);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            // Fallback: channels before DMs, then alphabetical
            let a_dm = a.is_im || a.is_mpim;
            let b_dm = b.is_im || b.is_mpim;
            match (a_dm, b_dm) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a.display_name().cmp(b.display_name()),
            }
        });
    }

    /// Re-sort channels in place (call after activity changes).
    pub fn resort_channels(&mut self) {
        let mut channels = std::mem::take(&mut self.channels);
        self.sort_channels(&mut channels);
        // Preserve selection by id
        let selected_id = self.active_channel_id();
        self.channels = channels;
        if let Some(id) = selected_id {
            if let Some(pos) = self.channels.iter().position(|c| c.id == id) {
                self.selected_channel_idx = pos;
            }
        }
    }

    pub fn channel_data(&self, channel_id: &str) -> Option<&ChannelData> {
        self.channel_data.get(channel_id)
    }

    pub fn channel_data_mut(&mut self, channel_id: &str) -> &mut ChannelData {
        self.channel_data.entry(channel_id.to_string()).or_insert_with(ChannelData::new)
    }

    /// Record activity for a channel (updates both legacy map for sorting and ChannelData).
    pub fn touch_channel_activity(&mut self, channel_id: &str, ts: &str) {
        let entry = self.channel_activity.entry(channel_id.to_string()).or_default();
        if ts > entry.as_str() {
            *entry = ts.to_string();
        }
        self.channel_data_mut(channel_id).touch_activity(ts);
    }

    /// Get indices of channels matching the current search filter.
    pub fn filtered_channel_indices(&self) -> Vec<usize> {
        if !self.channel_filter_active || self.channel_filter.is_empty() {
            return (0..self.channels.len()).collect();
        }
        let query = self.channel_filter.to_lowercase();
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| {
                let name = if ch.is_im {
                    ch.user
                        .as_ref()
                        .map(|uid| self.user_display_name(uid).to_string())
                        .unwrap_or_else(|| ch.display_name().to_string())
                } else {
                    ch.display_name().to_string()
                };
                name.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn set_history(
        &mut self,
        channel_id: String,
        mut messages: Vec<Message>,
        has_more: bool,
    ) {
        if let Some(newest) = messages.first() {
            self.touch_channel_activity(&channel_id, &newest.ts);
        }
        messages.reverse();
        let cd = self.channel_data_mut(&channel_id);
        cd.messages = VecDeque::from(messages);
        cd.has_more_history = has_more;
        self.selected_message_idx = 0;
    }

    pub fn prepend_history(
        &mut self,
        channel_id: String,
        mut messages: Vec<Message>,
        has_more: bool,
    ) {
        messages.reverse();
        let cd = self.channel_data_mut(&channel_id);
        let old_len = cd.messages.len();
        for msg in messages.into_iter().rev() {
            cd.messages.push_front(msg);
        }
        let added = cd.messages.len() - old_len;
        self.selected_message_idx += added;
        let cd = self.channel_data_mut(&channel_id);
        cd.has_more_history = has_more;
        cd.loading_more_history = false;
        self.dirty = true;
    }

    pub fn push_message(&mut self, channel_id: String, msg: Message) {
        self.touch_channel_activity(&channel_id, &msg.ts);
        self.channel_data_mut(&channel_id).push_message(msg);
        self.dirty = true;
    }

    pub fn channel_next(&mut self) {
        let visible = self.visible_channel_indices();
        if visible.is_empty() {
            return;
        }
        if let Some(pos) = visible.iter().position(|&i| i == self.selected_channel_idx) {
            let next = (pos + 1) % visible.len();
            self.selected_channel_idx = visible[next];
        } else {
            self.selected_channel_idx = visible[0];
        }
    }

    pub fn channel_prev(&mut self) {
        let visible = self.visible_channel_indices();
        if visible.is_empty() {
            return;
        }
        if let Some(pos) = visible.iter().position(|&i| i == self.selected_channel_idx) {
            let prev = if pos == 0 { visible.len() - 1 } else { pos - 1 };
            self.selected_channel_idx = visible[prev];
        } else {
            self.selected_channel_idx = *visible.last().unwrap();
        }
    }

    pub fn visible_channel_indices(&self) -> Vec<usize> {
        if self.channel_list_items.is_empty() {
            return (0..self.channels.len()).collect();
        }
        self.channel_list_items
            .iter()
            .filter_map(|entry| match entry {
                ChannelListEntry::Channel(idx) => Some(*idx),
                _ => None,
            })
            .collect()
    }

    /// Navigate to next channel in filtered list.
    pub fn filtered_channel_next(&mut self) {
        let indices = self.filtered_channel_indices();
        if indices.is_empty() {
            return;
        }
        if let Some(pos) = indices.iter().position(|&i| i == self.selected_channel_idx) {
            let next = (pos + 1) % indices.len();
            self.selected_channel_idx = indices[next];
        } else {
            self.selected_channel_idx = indices[0];
        }
    }

    /// Navigate to previous channel in filtered list.
    pub fn filtered_channel_prev(&mut self) {
        let indices = self.filtered_channel_indices();
        if indices.is_empty() {
            return;
        }
        if let Some(pos) = indices.iter().position(|&i| i == self.selected_channel_idx) {
            let prev = if pos == 0 { indices.len() - 1 } else { pos - 1 };
            self.selected_channel_idx = indices[prev];
        } else {
            self.selected_channel_idx = indices[0];
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = (self.scroll_offset + 1).min(self.max_scroll_offset);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.selected_message_idx = 0;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = self.max_scroll_offset;
    }

    pub fn scroll_half_page_down(&mut self, page_height: usize) {
        let amount = page_height / 2;
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_half_page_up(&mut self, page_height: usize) {
        let amount = page_height / 2;
        self.scroll_offset = (self.scroll_offset + amount).min(self.max_scroll_offset);
    }

    pub fn channel_messages(&self) -> Option<&VecDeque<Message>> {
        self.active_channel_id()
            .and_then(|id| self.channel_data.get(&id))
            .map(|cd| &cd.messages)
    }

    pub fn message_count(&self) -> usize {
        self.channel_messages().map(|m| m.len()).unwrap_or(0)
    }

    pub fn message_select_newer(&mut self) {
        self.selected_message_idx = self.selected_message_idx.saturating_sub(1);
    }

    pub fn message_select_older(&mut self) {
        let max = self.message_count().saturating_sub(1);
        self.selected_message_idx = (self.selected_message_idx + 1).min(max);
    }

    pub fn selected_message(&self) -> Option<&Message> {
        let msgs = self.channel_messages()?;
        if msgs.is_empty() {
            return None;
        }
        let idx = msgs.len().saturating_sub(1).saturating_sub(self.selected_message_idx);
        msgs.get(idx)
    }

    pub fn user_display_name<'a>(&'a self, user_id: &'a str) -> &'a str {
        self.user_cache
            .get(user_id)
            .map(|u| u.display_name())
            .unwrap_or(user_id)
    }

    /// Get display name for an MPIM channel from its member user IDs.
    pub fn mpim_display_name(&self, channel: &Channel) -> String {
        // MPIMs have names like "mpdm-user1--user2--user3-1"
        // Try to extract user names from the channel name
        if let Some(name) = &channel.name {
            if name.starts_with("mpdm-") {
                let inner = name
                    .strip_prefix("mpdm-")
                    .unwrap_or(name)
                    .trim_end_matches(|c: char| c == '-' || c.is_ascii_digit());
                let parts: Vec<&str> = inner.split("--").collect();
                let names: Vec<String> = parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        // Try to find user by name match in cache
                        self.user_cache
                            .values()
                            .find(|u| u.name == **p)
                            .map(|u| u.display_name().to_string())
                            .unwrap_or_else(|| p.to_string())
                    })
                    .filter(|n| {
                        // Filter out self
                        let self_name = self
                            .user_cache
                            .get(&self.self_user_id)
                            .map(|u| u.display_name().to_string());
                        Some(n.clone()) != self_name
                    })
                    .collect();
                if !names.is_empty() {
                    return names.join(", ");
                }
            }
        }
        channel.display_name().to_string()
    }

    pub fn add_reaction(
        &mut self,
        channel_id: &str,
        ts: &str,
        reaction: &str,
        user: &str,
    ) {
        if let Some(cd) = self.channel_data.get_mut(channel_id) {
            cd.add_reaction(ts, reaction, user);
            self.dirty = true;
        }
    }

    pub fn remove_reaction(
        &mut self,
        channel_id: &str,
        ts: &str,
        reaction: &str,
        user: &str,
    ) {
        if let Some(cd) = self.channel_data.get_mut(channel_id) {
            cd.remove_reaction(ts, reaction, user);
            self.dirty = true;
        }
    }

    pub fn mark_channel_read(&mut self, channel_id: &str, ts: &str) {
        if let Some(ch) = self.channels.iter_mut().find(|c| c.id == channel_id) {
            ch.last_read = Some(ts.to_string());
            ch.unread_count_display = 0;
        }
        self.dirty = true;
    }

    /// Record a typing event for a user in a channel.
    pub fn record_typing(&mut self, channel_id: &str, user_id: &str) {
        let entry = self.typing_users.entry(channel_id.to_string()).or_default();
        // Update existing or add new
        if let Some(existing) = entry.iter_mut().find(|(uid, _)| uid == user_id) {
            existing.1 = Instant::now();
        } else {
            entry.push((user_id.to_string(), Instant::now()));
        }
        self.dirty = true;
    }

    /// Remove stale typing indicators (older than 5 seconds).
    pub fn expire_typing(&mut self) {
        let now = Instant::now();
        let mut changed = false;
        for entries in self.typing_users.values_mut() {
            let before = entries.len();
            entries.retain(|(_, when)| now.duration_since(*when).as_secs() < 5);
            if entries.len() != before {
                changed = true;
            }
        }
        // Remove empty channels
        self.typing_users.retain(|_, v| !v.is_empty());
        if changed {
            self.dirty = true;
        }
    }

    /// Get typing users for the active channel, formatted as a string.
    pub fn typing_display(&self) -> Option<String> {
        let channel_id = self.active_channel_id()?;
        let entries = self.typing_users.get(&channel_id)?;
        if entries.is_empty() {
            return None;
        }
        let names: Vec<String> = entries
            .iter()
            .filter(|(uid, _)| uid != &self.self_user_id)
            .map(|(uid, _)| self.user_display_name(uid).to_string())
            .collect();
        if names.is_empty() {
            return None;
        }
        Some(match names.len() {
            1 => format!("{} is typing...", names[0]),
            2 => format!("{} and {} are typing...", names[0], names[1]),
            _ => format!("{} and {} others are typing...", names[0], names.len() - 1),
        })
    }

    /// Save current input to history and clear.
    pub fn save_input_to_history(&mut self) {
        let text = self.input_text.trim().to_string();
        if !text.is_empty() {
            // Don't add duplicates of the last entry
            if self.input_history.last() != Some(&text) {
                self.input_history.push(text);
            }
        }
        self.input_text.clear();
        self.input_cursor = 0;
        self.input_history_idx = None;
        self.input_stash.clear();
    }

    /// Navigate to previous input history entry.
    pub fn input_history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        match self.input_history_idx {
            None => {
                // Stash current input
                self.input_stash = self.input_text.clone();
                let idx = self.input_history.len() - 1;
                self.input_history_idx = Some(idx);
                self.input_text = self.input_history[idx].clone();
                self.input_cursor = self.input_text.len();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.input_history_idx = Some(new_idx);
                self.input_text = self.input_history[new_idx].clone();
                self.input_cursor = self.input_text.len();
            }
            _ => {}
        }
    }

    /// Navigate to next input history entry.
    pub fn input_history_next(&mut self) {
        match self.input_history_idx {
            Some(idx) => {
                if idx + 1 < self.input_history.len() {
                    let new_idx = idx + 1;
                    self.input_history_idx = Some(new_idx);
                    self.input_text = self.input_history[new_idx].clone();
                    self.input_cursor = self.input_text.len();
                } else {
                    // Restore stashed input
                    self.input_history_idx = None;
                    self.input_text = self.input_stash.clone();
                    self.input_cursor = self.input_text.len();
                    self.input_stash.clear();
                }
            }
            None => {}
        }
    }

    /// Open a thread for the given message.
    pub fn open_thread(&mut self, channel_id: String, parent_ts: String) {
        self.thread_channel_id = Some(channel_id);
        self.thread_parent_ts = Some(parent_ts);
        self.thread_scroll_offset = 0;
        self.thread_max_scroll_offset = 0;
        self.focus = Focus::Thread;
    }

    /// Close the thread panel.
    pub fn close_thread(&mut self) {
        self.thread_channel_id = None;
        self.thread_parent_ts = None;
        self.focus = Focus::Messages;
    }

    pub fn set_thread_messages(&mut self, channel_id: &str, parent_ts: &str, messages: Vec<Message>) {
        self.channel_data_mut(channel_id).set_thread_replies(parent_ts, messages);
        self.thread_scroll_offset = 0;
        self.dirty = true;
    }

    pub fn thread_messages(&self) -> Option<&[Message]> {
        let channel_id = self.thread_channel_id.as_deref()?;
        let parent_ts = self.thread_parent_ts.as_deref()?;
        self.channel_data.get(channel_id)?.thread_replies(parent_ts)
    }

    pub fn thread_message_count(&self) -> usize {
        self.thread_messages().map(|m| m.len()).unwrap_or(0)
    }

    pub fn perform_message_search(&mut self) {
        self.message_search_results.clear();
        self.message_search_results_set.clear();
        self.message_search_idx = 0;

        if self.message_search_query.is_empty() {
            return;
        }

        let query = self.message_search_query.to_lowercase();
        let channel_id = match self.active_channel_id() {
            Some(id) => id,
            None => return,
        };
        if let Some(cd) = self.channel_data.get(&channel_id) {
            for (i, msg) in cd.messages.iter().enumerate() {
                if msg.text.to_lowercase().contains(&query) {
                    self.message_search_results.push(i);
                    self.message_search_results_set.insert(i);
                }
            }
        }
    }

    /// Jump to the next search result.
    pub fn message_search_next(&mut self) {
        if self.message_search_results.is_empty() {
            return;
        }
        self.message_search_idx = (self.message_search_idx + 1) % self.message_search_results.len();
        self.jump_to_search_result();
    }

    /// Jump to the previous search result.
    pub fn message_search_prev(&mut self) {
        if self.message_search_results.is_empty() {
            return;
        }
        if self.message_search_idx == 0 {
            self.message_search_idx = self.message_search_results.len() - 1;
        } else {
            self.message_search_idx -= 1;
        }
        self.jump_to_search_result();
    }

    pub fn jump_to_search_result(&mut self) {
        if let Some(&msg_idx) = self.message_search_results.get(self.message_search_idx) {
            let count = self.message_count();
            self.selected_message_idx = count.saturating_sub(1).saturating_sub(msg_idx);
        }
    }

    pub fn clear_message_search(&mut self) {
        self.message_search_query.clear();
        self.message_search_results.clear();
        self.message_search_results_set.clear();
        self.message_search_idx = 0;
        self.message_search_active = false;
    }

    /// Navigate to the next unread channel.
    pub fn next_unread_channel(&mut self) -> bool {
        let len = self.channels.len();
        if len == 0 {
            return false;
        }
        for offset in 1..=len {
            let idx = (self.selected_channel_idx + offset) % len;
            if self.channels[idx].unread_count_display > 0 {
                self.selected_channel_idx = idx;
                return true;
            }
        }
        false
    }

    /// Navigate to the previous unread channel.
    pub fn prev_unread_channel(&mut self) -> bool {
        let len = self.channels.len();
        if len == 0 {
            return false;
        }
        for offset in 1..=len {
            let idx = (self.selected_channel_idx + len - offset) % len;
            if self.channels[idx].unread_count_display > 0 {
                self.selected_channel_idx = idx;
                return true;
            }
        }
        false
    }

    /// Open the emoji picker from the given source.
    pub fn open_emoji_picker(&mut self, source: EmojiPickerSource) {
        self.input_mode = InputMode::EmojiPicker;
        self.emoji_picker_source = source;
        self.emoji_picker_query.clear();
        self.emoji_picker_selected = 0;
        self.filter_emoji_picker();
        self.dirty = true;
    }

    /// Filter emoji picker results based on current query.
    pub fn filter_emoji_picker(&mut self) {
        let query = self.emoji_picker_query.to_lowercase();
        let mut results: Vec<(String, String, bool)> = Vec::new();

        // Standard emoji
        for &(name, display) in crate::ui::emoji::all_standard_emoji() {
            if query.is_empty() || name.contains(&query) {
                results.push((name.to_string(), display.to_string(), false));
            }
        }

        // Custom emoji
        for (name, _url) in &self.custom_emoji {
            if query.is_empty() || name.contains(&query) {
                results.push((name.clone(), format!(":{}:", name), true));
            }
        }

        // Sort: exact prefix matches first, then alphabetical
        if !query.is_empty() {
            results.sort_by(|a, b| {
                let a_prefix = a.0.starts_with(&query);
                let b_prefix = b.0.starts_with(&query);
                b_prefix.cmp(&a_prefix).then(a.0.cmp(&b.0))
            });
        }

        self.emoji_picker_results = results;
        self.emoji_picker_selected = 0;
    }

    /// Resolve a custom emoji name to its URL, following alias chains (max 5 hops).
    pub fn resolve_custom_emoji(&self, name: &str) -> Option<&str> {
        let mut current = name;
        for _ in 0..5 {
            match self.custom_emoji.get(current) {
                Some(val) if val.starts_with("alias:") => {
                    current = &val["alias:".len()..];
                }
                Some(url) => return Some(url.as_str()),
                None => return None,
            }
        }
        None
    }

    /// Resolve a custom emoji name to the canonical key used for caching.
    /// Follows alias chains to find the base name that has a direct URL.
    pub fn resolve_emoji_key(&self, name: &str) -> Option<String> {
        let mut current = name;
        for _ in 0..5 {
            match self.custom_emoji.get(current) {
                Some(val) if val.starts_with("alias:") => {
                    current = &val["alias:".len()..];
                }
                Some(_) => return Some(current.to_string()),
                None => return None,
            }
        }
        None
    }

    pub fn has_emoji_image(&self, name: &str) -> bool {
        if let Some(key) = self.resolve_emoji_key(name) {
            self.custom_emoji_images.contains_key(&key)
        } else {
            false
        }
    }

    pub fn emoji_image(&self, name: &str) -> Option<&CachedImage> {
        let key = self.resolve_emoji_key(name)?;
        self.custom_emoji_images.get(&key)
    }

    pub fn request_emoji_load(&mut self, name: &str) {
        if let Some(key) = self.resolve_emoji_key(name) {
            if !self.custom_emoji_images.contains_key(&key)
                && !self.pending_emoji_images.contains(&key)
            {
                self.emoji_load_queue.push(key);
            }
        }
    }

    /// Place a custom emoji image at an absolute screen position.
    /// Returns true if the image was placed (cached and ready), false if
    /// it was enqueued for loading. Callers should render a 2-cell-wide
    /// placeholder in either case.
    pub fn place_inline_emoji(
        &mut self,
        name: &str,
        screen_row: u16,
        screen_col: u16,
    ) -> bool {
        if let Some(key) = self.resolve_emoji_key(name) {
            if self.custom_emoji_images.contains_key(&key) {
                self.inline_emoji_placements.push(InlineEmojiPlacement {
                    emoji_key: key,
                    screen_row,
                    screen_col,
                    display_cols: 2,
                    display_rows: 1,
                });
                return true;
            }
            if !self.pending_emoji_images.contains(&key) {
                self.emoji_load_queue.push(key);
            }
        }
        false
    }

    /// Group channels into sections. Returns (section_name, section_id, channel_indices).
    /// Channels not in any user-defined section go into "Channels".
    /// DMs always go into "Direct Messages" at the end.
    pub fn channels_by_section(&self) -> Vec<(String, Option<String>, Vec<usize>)> {
        let mut result: Vec<(String, Option<String>, Vec<usize>)> = Vec::new();

        // Build a lookup: channel_id -> section index (for user-defined sections)
        let mut ch_to_section: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

        // Sort sections by sort_order
        let mut sorted_sections: Vec<_> = self.channel_sections.iter().collect();
        sorted_sections.sort_by_key(|s| s.sort_order);

        for section in &sorted_sections {
            let section_idx = result.len();
            let emoji_prefix = if section.emoji.is_empty() {
                String::new()
            } else {
                format!("{} ", section.emoji)
            };
            let name = format!("{}{}", emoji_prefix, section.name);
            result.push((name, Some(section.channel_section_id.clone()), Vec::new()));
            for ch_id in &section.channel_ids_page.channel_ids {
                ch_to_section.insert(ch_id.as_str(), section_idx);
            }
        }

        // Default "Channels" group for non-DM channels not in any section
        let mut default_channels: Vec<usize> = Vec::new();
        // DM group
        let mut dm_channels: Vec<usize> = Vec::new();

        for (i, ch) in self.channels.iter().enumerate() {
            if ch.is_im || ch.is_mpim {
                dm_channels.push(i);
            } else if let Some(&section_idx) = ch_to_section.get(ch.id.as_str()) {
                result[section_idx].2.push(i);
            } else {
                default_channels.push(i);
            }
        }

        // Insert default channels at beginning if any exist and there are no user sections,
        // or add them as their own section
        if !default_channels.is_empty() {
            // Insert at the beginning
            result.insert(0, ("Channels".to_string(), None, default_channels));
        }

        // DMs at the end
        if !dm_channels.is_empty() {
            result.push(("Direct Messages".to_string(), None, dm_channels));
        }

        // Remove empty sections
        result.retain(|(_name, _id, channels)| !channels.is_empty());

        result
    }

    /// Toggle collapse state for a channel section.
    pub fn toggle_section_collapse(&mut self, section_id: &str) {
        if !self.collapsed_sections.remove(section_id) {
            self.collapsed_sections.insert(section_id.to_string());
        }
        self.dirty = true;
    }

    /// Move selection by N messages (positive = newer, negative = older).
    pub fn message_select_page(&mut self, delta: isize) {
        let max = self.message_count().saturating_sub(1);
        if delta > 0 {
            self.selected_message_idx = self.selected_message_idx.saturating_sub(delta as usize);
        } else {
            self.selected_message_idx =
                (self.selected_message_idx + (-delta) as usize).min(max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::types::{Channel, ChannelSection, ChannelIdsPage};

    fn make_channel(id: &str, name: &str, is_im: bool) -> Channel {
        Channel {
            id: id.into(),
            name: Some(name.into()),
            is_channel: !is_im,
            is_im,
            is_mpim: false,
            is_private: false,
            is_member: true,
            user: None,
            topic: None,
            purpose: None,
            last_read: None,
            unread_count: 0,
            unread_count_display: 0,
        }
    }

    fn make_section(id: &str, name: &str, channel_ids: Vec<&str>, sort_order: i32) -> ChannelSection {
        ChannelSection {
            channel_section_id: id.into(),
            name: name.into(),
            emoji: String::new(),
            channel_ids_page: ChannelIdsPage {
                channel_ids: channel_ids.into_iter().map(|s| s.to_string()).collect(),
            },
            is_collapsed: false,
            sort_order,
        }
    }

    #[test]
    fn channels_by_section_no_sections() {
        let mut state = AppState::new();
        state.channels = vec![
            make_channel("C1", "general", false),
            make_channel("C2", "random", false),
            make_channel("D1", "dm1", true),
        ];

        let sections = state.channels_by_section();
        assert_eq!(sections.len(), 2); // Channels + DMs
        assert_eq!(sections[0].0, "Channels");
        assert_eq!(sections[0].2.len(), 2);
        assert_eq!(sections[1].0, "Direct Messages");
        assert_eq!(sections[1].2.len(), 1);
    }

    #[test]
    fn channels_by_section_with_user_sections() {
        let mut state = AppState::new();
        state.channels = vec![
            make_channel("C1", "general", false),
            make_channel("C2", "random", false),
            make_channel("C3", "eng", false),
            make_channel("D1", "dm1", true),
        ];
        state.channel_sections = vec![
            make_section("S1", "Important", vec!["C1"], 0),
            make_section("S2", "Work", vec!["C3"], 1),
        ];

        let sections = state.channels_by_section();
        // Should have: default Channels (C2), Important (C1), Work (C3), Direct Messages (D1)
        assert_eq!(sections.len(), 4);

        // Default channels section contains C2 (not in any user section)
        let default = sections.iter().find(|(name, _, _)| name == "Channels").unwrap();
        assert_eq!(default.2.len(), 1); // only C2

        // Important section
        let important = sections.iter().find(|(name, _, _)| name == "Important").unwrap();
        assert_eq!(important.2.len(), 1);
    }

    #[test]
    fn resolve_custom_emoji_direct() {
        let mut state = AppState::new();
        state.custom_emoji.insert("parrot".into(), "https://example.com/parrot.gif".into());

        assert_eq!(
            state.resolve_custom_emoji("parrot"),
            Some("https://example.com/parrot.gif")
        );
        assert_eq!(state.resolve_custom_emoji("unknown"), None);
    }

    #[test]
    fn resolve_custom_emoji_alias_chain() {
        let mut state = AppState::new();
        state.custom_emoji.insert("parrot".into(), "https://example.com/parrot.gif".into());
        state.custom_emoji.insert("party_parrot".into(), "alias:parrot".into());
        state.custom_emoji.insert("pp".into(), "alias:party_parrot".into());

        assert_eq!(
            state.resolve_custom_emoji("pp"),
            Some("https://example.com/parrot.gif")
        );
    }

    #[test]
    fn resolve_custom_emoji_loop_terminates() {
        let mut state = AppState::new();
        state.custom_emoji.insert("a".into(), "alias:b".into());
        state.custom_emoji.insert("b".into(), "alias:a".into());

        // Should terminate without panicking
        assert_eq!(state.resolve_custom_emoji("a"), None);
    }

    #[test]
    fn toggle_section_collapse() {
        let mut state = AppState::new();

        state.toggle_section_collapse("S1");
        assert!(state.collapsed_sections.contains("S1"));

        state.toggle_section_collapse("S1");
        assert!(!state.collapsed_sections.contains("S1"));
    }
}
