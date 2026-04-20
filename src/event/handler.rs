use crate::event::Event;
use crate::slack::client::SlackApi;
use crate::slack::types::{WsEvent, WsMessage};
use crate::state::{AppState, Focus, InputMode};
use crossterm::event::{KeyCode, KeyModifiers};
use tokio::sync::mpsc;
use tracing::error;

pub fn handle_event<C: SlackApi>(
    event: Event,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    match event {
        Event::Key(key) => handle_key(key, state, client, event_tx),
        Event::SlackConnected { self_id, team } => {
            state.self_user_id = self_id;
            state.team_name = team;
            state.connected = true;
            state.dirty = true;
            HandleResult::Continue
        }
        Event::SlackDisconnected => {
            state.connected = false;
            state.dirty = true;
            HandleResult::Continue
        }
        Event::SlackWsEvent(ws_event) => {
            handle_ws_event(ws_event, state);
            HandleResult::Continue
        }
        Event::WsPing(id) => {
            if let Some(ref ws_tx) = state.ws_writer {
                let msg = serde_json::json!({"id": id, "type": "ping"}).to_string();
                let _ = ws_tx.send(msg);
            }
            HandleResult::Continue
        }
        Event::WsWriterReady(ws_tx) => {
            state.ws_writer = Some(ws_tx);
            HandleResult::Continue
        }
        Event::ChannelsLoaded(channels) => {
            state.set_channels(channels);
            state.dirty = true;
            HandleResult::Continue
        }
        Event::HistoryLoaded {
            channel_id,
            messages,
            has_more,
        } => {
            state.set_history(channel_id.clone(), messages, has_more);
            trigger_image_downloads(state, &channel_id, client, event_tx);
            mark_channel_read(state, client, event_tx, &channel_id);
            state.dirty = true;
            HandleResult::Continue
        }
        Event::OlderHistoryLoaded {
            channel_id,
            messages,
            has_more,
        } => {
            state.prepend_history(channel_id, messages, has_more);
            HandleResult::Continue
        }
        Event::ThreadLoaded {
            channel_id,
            thread_ts,
            messages,
        } => {
            state.set_thread_messages(&channel_id, &thread_ts, messages);
            HandleResult::Continue
        }
        Event::UsersLoaded(users) => {
            for user in users {
                state.user_cache.insert(user.id.clone(), user);
            }
            state.dirty = true;
            HandleResult::Continue
        }
        Event::MessageSent { .. } => HandleResult::Continue,
        Event::ChannelMarked { channel_id } => {
            let _ = channel_id;
            HandleResult::Continue
        }
        Event::ImageLoaded {
            url,
            png_data,
            width,
            height,
        } => {
            crate::ui::images::cache_image(state, url, png_data, width, height);
            HandleResult::Continue
        }
        Event::CustomEmojiLoaded(emoji_map) => {
            state.custom_emoji = emoji_map;
            state.dirty = true;
            HandleResult::Continue
        }
        Event::ChannelSectionsLoaded(sections) => {
            state.channel_sections = sections;
            state.dirty = true;
            HandleResult::Continue
        }
        Event::CustomEmojiImageLoaded {
            name,
            png_data,
            width,
            height,
        } => {
            state.pending_emoji_images.remove(&name);
            save_emoji_to_disk(&state.team_id, &name, &png_data);
            state.custom_emoji_images.insert(
                name,
                crate::state::CachedImage {
                    png_data,
                    width,
                    height,
                },
            );
            state.dirty = true;
            HandleResult::Continue
        }
        Event::CustomEmojiImageFailed { name } => {
            state.pending_emoji_images.remove(&name);
            HandleResult::Continue
        }
        Event::AvatarImageLoaded {
            user_id,
            png_data,
            width,
            height,
        } => {
            state.pending_avatar_images.remove(&user_id);
            save_avatar_to_disk(&state.team_id, &user_id, &png_data);
            state.avatar_images.insert(
                user_id,
                crate::state::CachedImage {
                    png_data,
                    width,
                    height,
                },
            );
            state.dirty = true;
            HandleResult::Continue
        }
        Event::AvatarImageFailed { user_id } => {
            state.pending_avatar_images.remove(&user_id);
            HandleResult::Continue
        }
        Event::SearchResultsLoaded {
            query,
            matches,
            total,
        } => {
            if query == state.global_search_query {
                state.global_search_results = matches;
                state.global_search_total = total;
                state.global_search_loading = false;
                state.global_search_selected = 0;
                state.dirty = true;
            }
            HandleResult::Continue
        }
        Event::ApiError(err) => {
            state.last_error = Some(err);
            state.dirty = true;
            HandleResult::Continue
        }
        Event::Tick => {
            state.expire_typing();
            HandleResult::Continue
        }
        Event::Resize(_, _) => {
            state.dirty = true;
            HandleResult::Continue
        }
        Event::Mouse(mouse) => {
            handle_mouse_event(mouse, state, client, event_tx);
            HandleResult::Continue
        }
    }
}

fn handle_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return HandleResult::Quit;
    }

    // Help overlay: any key dismisses it
    if state.show_help {
        state.show_help = false;
        state.dirty = true;
        return HandleResult::Continue;
    }

    match state.input_mode {
        InputMode::Normal => handle_normal_key(key, state, client, event_tx),
        InputMode::Insert => handle_insert_key(key, state, client, event_tx),
        InputMode::Search => handle_search_key(key, state, client, event_tx),
        InputMode::MessageSearch => handle_message_search_key(key, state),
        InputMode::Reaction => handle_reaction_key(key, state, client, event_tx),
        InputMode::EmojiPicker => handle_emoji_picker_key(key, state, client, event_tx),
        InputMode::UserPicker => handle_user_picker_key(key, state),
        InputMode::GlobalSearch => handle_global_search_key(key, state, client, event_tx),
    }
}

// ── Normal mode ─────────────────────────────────────────────────────────────

fn handle_normal_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;

    match state.focus {
        Focus::ChannelList => handle_normal_channels(key, state, client, event_tx),
        Focus::Messages => handle_normal_messages(key, state, client, event_tx),
        Focus::Thread => handle_normal_thread(key, state, client, event_tx),
        Focus::Input => {
            // Shouldn't be in Normal mode with Input focus, switch to insert
            state.input_mode = InputMode::Insert;
            HandleResult::Continue
        }
    }
}

fn handle_normal_channels<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    match key.code {
        KeyCode::Char('q') => return HandleResult::Quit,
        KeyCode::Char('?') => {
            state.show_help = true;
        }
        KeyCode::Char('F') => {
            state.show_fps = !state.show_fps;
        }
        KeyCode::Char('S') => {
            state.input_mode = InputMode::GlobalSearch;
            state.global_search_query.clear();
            state.global_search_results.clear();
            state.global_search_selected = 0;
            state.global_search_loading = false;
        }
        // Navigation
        KeyCode::Char('j') | KeyCode::Down => state.channel_next(),
        KeyCode::Char('k') | KeyCode::Up => state.channel_prev(),
        KeyCode::Char('G') | KeyCode::End => {
            let visible = state.visible_channel_indices();
            if let Some(&last) = visible.last() {
                state.selected_channel_idx = last;
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            let visible = state.visible_channel_indices();
            if let Some(&first) = visible.first() {
                state.selected_channel_idx = first;
            }
        }
        KeyCode::PageDown => {
            let visible = state.visible_channel_indices();
            if let Some(pos) = visible.iter().position(|&i| i == state.selected_channel_idx) {
                let target = (pos + 15).min(visible.len().saturating_sub(1));
                state.selected_channel_idx = visible[target];
            }
        }
        KeyCode::PageUp => {
            let visible = state.visible_channel_indices();
            if let Some(pos) = visible.iter().position(|&i| i == state.selected_channel_idx) {
                let target = pos.saturating_sub(15);
                state.selected_channel_idx = visible[target];
            }
        }
        // Open channel / toggle section
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            // Check if the currently selected visual item is a section header or DmMore
            let visual_entry = find_selected_visual_entry(state);
            match visual_entry {
                Some(crate::state::ChannelListEntry::SectionHeader(ref id))
                    if id != "__channels__" && id != "__dm__" =>
                {
                    state.toggle_section_collapse(id);
                }
                Some(crate::state::ChannelListEntry::DmMore) => {
                    state.dm_list_expanded = true;
                    state.dirty = true;
                }
                _ => {
                    state.focus = Focus::Messages;
                    state.selected_message_idx = 0;
                    ensure_history_loaded(state, client, event_tx);
                }
            }
        }
        // Search
        KeyCode::Char('/') => {
            state.input_mode = InputMode::Search;
            state.channel_filter.clear();
            state.channel_filter_active = true;
        }
        // Insert mode (from channels → always to channel)
        KeyCode::Char('i') | KeyCode::Char('a') => {
            state.reply_to_thread = false;
            state.focus = Focus::Input;
            state.input_mode = InputMode::Insert;
            ensure_history_loaded(state, client, event_tx);
        }
        // Cycle focus
        KeyCode::Tab => {
            state.focus = Focus::Messages;
            ensure_history_loaded(state, client, event_tx);
        }
        // Unread navigation
        KeyCode::Char('}') => {
            state.next_unread_channel();
        }
        KeyCode::Char('{') => {
            state.prev_unread_channel();
        }
        _ => {}
    }
    HandleResult::Continue
}

/// Find the visual channel list entry corresponding to the currently selected channel.
fn find_selected_visual_entry(state: &AppState) -> Option<crate::state::ChannelListEntry> {
    // Find the visual item that corresponds to the selected channel
    state
        .channel_list_items
        .iter()
        .find(|entry| matches!(entry, crate::state::ChannelListEntry::Channel(idx) if *idx == state.selected_channel_idx))
        .cloned()
}

fn handle_normal_messages<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    // Ctrl modifiers
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('f') => {
                // Half/full page down (newer messages)
                let page = if key.code == KeyCode::Char('f') { 15 } else { 8 };
                state.message_select_page(page);
            }
            KeyCode::Char('u') | KeyCode::Char('b') => {
                // Half/full page up (older messages)
                let page = if key.code == KeyCode::Char('b') { 15 } else { 8 };
                state.message_select_page(-page);
                maybe_load_older(state, client, event_tx);
            }
            _ => {}
        }
        return HandleResult::Continue;
    }

    match key.code {
        KeyCode::Char('q') => return HandleResult::Quit,
        KeyCode::Char('?') => {
            state.show_help = true;
        }
        KeyCode::Char('F') => {
            state.show_fps = !state.show_fps;
        }
        KeyCode::Char('S') => {
            state.input_mode = InputMode::GlobalSearch;
            state.global_search_query.clear();
            state.global_search_results.clear();
            state.global_search_selected = 0;
            state.global_search_loading = false;
        }
        // Message selection (j/k navigate between messages)
        KeyCode::Char('j') | KeyCode::Down => {
            state.message_select_newer();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.message_select_older();
            maybe_load_older(state, client, event_tx);
        }
        KeyCode::Char('G') | KeyCode::End => {
            state.selected_message_idx = 0; // newest
        }
        KeyCode::Char('g') | KeyCode::Home => {
            let max = state.message_count().saturating_sub(1);
            state.selected_message_idx = max; // oldest
            maybe_load_older(state, client, event_tx);
        }
        KeyCode::PageDown => {
            state.message_select_page(15);
        }
        KeyCode::PageUp => {
            state.message_select_page(-15);
            maybe_load_older(state, client, event_tx);
        }
        // Back to channel list
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => {
            state.focus = Focus::ChannelList;
            state.clear_message_search();
        }
        // Open thread for selected message
        KeyCode::Enter | KeyCode::Char('l') => {
            open_thread_for_selected(state, client, event_tx);
        }
        // Reply in thread (open thread + insert mode)
        KeyCode::Char('R') => {
            open_thread_for_selected(state, client, event_tx);
            state.reply_to_thread = true;
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        // Insert mode (from messages → always to channel, not thread)
        KeyCode::Char('i') | KeyCode::Char('a') => {
            state.reply_to_thread = false;
            state.focus = Focus::Input;
            state.input_mode = InputMode::Insert;
        }
        // Reaction via emoji picker
        KeyCode::Char('r') => {
            if state.selected_message().is_some() {
                state.open_emoji_picker(crate::state::EmojiPickerSource::Reaction);
            }
        }
        // Message search
        KeyCode::Char('/') => {
            state.input_mode = InputMode::MessageSearch;
            state.message_search_query.clear();
            state.message_search_results.clear();
            state.message_search_idx = 0;
        }
        // Navigate search results
        KeyCode::Char('n') => {
            if state.message_search_active {
                state.message_search_next();
            }
        }
        KeyCode::Char('N') => {
            if state.message_search_active {
                state.message_search_prev();
            }
        }
        // Cycle focus
        KeyCode::Tab => {
            if state.thread_channel_id.is_some() {
                state.focus = Focus::Thread;
            } else {
                state.focus = Focus::ChannelList;
            }
        }
        KeyCode::BackTab => {
            state.focus = Focus::ChannelList;
        }
        // Channel navigation from messages view
        KeyCode::Char(']') => {
            state.close_thread();
            state.channel_next();
            state.selected_message_idx = 0;
            state.clear_message_search();
            ensure_history_loaded(state, client, event_tx);
        }
        KeyCode::Char('[') => {
            state.close_thread();
            state.channel_prev();
            state.selected_message_idx = 0;
            state.clear_message_search();
            ensure_history_loaded(state, client, event_tx);
        }
        KeyCode::Char('}') => {
            if state.next_unread_channel() {
                state.close_thread();
                state.selected_message_idx = 0;
                state.clear_message_search();
                ensure_history_loaded(state, client, event_tx);
            }
        }
        KeyCode::Char('{') => {
            if state.prev_unread_channel() {
                state.close_thread();
                state.selected_message_idx = 0;
                state.clear_message_search();
                ensure_history_loaded(state, client, event_tx);
            }
        }
        // Copy selected message to clipboard (OSC 52)
        KeyCode::Char('y') => {
            if let Some(msg) = state.selected_message() {
                let text = msg.text.clone();
                state.clipboard_pending = Some(text);
            }
        }
        _ => {}
    }
    HandleResult::Continue
}

fn handle_normal_thread<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    // Ctrl modifiers for page scroll
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('f') => {
                state.thread_scroll_offset = state
                    .thread_scroll_offset
                    .saturating_sub(state.thread_max_scroll_offset.min(40) / 2);
            }
            KeyCode::Char('u') | KeyCode::Char('b') => {
                let amount = state.thread_max_scroll_offset.min(40) / 2;
                state.thread_scroll_offset =
                    (state.thread_scroll_offset + amount).min(state.thread_max_scroll_offset);
            }
            _ => {}
        }
        return HandleResult::Continue;
    }

    match key.code {
        KeyCode::Char('q') => return HandleResult::Quit,
        KeyCode::Char('?') => {
            state.show_help = true;
        }
        KeyCode::Char('F') => {
            state.show_fps = !state.show_fps;
        }
        KeyCode::Char('S') => {
            state.input_mode = InputMode::GlobalSearch;
            state.global_search_query.clear();
            state.global_search_results.clear();
            state.global_search_selected = 0;
            state.global_search_loading = false;
        }
        // Close thread
        KeyCode::Esc | KeyCode::Char('h') => {
            state.close_thread();
        }
        // Scroll
        KeyCode::Char('j') | KeyCode::Down => {
            state.thread_scroll_offset = state.thread_scroll_offset.saturating_sub(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.thread_scroll_offset =
                (state.thread_scroll_offset + 1).min(state.thread_max_scroll_offset);
        }
        KeyCode::Char('G') | KeyCode::End => {
            state.thread_scroll_offset = 0;
        }
        KeyCode::Char('g') | KeyCode::Home => {
            state.thread_scroll_offset = state.thread_max_scroll_offset;
        }
        KeyCode::PageDown => {
            let page = 15;
            state.thread_scroll_offset = state.thread_scroll_offset.saturating_sub(page);
        }
        KeyCode::PageUp => {
            let page = 15;
            state.thread_scroll_offset =
                (state.thread_scroll_offset + page).min(state.thread_max_scroll_offset);
        }
        // Insert mode (from thread → always reply to thread)
        KeyCode::Char('i') | KeyCode::Char('a') | KeyCode::Char('R') => {
            state.reply_to_thread = true;
            state.focus = Focus::Input;
            state.input_mode = InputMode::Insert;
        }
        // Reaction via emoji picker (on thread parent for now)
        KeyCode::Char('r') => {
            if state.thread_parent_ts.is_some() && state.thread_channel_id.is_some() {
                state.open_emoji_picker(crate::state::EmojiPickerSource::Reaction);
            }
        }
        // Cycle focus
        KeyCode::Tab => {
            state.focus = Focus::Messages;
        }
        KeyCode::BackTab => {
            state.focus = Focus::ChannelList;
        }
        // Channel navigation from thread
        KeyCode::Char(']') => {
            state.close_thread();
            state.channel_next();
            state.selected_message_idx = 0;
            ensure_history_loaded(state, client, event_tx);
        }
        KeyCode::Char('[') => {
            state.close_thread();
            state.channel_prev();
            state.selected_message_idx = 0;
            ensure_history_loaded(state, client, event_tx);
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Insert mode ─────────────────────────────────────────────────────────────

fn handle_insert_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;

    // Ctrl modifiers for input editing
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('w') => delete_word_backward(state),
            KeyCode::Char('u') => {
                state.input_text.drain(..state.input_cursor);
                state.input_cursor = 0;
            }
            KeyCode::Char('k') => {
                state.input_text.truncate(state.input_cursor);
            }
            KeyCode::Char('a') => {
                state.input_cursor = 0;
            }
            KeyCode::Char('e') => {
                // Ctrl+e: if there's no text or cursor is at end, open emoji picker
                // Otherwise jump to end of line (standard behavior)
                if state.input_cursor == state.input_text.len() {
                    state.open_emoji_picker(crate::state::EmojiPickerSource::Insert);
                    return HandleResult::Continue;
                }
                state.input_cursor = state.input_text.len();
            }
            KeyCode::Char('p') => {
                state.input_history_prev();
            }
            KeyCode::Char('n') => {
                state.input_history_next();
            }
            _ => {}
        }
        return HandleResult::Continue;
    }

    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            if state.thread_channel_id.is_some() && state.focus == Focus::Input {
                state.focus = Focus::Thread;
            } else {
                state.focus = Focus::Messages;
            }
        }
        KeyCode::Enter => {
            let text = state.input_text.trim().to_string();
            if !text.is_empty() {
                let thread_ts = if state.reply_to_thread {
                    state.thread_parent_ts.clone()
                } else {
                    None
                };

                if let Some(channel_id) = state.active_channel_id() {
                    spawn_send_message(
                        client,
                        &channel_id,
                        &text,
                        thread_ts.as_deref(),
                        event_tx,
                    );
                    state.save_input_to_history();
                }
            }
        }
        KeyCode::Backspace => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
                state.input_text.remove(state.input_cursor);
            }
        }
        KeyCode::Delete => {
            if state.input_cursor < state.input_text.len() {
                state.input_text.remove(state.input_cursor);
            }
        }
        KeyCode::Left => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
            }
        }
        KeyCode::Right => {
            if state.input_cursor < state.input_text.len() {
                state.input_cursor += 1;
            }
        }
        KeyCode::Up => state.input_history_prev(),
        KeyCode::Down => state.input_history_next(),
        KeyCode::Home => state.input_cursor = 0,
        KeyCode::End => state.input_cursor = state.input_text.len(),
        KeyCode::Char('@') => {
            state.open_user_picker();
            return HandleResult::Continue;
        }
        KeyCode::Char(c) => {
            state.input_text.insert(state.input_cursor, c);
            state.input_cursor += 1;
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Channel search mode ─────────────────────────────────────────────────────

fn handle_search_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            state.channel_filter.clear();
            state.channel_filter_active = false;
        }
        KeyCode::Enter => {
            state.input_mode = InputMode::Normal;
            state.channel_filter.clear();
            state.channel_filter_active = false;
            state.focus = Focus::Messages;
            state.selected_message_idx = 0;
            ensure_history_loaded(state, client, event_tx);
        }
        KeyCode::Backspace => {
            state.channel_filter.pop();
            let indices = state.filtered_channel_indices();
            if let Some(&first) = indices.first() {
                state.selected_channel_idx = first;
            }
        }
        KeyCode::Down | KeyCode::Tab => state.filtered_channel_next(),
        KeyCode::Up | KeyCode::BackTab => state.filtered_channel_prev(),
        KeyCode::Char(c) => {
            state.channel_filter.push(c);
            let indices = state.filtered_channel_indices();
            if let Some(&first) = indices.first() {
                state.selected_channel_idx = first;
            }
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Message search mode ─────────────────────────────────────────────────────

fn handle_message_search_key(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            state.clear_message_search();
        }
        KeyCode::Enter => {
            // Confirm search — keep results highlighted, return to normal
            state.input_mode = InputMode::Normal;
            if !state.message_search_query.is_empty() {
                state.message_search_active = true;
                state.perform_message_search();
                // Jump to first result
                if !state.message_search_results.is_empty() {
                    state.jump_to_search_result();
                }
            }
        }
        KeyCode::Backspace => {
            state.message_search_query.pop();
            state.perform_message_search();
        }
        // Navigate results while typing
        KeyCode::Down | KeyCode::Tab => {
            state.message_search_active = true;
            state.perform_message_search();
            state.message_search_next();
        }
        KeyCode::Up | KeyCode::BackTab => {
            state.message_search_active = true;
            state.perform_message_search();
            state.message_search_prev();
        }
        KeyCode::Char(c) => {
            state.message_search_query.push(c);
            state.perform_message_search();
            // Auto-jump to first result as user types
            if !state.message_search_results.is_empty() {
                state.message_search_active = true;
                state.message_search_idx = 0;
                state.jump_to_search_result();
            }
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Reaction mode ───────────────────────────────────────────────────────────

fn handle_reaction_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            state.reaction_input.clear();
        }
        KeyCode::Enter => {
            let emoji = state.reaction_input.trim().to_string();
            if !emoji.is_empty() {
                if let Some(msg) = state.selected_message() {
                    let ts = msg.ts.clone();
                    if let Some(channel_id) = state.active_channel_id() {
                        spawn_add_reaction(client, &channel_id, &ts, &emoji, event_tx);
                    }
                }
            }
            state.input_mode = InputMode::Normal;
            state.reaction_input.clear();
        }
        KeyCode::Backspace => {
            state.reaction_input.pop();
        }
        KeyCode::Char(c) => {
            state.reaction_input.push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Emoji picker mode ──────────────────────────────────────────────────

fn handle_emoji_picker_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
        }
        KeyCode::Enter => {
            if let Some((name, _display, _is_custom)) = state
                .emoji_picker_results
                .get(state.emoji_picker_selected)
                .cloned()
            {
                match state.emoji_picker_source {
                    crate::state::EmojiPickerSource::Reaction => {
                        // Add reaction to selected message
                        if state.focus == Focus::Thread {
                            if let (Some(channel_id), Some(ts)) = (
                                state.thread_parent_ts.clone().and_then(|_| state.thread_channel_id.clone()),
                                state.thread_parent_ts.clone(),
                            ) {
                                spawn_add_reaction(client, &channel_id, &ts, &name, event_tx);
                            }
                        } else if let Some(msg) = state.selected_message() {
                            let ts = msg.ts.clone();
                            if let Some(channel_id) = state.active_channel_id() {
                                spawn_add_reaction(client, &channel_id, &ts, &name, event_tx);
                            }
                        }
                        state.input_mode = InputMode::Normal;
                    }
                    crate::state::EmojiPickerSource::Insert => {
                        // Insert :name: at cursor position
                        let insert = format!(":{}:", name);
                        state.input_text.insert_str(state.input_cursor, &insert);
                        state.input_cursor += insert.len();
                        state.input_mode = InputMode::Insert;
                        state.focus = Focus::Input;
                    }
                }
            } else {
                state.input_mode = InputMode::Normal;
            }
        }
        KeyCode::Backspace => {
            state.emoji_picker_query.pop();
            state.filter_emoji_picker();
        }
        KeyCode::Char('j') | KeyCode::Down if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.emoji_picker_results.is_empty() {
                state.emoji_picker_selected =
                    (state.emoji_picker_selected + 1) % state.emoji_picker_results.len();
            }
        }
        KeyCode::Char('k') | KeyCode::Up if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.emoji_picker_results.is_empty() {
                if state.emoji_picker_selected == 0 {
                    state.emoji_picker_selected = state.emoji_picker_results.len() - 1;
                } else {
                    state.emoji_picker_selected -= 1;
                }
            }
        }
        KeyCode::PageDown => {
            let page = 15;
            if !state.emoji_picker_results.is_empty() {
                state.emoji_picker_selected = (state.emoji_picker_selected + page)
                    .min(state.emoji_picker_results.len() - 1);
            }
        }
        KeyCode::PageUp => {
            let page = 15;
            state.emoji_picker_selected = state.emoji_picker_selected.saturating_sub(page);
        }
        KeyCode::Tab => {
            if !state.emoji_picker_results.is_empty() {
                state.emoji_picker_selected =
                    (state.emoji_picker_selected + 1) % state.emoji_picker_results.len();
            }
        }
        KeyCode::BackTab => {
            if !state.emoji_picker_results.is_empty() {
                if state.emoji_picker_selected == 0 {
                    state.emoji_picker_selected = state.emoji_picker_results.len() - 1;
                } else {
                    state.emoji_picker_selected -= 1;
                }
            }
        }
        KeyCode::Char(c) => {
            state.emoji_picker_query.push(c);
            state.filter_emoji_picker();
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── User picker ────────────────────────────────────────────────────────────

fn handle_user_picker_key(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_text.insert(state.input_cursor, '@');
            state.input_cursor += 1;
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        KeyCode::Enter => {
            if let Some((user_id, _display)) = state
                .user_picker_results
                .get(state.user_picker_selected)
                .cloned()
            {
                let insert = format!("<@{}>", user_id);
                state.input_text.insert_str(state.input_cursor, &insert);
                state.input_cursor += insert.len();
            }
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        KeyCode::Backspace => {
            if state.user_picker_query.is_empty() {
                state.input_text.insert(state.input_cursor, '@');
                state.input_cursor += 1;
                state.input_mode = InputMode::Insert;
                state.focus = Focus::Input;
            } else {
                state.user_picker_query.pop();
                state.filter_user_picker();
            }
        }
        KeyCode::Char('j') | KeyCode::Down if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.user_picker_results.is_empty() {
                state.user_picker_selected =
                    (state.user_picker_selected + 1) % state.user_picker_results.len();
            }
        }
        KeyCode::Char('k') | KeyCode::Up if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.user_picker_results.is_empty() {
                if state.user_picker_selected == 0 {
                    state.user_picker_selected = state.user_picker_results.len() - 1;
                } else {
                    state.user_picker_selected -= 1;
                }
            }
        }
        KeyCode::PageDown => {
            let page = 15;
            if !state.user_picker_results.is_empty() {
                state.user_picker_selected = (state.user_picker_selected + page)
                    .min(state.user_picker_results.len() - 1);
            }
        }
        KeyCode::PageUp => {
            let page = 15;
            state.user_picker_selected = state.user_picker_selected.saturating_sub(page);
        }
        KeyCode::Tab => {
            if !state.user_picker_results.is_empty() {
                state.user_picker_selected =
                    (state.user_picker_selected + 1) % state.user_picker_results.len();
            }
        }
        KeyCode::BackTab => {
            if !state.user_picker_results.is_empty() {
                if state.user_picker_selected == 0 {
                    state.user_picker_selected = state.user_picker_results.len() - 1;
                } else {
                    state.user_picker_selected -= 1;
                }
            }
        }
        KeyCode::Char(c) => {
            state.user_picker_query.push(c);
            state.filter_user_picker();
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Global search ──────────────────────────────────────────────────────────

fn handle_global_search_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            state.global_search_results.clear();
        }
        KeyCode::Enter => {
            if !state.global_search_results.is_empty() {
                if let Some(result) = state
                    .global_search_results
                    .get(state.global_search_selected)
                    .cloned()
                {
                    if let Some(ch) = &result.channel {
                        let channel_id = ch.id.clone();
                        state.global_search_results.clear();
                        state.input_mode = InputMode::Normal;
                        if let Some(idx) =
                            state.channels.iter().position(|c| c.id == channel_id)
                        {
                            state.selected_channel_idx = idx;
                            state.close_thread();
                            state.clear_message_search();
                            state.focus = Focus::Messages;
                            state.selected_message_idx = 0;
                            ensure_history_loaded(state, client, event_tx);
                        }
                    }
                }
            } else if !state.global_search_query.is_empty() && !state.global_search_loading {
                state.global_search_loading = true;
                spawn_search_messages(client, &state.global_search_query, event_tx);
            }
        }
        KeyCode::Backspace => {
            state.global_search_query.pop();
        }
        KeyCode::Down | KeyCode::Tab => {
            if !state.global_search_results.is_empty() {
                state.global_search_selected =
                    (state.global_search_selected + 1) % state.global_search_results.len();
            }
        }
        KeyCode::Up | KeyCode::BackTab => {
            if !state.global_search_results.is_empty() {
                if state.global_search_selected == 0 {
                    state.global_search_selected = state.global_search_results.len() - 1;
                } else {
                    state.global_search_selected -= 1;
                }
            }
        }
        KeyCode::Char(c) => {
            state.global_search_query.push(c);
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Mouse ──────────────────────────────────────────────────────────────────

fn handle_mouse_event<C: SlackApi>(
    mouse: crossterm::event::MouseEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    use crossterm::event::MouseEventKind;
    use ratatui::layout::Rect;

    fn contains(r: Rect, col: u16, row: u16) -> bool {
        col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
    }

    let col = mouse.column;
    let row = mouse.row;
    let scroll_lines: usize = 3;

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if contains(state.channel_list_area, col, row) {
                state.channel_prev();
                state.dirty = true;
            } else if state.thread_area.map_or(false, |r| contains(r, col, row)) {
                let old = state.thread_scroll_offset;
                state.thread_scroll_offset = (state.thread_scroll_offset + scroll_lines)
                    .min(state.thread_max_scroll_offset);
                if state.thread_scroll_offset != old {
                    state.dirty = true;
                }
            } else if contains(state.messages_area, col, row) {
                if state.message_select_page(-1) {
                    state.dirty = true;
                }
                maybe_load_older(state, client, event_tx);
            }
        }
        MouseEventKind::ScrollDown => {
            if contains(state.channel_list_area, col, row) {
                state.channel_next();
                state.dirty = true;
            } else if state.thread_area.map_or(false, |r| contains(r, col, row)) {
                let old = state.thread_scroll_offset;
                state.thread_scroll_offset = state.thread_scroll_offset.saturating_sub(scroll_lines);
                if state.thread_scroll_offset != old {
                    state.dirty = true;
                }
            } else if contains(state.messages_area, col, row) {
                if state.message_select_page(1) {
                    state.dirty = true;
                }
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            if contains(state.channel_list_area, col, row) {
                // Click on channel list: select channel, open messages
                let inner_row = (row - state.channel_list_area.y).saturating_sub(1) as usize;
                let visual_idx = state.channel_list_offset + inner_row;
                if let Some(entry) = state.channel_list_items.get(visual_idx).cloned() {
                    match entry {
                        crate::state::ChannelListEntry::Channel(ch_idx) => {
                            state.selected_channel_idx = ch_idx;
                            state.selected_message_idx = 0;
                            state.focus = Focus::Messages;
                            state.input_mode = InputMode::Normal;
                            ensure_history_loaded(state, client, event_tx);
                        }
                        crate::state::ChannelListEntry::SectionHeader(id) => {
                            state.toggle_section_collapse(&id);
                        }
                        crate::state::ChannelListEntry::DmMore => {
                            state.dm_list_expanded = !state.dm_list_expanded;
                        }
                        _ => {}
                    }
                }
                state.dirty = true;
            } else if contains(state.messages_area, col, row) {
                // Click on messages: select the clicked message
                if let Some(info) = &state.messages_render_info {
                    let click_row = (row - info.inner_y) as usize;
                    let vline = info.scroll_y + click_row;
                    // Find which message this line belongs to
                    let msg_count = state.message_count();
                    if msg_count > 0 && !state.message_line_starts.is_empty() {
                        let msg_idx = match state.message_line_starts.binary_search(&vline) {
                            Ok(i) => i,
                            Err(i) => i.saturating_sub(1),
                        };
                        if msg_idx < msg_count {
                            state.selected_message_idx =
                                msg_count.saturating_sub(1).saturating_sub(msg_idx);
                            state.focus = Focus::Messages;
                        }
                    }
                }
                state.dirty = true;
            }
        }
        _ => {}
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn open_thread_for_selected<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    if let Some(msg) = state.selected_message() {
        let ts = msg
            .thread_ts
            .clone()
            .unwrap_or_else(|| msg.ts.clone());
        if let Some(channel_id) = state.active_channel_id() {
            spawn_load_thread(client, &channel_id, &ts, event_tx);
            state.open_thread(channel_id, ts);
        }
    }
}

fn delete_word_backward(state: &mut AppState) {
    if state.input_cursor == 0 {
        return;
    }
    let mut pos = state.input_cursor;
    while pos > 0 && state.input_text.as_bytes()[pos - 1] == b' ' {
        pos -= 1;
    }
    while pos > 0 && state.input_text.as_bytes()[pos - 1] != b' ' {
        pos -= 1;
    }
    state.input_text.drain(pos..state.input_cursor);
    state.input_cursor = pos;
}

// ── WebSocket events ────────────────────────────────────────────────────────

fn handle_ws_event(ws_event: WsEvent, state: &mut AppState) {
    match ws_event {
        WsEvent::Hello => {
            state.connected = true;
            state.dirty = true;
        }
        WsEvent::Goodbye => {
            state.connected = false;
            state.dirty = true;
        }
        WsEvent::Message(ws_msg) => handle_ws_message(ws_msg, state),
        WsEvent::ReactionAdded(reaction) => {
            if let (Some(item), Some(reaction_name), Some(user)) =
                (reaction.item, reaction.reaction, reaction.user)
            {
                if let (Some(channel), Some(ts)) = (item.channel, item.ts) {
                    state.add_reaction(&channel, &ts, &reaction_name, &user);
                }
            }
        }
        WsEvent::ReactionRemoved(reaction) => {
            if let (Some(item), Some(reaction_name), Some(user)) =
                (reaction.item, reaction.reaction, reaction.user)
            {
                if let (Some(channel), Some(ts)) = (item.channel, item.ts) {
                    state.remove_reaction(&channel, &ts, &reaction_name, &user);
                }
            }
        }
        WsEvent::ChannelMarked(marked) => {
            if let (Some(channel), Some(ts)) = (marked.channel, marked.ts) {
                state.mark_channel_read(&channel, &ts);
            }
        }
        WsEvent::UserTyping(typing) => {
            if let (Some(channel), Some(user)) = (typing.channel, typing.user) {
                state.record_typing(&channel, &user);
            }
        }
        WsEvent::PresenceChange(_) => {}
        WsEvent::Error(_) => {}
    }
}

fn handle_ws_message(ws_msg: WsMessage, state: &mut AppState) {
    let channel_id = match ws_msg.channel.as_ref() {
        Some(c) => c.clone(),
        None => return,
    };

    // Check if we should show a desktop notification before consuming fields
    let should_notify = ws_msg
        .user
        .as_ref()
        .map(|u| u != &state.self_user_id)
        .unwrap_or(false)
        && ws_msg.subtype.is_none()
        && state.active_channel_id().as_deref() != Some(&channel_id);

    let notify_sender = if should_notify {
        ws_msg.user.as_ref().map(|uid| state.user_display_name(uid).to_string())
    } else {
        None
    };
    let notify_channel = if should_notify {
        state
            .channels
            .iter()
            .find(|c| c.id == channel_id)
            .map(|c| {
                if c.is_im {
                    "DM".to_string()
                } else {
                    format!("#{}", c.display_name())
                }
            })
    } else {
        None
    };
    let notify_text = if should_notify {
        let text = &ws_msg.text;
        if text.len() > 100 {
            format!("{}...", &text[..97])
        } else {
            text.clone()
        }
    } else {
        String::new()
    };

    let msg = crate::slack::types::Message {
        user: ws_msg.user,
        text: ws_msg.text,
        ts: ws_msg.ts,
        thread_ts: ws_msg.thread_ts,
        reply_count: None,
        reactions: Vec::new(),
        edited: None,
        subtype: ws_msg.subtype,
        bot_id: None,
        username: None,
        files: Vec::new(),
    };

    state.push_message(channel_id, msg);
    state.channels_need_resort = true;

    // Fire desktop notification (fire-and-forget)
    if let (Some(sender), Some(channel)) = (notify_sender, notify_channel) {
        tokio::spawn(async move {
            let _ = notify_rust::Notification::new()
                .summary(&format!("{} in {}", sender, channel))
                .body(&notify_text)
                .appname("slackslack")
                .timeout(notify_rust::Timeout::Milliseconds(5000))
                .show();
        });
    }
}

// ── API spawners ────────────────────────────────────────────────────────────

fn ensure_history_loaded<C: SlackApi>(
    state: &AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    if let Some(channel_id) = state.active_channel_id() {
        if !state.channel_data.contains_key(&channel_id) {
            spawn_load_history(client, &channel_id, event_tx);
        }
    }
}

fn maybe_load_older<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    if let Some(channel_id) = state.active_channel_id() {
        let max = state.message_count().saturating_sub(1);
        let near_top = state.selected_message_idx >= max.saturating_sub(5);

        if let Some(cd) = state.channel_data.get(&channel_id) {
            if near_top && cd.has_more_history && !cd.loading_more_history {
                if let Some(oldest) = cd.messages.front() {
                    let oldest_ts = oldest.ts.clone();
                    state.channel_data_mut(&channel_id).loading_more_history = true;
                    spawn_load_older_history(client, &channel_id, &oldest_ts, event_tx);
                }
            }
        }
    }
}

fn mark_channel_read<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
    channel_id: &str,
) {
    let latest_ts = state
        .channel_data.get(channel_id)
        .and_then(|cd| cd.messages.back())
        .map(|m| m.ts.clone());

    if let Some(ts) = latest_ts {
        state.mark_channel_read(channel_id, &ts);
        spawn_mark_read(client, channel_id, &ts, event_tx);
    }
}

fn spawn_load_history<C: SlackApi>(
    client: &C,
    channel_id: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client
            .conversations_history(&channel_id, 50, None, None)
            .await
        {
            Ok(data) => {
                let _ = tx.send(Event::HistoryLoaded {
                    channel_id,
                    messages: data.messages,
                    has_more: data.has_more,
                });
            }
            Err(e) => {
                error!("Failed to load history: {}", e);
                let _ = tx.send(Event::ApiError(format!("History: {}", e)));
            }
        }
    });
}

fn spawn_load_older_history<C: SlackApi>(
    client: &C,
    channel_id: &str,
    oldest_ts: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let latest = oldest_ts.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client
            .conversations_history(&channel_id, 50, None, Some(&latest))
            .await
        {
            Ok(data) => {
                let _ = tx.send(Event::OlderHistoryLoaded {
                    channel_id,
                    messages: data.messages,
                    has_more: data.has_more,
                });
            }
            Err(e) => {
                error!("Failed to load older history: {}", e);
                let _ = tx.send(Event::ApiError(format!("Older history: {}", e)));
            }
        }
    });
}

fn spawn_load_thread<C: SlackApi>(
    client: &C,
    channel_id: &str,
    thread_ts: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let thread_ts = thread_ts.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client
            .conversations_replies(&channel_id, &thread_ts, 100)
            .await
        {
            Ok(data) => {
                let _ = tx.send(Event::ThreadLoaded {
                    channel_id,
                    thread_ts,
                    messages: data.messages,
                });
            }
            Err(e) => {
                error!("Failed to load thread: {}", e);
                let _ = tx.send(Event::ApiError(format!("Thread: {}", e)));
            }
        }
    });
}

fn spawn_send_message<C: SlackApi>(
    client: &C,
    channel_id: &str,
    text: &str,
    thread_ts: Option<&str>,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let text = text.to_string();
    let thread_ts = thread_ts.map(|s| s.to_string());
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client
            .chat_post_message(&channel_id, &text, thread_ts.as_deref())
            .await
        {
            Ok(data) => {
                let _ = tx.send(Event::MessageSent {
                    channel_id,
                    ts: data.ts.unwrap_or_default(),
                });
            }
            Err(e) => {
                error!("Failed to send message: {}", e);
                let _ = tx.send(Event::ApiError(format!("Send: {}", e)));
            }
        }
    });
}

fn spawn_mark_read<C: SlackApi>(
    client: &C,
    channel_id: &str,
    ts: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let ts = ts.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client.conversations_mark(&channel_id, &ts).await {
            Ok(_) => {
                let _ = tx.send(Event::ChannelMarked { channel_id });
            }
            Err(e) => {
                error!("Failed to mark channel: {}", e);
            }
        }
    });
}

fn spawn_search_messages<C: SlackApi>(
    client: &C,
    query: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let query = query.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client.search_messages(&query, 1, 20).await {
            Ok(data) => {
                let total = data
                    .messages
                    .paging
                    .as_ref()
                    .and_then(|p| p.total)
                    .unwrap_or(0);
                let _ = tx.send(Event::SearchResultsLoaded {
                    query,
                    matches: data.messages.matches,
                    total,
                });
            }
            Err(e) => {
                error!("Search failed: {}", e);
                let _ = tx.send(Event::ApiError(format!("Search: {}", e)));
            }
        }
    });
}

fn spawn_add_reaction<C: SlackApi>(
    client: &C,
    channel_id: &str,
    ts: &str,
    emoji: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let ts = ts.to_string();
    let emoji = emoji.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = client.reactions_add(&channel_id, &ts, &emoji).await {
            error!("Failed to add reaction: {}", e);
            let _ = tx.send(Event::ApiError(format!("Reaction: {}", e)));
        }
    });
}

fn trigger_image_downloads<C: SlackApi>(
    state: &mut AppState,
    channel_id: &str,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    if let Some(msgs) = state.channel_data.get(channel_id).map(|cd| &cd.messages) {
        let urls: Vec<String> = msgs
            .iter()
            .flat_map(|msg| {
                msg.files.iter().filter_map(|f| {
                    if f.is_image() {
                        f.best_thumb_url().map(|u| u.to_string())
                    } else {
                        None
                    }
                })
            })
            .filter(|url| {
                !state.image_cache.contains_key(url) && !state.pending_images.contains(url)
            })
            .collect();

        for url in urls {
            state.pending_images.insert(url.clone());
            spawn_download_image(client, &url, event_tx);
        }
    }
}

pub fn process_emoji_load_queue<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let queue: Vec<String> = state.emoji_load_queue.drain(..).collect();
    for key in queue {
        if state.custom_emoji_images.contains_key(&key) || state.pending_emoji_images.contains(&key) {
            continue;
        }
        if let Some(img) = load_emoji_from_disk(&state.team_id, &key) {
            state.custom_emoji_images.insert(key, img);
            state.dirty = true;
            continue;
        }
        let url = match state.custom_emoji.get(&key) {
            Some(url) if !url.starts_with("alias:") => url.clone(),
            _ => continue,
        };
        state.pending_emoji_images.insert(key.clone());
        spawn_download_emoji_image(client, &key, &url, event_tx);
    }
}

pub fn process_avatar_load_queue<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let queue: Vec<String> = state.avatar_load_queue.drain(..).collect();
    for user_id in queue {
        if state.avatar_images.contains_key(&user_id)
            || state.pending_avatar_images.contains(&user_id)
        {
            continue;
        }
        if let Some(img) = load_avatar_from_disk(&state.team_id, &user_id) {
            state.avatar_images.insert(user_id, img);
            state.dirty = true;
            continue;
        }
        let url = match state.avatar_url(&user_id) {
            Some(url) => url.to_string(),
            None => continue,
        };
        state.pending_avatar_images.insert(user_id.clone());
        spawn_download_avatar(client, &user_id, &url, event_tx);
    }
}

fn avatar_cache_dir(team_id: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(format!(
        "{}/.cache/slackslack/{}/avatars",
        home, team_id
    ))
}

fn save_avatar_to_disk(team_id: &str, user_id: &str, png_data: &[u8]) {
    if team_id.is_empty() {
        return;
    }
    let dir = avatar_cache_dir(team_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("Failed to create avatar cache dir: {}", e);
        return;
    }
    let path = dir.join(format!("{}.png", user_id));
    if let Err(e) = std::fs::write(&path, png_data) {
        error!("Failed to write avatar cache {}: {}", path.display(), e);
    }
}

fn load_avatar_from_disk(team_id: &str, user_id: &str) -> Option<crate::state::CachedImage> {
    if team_id.is_empty() {
        return None;
    }
    let path = avatar_cache_dir(team_id).join(format!("{}.png", user_id));
    let data = std::fs::read(&path).ok()?;
    let (width, height) = read_png_dimensions(&data)?;
    Some(crate::state::CachedImage {
        png_data: data,
        width,
        height,
    })
}

fn spawn_download_avatar<C: SlackApi>(
    client: &C,
    user_id: &str,
    url: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let user_id = user_id.to_string();
    let url = url.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client.download_file(&url).await {
            Ok(data) => {
                if let Some((png_data, width, height)) = crate::ui::images::encode_as_png(&data) {
                    let _ = tx.send(Event::AvatarImageLoaded {
                        user_id,
                        png_data,
                        width,
                        height,
                    });
                } else {
                    let _ = tx.send(Event::AvatarImageFailed { user_id });
                }
            }
            Err(e) => {
                error!("Failed to download avatar {}: {}", user_id, e);
                let _ = tx.send(Event::AvatarImageFailed { user_id });
            }
        }
    });
}

fn emoji_cache_dir(team_id: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(format!(
        "{}/.cache/slackslack/{}/emoji",
        home, team_id
    ))
}

fn save_emoji_to_disk(team_id: &str, name: &str, png_data: &[u8]) {
    if team_id.is_empty() {
        return;
    }
    let dir = emoji_cache_dir(team_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("Failed to create emoji cache dir: {}", e);
        return;
    }
    let path = dir.join(format!("{}.png", name));
    if let Err(e) = std::fs::write(&path, png_data) {
        error!("Failed to write emoji cache {}: {}", path.display(), e);
    }
}

fn load_emoji_from_disk(team_id: &str, name: &str) -> Option<crate::state::CachedImage> {
    if team_id.is_empty() {
        return None;
    }
    let path = emoji_cache_dir(team_id).join(format!("{}.png", name));
    let data = std::fs::read(&path).ok()?;
    let (width, height) = read_png_dimensions(&data)?;
    Some(crate::state::CachedImage {
        png_data: data,
        width,
        height,
    })
}

fn read_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((width, height))
}

fn spawn_download_emoji_image<C: SlackApi>(
    client: &C,
    name: &str,
    url: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let name = name.to_string();
    let url = url.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client.download_file(&url).await {
            Ok(data) => {
                if let Some((png_data, width, height)) = crate::ui::images::encode_as_png(&data) {
                    let _ = tx.send(Event::CustomEmojiImageLoaded {
                        name,
                        png_data,
                        width,
                        height,
                    });
                } else {
                    let _ = tx.send(Event::CustomEmojiImageFailed { name });
                }
            }
            Err(e) => {
                error!("Failed to download emoji {}: {}", name, e);
                let _ = tx.send(Event::CustomEmojiImageFailed { name });
            }
        }
    });
}

fn spawn_download_image<C: SlackApi>(
    client: &C,
    url: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let url = url.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client.download_file(&url).await {
            Ok(data) => {
                if let Some((png_data, width, height)) = crate::ui::images::encode_as_png(&data) {
                    let _ = tx.send(Event::ImageLoaded {
                        url,
                        png_data,
                        width,
                        height,
                    });
                } else {
                    error!("Failed to decode image: {}", url);
                }
            }
            Err(e) => {
                error!("Failed to download image {}: {}", url, e);
            }
        }
    });
}

pub enum HandleResult {
    Continue,
    Quit,
}
