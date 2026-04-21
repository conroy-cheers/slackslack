use crate::event::Event;
use crate::slack::client::SlackApi;
use crate::slack::types::{WsEvent, WsMessage};
use crate::state::{AppState, ChannelListEntry, Focus, InputMode};
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
        Event::FileUploaded { filename, .. } => {
            state.upload_status = Some(format!("Uploaded {}", filename));
            state.input_mode = InputMode::Insert;
            state.file_path_input.clear();
            state.file_path_cursor = 0;
            state.dirty = true;
            HandleResult::Continue
        }
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
        Event::StandardEmojiLoaded(emoji_map) => {
            state.standard_emoji = emoji_map;
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
        Event::EmojiPreviewImageLoaded { frames, frame_delays, width, height } => {
            state.emoji_preview_pending = false;
            state.emoji_preview_tick = 0;
            state.emoji_preview_frames = frames;
            state.emoji_preview_frame_delays = frame_delays;
            state.emoji_preview_tex_w = width;
            state.emoji_preview_tex_h = height;
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

    // Context menu
    if state.show_context_menu {
        return handle_context_menu_key(key, state, client, event_tx);
    }

    match state.input_mode {
        InputMode::Normal => handle_normal_key(key, state, client, event_tx),
        InputMode::Insert => handle_insert_key(key, state, client, event_tx),
        InputMode::Search => handle_search_key(key, state, client, event_tx),
        InputMode::MessageSearch => handle_message_search_key(key, state),
        InputMode::Reaction => handle_reaction_key(key, state, client, event_tx),
        InputMode::EmojiPicker => handle_emoji_picker_key(key, state, client, event_tx),
        InputMode::EmojiPreview => handle_emoji_preview_key(key, state),
        InputMode::UserPicker => handle_user_picker_key(key, state),
        InputMode::GlobalSearch => handle_global_search_key(key, state, client, event_tx),
        InputMode::FilePath => handle_file_path_key(key, state, client, event_tx),
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
            if !state.channel_list_items.is_empty() {
                let last = state.channel_list_items.len() - 1;
                state.selected_visual_idx = last;
                state.sync_selected_channel_from_visual();
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            state.selected_visual_idx = 0;
            state.sync_selected_channel_from_visual();
        }
        KeyCode::PageDown => {
            let len = state.channel_list_items.len();
            if len > 0 {
                state.selected_visual_idx = (state.selected_visual_idx + 15).min(len - 1);
                if matches!(state.channel_list_items.get(state.selected_visual_idx), Some(ChannelListEntry::Spacer)) {
                    state.selected_visual_idx = (state.selected_visual_idx + 1).min(len - 1);
                }
                state.sync_selected_channel_from_visual();
            }
        }
        KeyCode::PageUp => {
            state.selected_visual_idx = state.selected_visual_idx.saturating_sub(15);
            if matches!(state.channel_list_items.get(state.selected_visual_idx), Some(ChannelListEntry::Spacer)) {
                state.selected_visual_idx = state.selected_visual_idx.saturating_sub(1);
            }
            state.sync_selected_channel_from_visual();
        }
        // Open channel / toggle section
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            let visual_entry = state.selected_visual_entry().cloned();
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
                    state.save_current_draft();
                    state.focus = Focus::Messages;
                    state.selected_message_idx = 0;
                    state.restore_draft_for_current();
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
                let reactions = state.selected_message_reactions();
                state.open_emoji_picker(crate::state::EmojiPickerSource::Reaction, reactions);
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
            switch_channel(state, client, event_tx, |s| s.channel_next_channel());
        }
        KeyCode::Char('[') => {
            state.close_thread();
            switch_channel(state, client, event_tx, |s| s.channel_prev_channel());
        }
        KeyCode::Char('}') => {
            state.save_current_draft();
            if state.next_unread_channel() {
                state.close_thread();
                state.restore_draft_for_current();
                state.selected_message_idx = 0;
                state.clear_message_search();
                ensure_history_loaded(state, client, event_tx);
            }
        }
        KeyCode::Char('{') => {
            state.save_current_draft();
            if state.prev_unread_channel() {
                state.close_thread();
                state.restore_draft_for_current();
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
        // Context menu
        KeyCode::Char(' ') => {
            if state.selected_message().is_some() {
                state.show_context_menu = true;
                state.context_menu_selected = 0;
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
        // Reaction via emoji picker (on thread parent)
        KeyCode::Char('r') => {
            if state.thread_parent_ts.is_some() && state.thread_channel_id.is_some() {
                let reactions = state.selected_message_reactions();
                state.open_emoji_picker(crate::state::EmojiPickerSource::Reaction, reactions);
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
            switch_channel(state, client, event_tx, |s| s.channel_next());
        }
        KeyCode::Char('[') => {
            state.close_thread();
            switch_channel(state, client, event_tx, |s| s.channel_prev());
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

    // Alt modifiers for word-level navigation
    if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('f') => word_forward(state),
            KeyCode::Char('b') => word_backward(state),
            KeyCode::Char('d') => delete_word_forward(state),
            KeyCode::Char('x') => {
                return open_external_editor(state);
            }
            _ => {}
        }
        return HandleResult::Continue;
    }

    // Ctrl modifiers for input editing
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('w') => delete_word_backward(state),
            KeyCode::Char('u') => {
                let byte_pos = state.input_cursor_byte_offset();
                let killed: String = state.input_text.drain(..byte_pos).collect();
                state.push_kill_ring(killed);
                state.input_cursor = 0;
            }
            KeyCode::Char('k') => {
                let byte_pos = state.input_cursor_byte_offset();
                let killed = state.input_text[byte_pos..].to_string();
                state.input_text.truncate(byte_pos);
                state.push_kill_ring(killed);
            }
            KeyCode::Char('y') => {
                if let Some(text) = state.kill_ring.back().cloned() {
                    let byte_pos = state.input_cursor_byte_offset();
                    state.input_text.insert_str(byte_pos, &text);
                    state.input_cursor += text.chars().count();
                }
            }
            KeyCode::Char('a') => {
                state.input_cursor = 0;
            }
            KeyCode::Char('e') => {
                if state.input_cursor == state.input_char_count() {
                    state.open_emoji_picker(crate::state::EmojiPickerSource::Insert, vec![]);
                    return HandleResult::Continue;
                }
                state.input_cursor = state.input_char_count();
            }
            KeyCode::Char('o') => {
                state.input_mode = InputMode::FilePath;
                state.file_path_input.clear();
                state.file_path_cursor = 0;
                state.upload_status = None;
                return HandleResult::Continue;
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
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            let byte_pos = state.input_cursor_byte_offset();
            state.input_text.insert(byte_pos, '\n');
            state.input_cursor += 1;
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
                let byte_pos = state.input_cursor_byte_offset();
                let ch = state.input_text[byte_pos..].chars().next().unwrap();
                state.input_text.drain(byte_pos..byte_pos + ch.len_utf8());
            }
        }
        KeyCode::Delete => {
            if state.input_cursor < state.input_char_count() {
                let byte_pos = state.input_cursor_byte_offset();
                let ch = state.input_text[byte_pos..].chars().next().unwrap();
                state.input_text.drain(byte_pos..byte_pos + ch.len_utf8());
            }
        }
        KeyCode::Left => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
            }
        }
        KeyCode::Right => {
            if state.input_cursor < state.input_char_count() {
                state.input_cursor += 1;
            }
        }
        KeyCode::Up => state.input_history_prev(),
        KeyCode::Down => state.input_history_next(),
        KeyCode::Home => state.input_cursor = 0,
        KeyCode::End => state.input_cursor = state.input_char_count(),
        KeyCode::Tab => {
            if state.thread_channel_id.is_some() {
                state.reply_to_thread = !state.reply_to_thread;
            }
        }
        KeyCode::Char('@') => {
            state.open_user_picker();
            return HandleResult::Continue;
        }
        KeyCode::Char(':') => {
            let byte_pos = state.input_cursor_byte_offset();
            state.input_text.insert(byte_pos, ':');
            state.input_cursor += 1;
            if !cursor_in_code_span(&state.input_text, state.input_cursor) {
                let colon_pos = state.input_cursor - 1;
                state.open_emoji_picker(crate::state::EmojiPickerSource::Insert, vec![]);
                state.emoji_picker_inline_colon_pos = Some(colon_pos);
            }
        }
        KeyCode::Char(c) => {
            let byte_pos = state.input_cursor_byte_offset();
            state.input_text.insert(byte_pos, c);
            state.input_cursor += 1;
        }
        _ => {}
    }
    HandleResult::Continue
}

fn cursor_in_code_span(text: &str, char_cursor: usize) -> bool {
    let chars: Vec<char> = text.chars().take(char_cursor).collect();
    let backtick_count = chars.iter().filter(|&&c| c == '`').count();
    backtick_count % 2 != 0
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
            state.save_current_draft();
            state.input_mode = InputMode::Normal;
            state.channel_filter.clear();
            state.channel_filter_active = false;
            state.focus = Focus::Messages;
            state.selected_message_idx = 0;
            state.restore_draft_for_current();
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
    let inline = state.emoji_picker_inline_colon_pos;
    match key.code {
        KeyCode::Esc => {
            if inline.is_some() {
                state.emoji_picker_inline_colon_pos = None;
                state.input_mode = InputMode::Insert;
                state.focus = Focus::Input;
            } else {
                state.input_mode = InputMode::Normal;
            }
        }
        KeyCode::Enter => {
            if let Some((name, _display, _is_custom)) = state
                .emoji_picker_results
                .get(state.emoji_picker_selected)
                .cloned()
            {
                match state.emoji_picker_source {
                    crate::state::EmojiPickerSource::Reaction => {
                        let should_remove = state.emoji_picker_message_reactions
                            .iter()
                            .any(|(n, user_reacted)| n == &name && *user_reacted);
                        if state.focus == Focus::Thread {
                            if let (Some(channel_id), Some(ts)) = (
                                state.thread_parent_ts.clone().and_then(|_| state.thread_channel_id.clone()),
                                state.thread_parent_ts.clone(),
                            ) {
                                if should_remove {
                                    spawn_remove_reaction(client, &channel_id, &ts, &name, event_tx);
                                } else {
                                    spawn_add_reaction(client, &channel_id, &ts, &name, event_tx);
                                }
                            }
                        } else if let Some(msg) = state.selected_message() {
                            let ts = msg.ts.clone();
                            if let Some(channel_id) = state.active_channel_id() {
                                if should_remove {
                                    spawn_remove_reaction(client, &channel_id, &ts, &name, event_tx);
                                } else {
                                    spawn_add_reaction(client, &channel_id, &ts, &name, event_tx);
                                }
                            }
                        }
                        state.input_mode = InputMode::Normal;
                    }
                    crate::state::EmojiPickerSource::Insert => {
                        if let Some(colon_pos) = inline {
                            // Replace `:query` with `:name:` in input text
                            let remove_len = 1 + state.emoji_picker_query.chars().count(); // ':' + query
                            let start_byte: usize = state.input_text.char_indices()
                                .nth(colon_pos).map(|(b, _)| b).unwrap_or(state.input_text.len());
                            let end_byte: usize = state.input_text.char_indices()
                                .nth(colon_pos + remove_len).map(|(b, _)| b).unwrap_or(state.input_text.len());
                            let insert = format!(":{}:", name);
                            state.input_text.replace_range(start_byte..end_byte, &insert);
                            state.input_cursor = colon_pos + insert.chars().count();
                            state.emoji_picker_inline_colon_pos = None;
                        } else {
                            let insert = format!(":{}:", name);
                            let byte_pos = state.input_cursor_byte_offset();
                            state.input_text.insert_str(byte_pos, &insert);
                            state.input_cursor += insert.chars().count();
                        }
                        state.input_mode = InputMode::Insert;
                        state.focus = Focus::Input;
                    }
                }
            } else {
                if inline.is_some() {
                    state.emoji_picker_inline_colon_pos = None;
                    state.input_mode = InputMode::Insert;
                    state.focus = Focus::Input;
                } else {
                    state.input_mode = InputMode::Normal;
                }
            }
        }
        KeyCode::Backspace => {
            if inline.is_some() && state.emoji_picker_query.is_empty() {
                // Remove the ':' from input and close picker
                let colon_pos = inline.unwrap();
                let byte_pos: usize = state.input_text.char_indices()
                    .nth(colon_pos).map(|(b, _)| b).unwrap_or(state.input_text.len());
                if byte_pos < state.input_text.len() {
                    state.input_text.remove(byte_pos);
                }
                state.input_cursor = colon_pos;
                state.emoji_picker_inline_colon_pos = None;
                state.input_mode = InputMode::Insert;
                state.focus = Focus::Input;
                return HandleResult::Continue;
            }
            if let Some(colon_pos) = inline {
                // Also remove the character from input_text
                state.emoji_picker_query.pop();
                let query_len = state.emoji_picker_query.chars().count();
                let char_pos = colon_pos + 1 + query_len;
                let byte_pos: usize = state.input_text.char_indices()
                    .nth(char_pos).map(|(b, _)| b).unwrap_or(state.input_text.len());
                if byte_pos < state.input_text.len() {
                    state.input_text.remove(byte_pos);
                }
                state.input_cursor = colon_pos + 1 + query_len;
            } else {
                state.emoji_picker_query.pop();
            }
            state.filter_emoji_picker();
        }
        KeyCode::Char('j') | KeyCode::Down if inline.is_none() && !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.emoji_picker_results.is_empty() {
                state.emoji_picker_selected =
                    (state.emoji_picker_selected + 1) % state.emoji_picker_results.len();
            }
        }
        KeyCode::Char('k') | KeyCode::Up if inline.is_none() && !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.emoji_picker_results.is_empty() {
                if state.emoji_picker_selected == 0 {
                    state.emoji_picker_selected = state.emoji_picker_results.len() - 1;
                } else {
                    state.emoji_picker_selected -= 1;
                }
            }
        }
        KeyCode::Down => {
            if !state.emoji_picker_results.is_empty() {
                state.emoji_picker_selected =
                    (state.emoji_picker_selected + 1) % state.emoji_picker_results.len();
            }
        }
        KeyCode::Up => {
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
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some((name, display, is_custom)) = state.emoji_picker_results.get(state.emoji_picker_selected).cloned() {
                state.emoji_preview_name = name.clone();
                state.emoji_preview_char = display.clone();
                state.emoji_preview_tick = 0;
                state.emoji_preview_frames.clear();
                state.emoji_preview_frame_delays.clear();
                state.emoji_preview_tex_w = 0;
                state.emoji_preview_tex_h = 0;
                state.emoji_preview_pending = false;
                state.input_mode = InputMode::EmojiPreview;

                if matches!(state.billboard_renderer, crate::ui::emoji_preview::BillboardRenderer::Cpu) {
                    match crate::ui::emoji_preview::gpu::GpuRenderer::try_new() {
                        Ok(gpu) => {
                            tracing::info!("GPU renderer initialized");
                            state.billboard_renderer = crate::ui::emoji_preview::BillboardRenderer::Gpu(gpu);
                        }
                        Err(e) => {
                            tracing::warn!("wgpu init failed, using CPU renderer: {}", e);
                        }
                    }
                }

                if is_custom {
                    if let Some(key) = state.resolve_emoji_key(&name) {
                        let has_cached = if let Some(cached) = state.custom_emoji_images.get(&key) {
                            if let Some((frames, delays, w, h)) = crate::ui::emoji_preview::decode_emoji_frames(&cached.png_data) {
                                state.emoji_preview_frames = frames;
                                state.emoji_preview_frame_delays = delays;
                                state.emoji_preview_tex_w = w;
                                state.emoji_preview_tex_h = h;
                            }
                            true
                        } else {
                            false
                        };
                        if !has_cached {
                            state.emoji_preview_pending = true;
                        }
                        // Always download the original — cached PNG is a static
                        // rasterization that drops animation frames.
                        if let Some(url) = state.custom_emoji.get(&key).cloned() {
                            if !url.starts_with("alias:") {
                                spawn_download_emoji_preview_url(&url, event_tx);
                            }
                        }
                    }
                } else {
                    let unified = display.chars()
                        .filter(|&c| c != '\u{fe0f}')
                        .map(|c| format!("{:x}", c as u32))
                        .collect::<Vec<_>>()
                        .join("-");
                    let url = format!(
                        "https://cdn.jsdelivr.net/gh/twitter/twemoji@latest/assets/72x72/{}.png",
                        unified
                    );
                    state.emoji_preview_pending = true;
                    spawn_download_emoji_preview_url(&url, event_tx);
                }
            }
        }
        KeyCode::Char(c) => {
            state.emoji_picker_query.push(c);
            if let Some(colon_pos) = inline {
                let insert_char_pos = colon_pos + 1 + state.emoji_picker_query.chars().count() - 1;
                let byte_pos: usize = state.input_text.char_indices()
                    .nth(insert_char_pos).map(|(b, _)| b).unwrap_or(state.input_text.len());
                state.input_text.insert(byte_pos, c);
                state.input_cursor = insert_char_pos + 1;
            }
            state.filter_emoji_picker();
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── Emoji 3D preview ──────────────────────────────────────────────────────

fn handle_emoji_preview_key(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            state.input_mode = InputMode::EmojiPicker;
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
            let byte_pos = state.input_cursor_byte_offset();
            state.input_text.insert(byte_pos, '@');
            state.input_cursor += 1;
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        KeyCode::Enter => {
            if let Some((user_id, display)) = state
                .user_picker_results
                .get(state.user_picker_selected)
                .cloned()
            {
                let insert = format!("<@{}|{}>", user_id, display);
                let byte_pos = state.input_cursor_byte_offset();
                state.input_text.insert_str(byte_pos, &insert);
                state.input_cursor += insert.chars().count();
            }
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        KeyCode::Backspace => {
            if state.user_picker_query.is_empty() {
                let byte_pos = state.input_cursor_byte_offset();
                state.input_text.insert(byte_pos, '@');
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
                        state.save_current_draft();
                        state.global_search_results.clear();
                        state.input_mode = InputMode::Normal;
                        if let Some(idx) =
                            state.channels.iter().position(|c| c.id == channel_id)
                        {
                            state.selected_channel_idx = idx;
                            state.sync_visual_from_selected_channel();
                            state.close_thread();
                            state.clear_message_search();
                            state.focus = Focus::Messages;
                            state.selected_message_idx = 0;
                            state.restore_draft_for_current();
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
                state.channel_prev_no_wrap();
                state.dirty = true;
            } else if state.thread_area.map_or(false, |r| contains(r, col, row)) {
                let old = state.thread_scroll_offset;
                state.thread_scroll_offset = (state.thread_scroll_offset + scroll_lines)
                    .min(state.thread_max_scroll_offset);
                if state.thread_scroll_offset != old {
                    state.dirty = true;
                }
            } else if contains(state.messages_area, col, row) {
                if state.messages_scroll_lines(-(scroll_lines as isize)) {
                    state.dirty = true;
                }
                maybe_load_older(state, client, event_tx);
            }
        }
        MouseEventKind::ScrollDown => {
            if contains(state.channel_list_area, col, row) {
                state.channel_next_no_wrap();
                state.dirty = true;
            } else if state.thread_area.map_or(false, |r| contains(r, col, row)) {
                let old = state.thread_scroll_offset;
                state.thread_scroll_offset = state.thread_scroll_offset.saturating_sub(scroll_lines);
                if state.thread_scroll_offset != old {
                    state.dirty = true;
                }
            } else if contains(state.messages_area, col, row) {
                if state.messages_scroll_lines(scroll_lines as isize) {
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
                    state.selected_visual_idx = visual_idx;
                    match entry {
                        ChannelListEntry::Channel(ch_idx) => {
                            state.save_current_draft();
                            state.selected_channel_idx = ch_idx;
                            state.selected_message_idx = 0;
                            state.focus = Focus::Messages;
                            state.input_mode = InputMode::Normal;
                            state.restore_draft_for_current();
                            ensure_history_loaded(state, client, event_tx);
                        }
                        ChannelListEntry::SectionHeader(id) => {
                            state.toggle_section_collapse(&id);
                        }
                        ChannelListEntry::DmMore => {
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
        MouseEventKind::Down(crossterm::event::MouseButton::Right) => {
            if contains(state.messages_area, col, row) {
                // Right-click on messages: select the message and open context menu
                if let Some(info) = &state.messages_render_info {
                    let click_row = (row - info.inner_y) as usize;
                    let vline = info.scroll_y + click_row;
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
                            state.show_context_menu = true;
                            state.context_menu_selected = 0;
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

fn switch_channel<C: SlackApi>(
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
    switch: impl FnOnce(&mut AppState),
) {
    state.save_current_draft();
    switch(state);
    state.restore_draft_for_current();
    state.selected_message_idx = 0;
    state.messages_scroll_override = None;
    state.clear_message_search();
    ensure_history_loaded(state, client, event_tx);
}

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

fn char_pos_to_byte(text: &str, pos: usize) -> usize {
    text.char_indices()
        .nth(pos)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn find_word_boundary_backward(chars: &[char], from: usize) -> usize {
    let mut pos = from;
    while pos > 0 && chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    while pos > 0 && !chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    pos
}

fn find_word_boundary_forward(chars: &[char], from: usize) -> usize {
    let len = chars.len();
    let mut pos = from;
    while pos < len && !chars[pos].is_whitespace() {
        pos += 1;
    }
    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }
    pos
}

fn delete_word_backward(state: &mut AppState) {
    if state.input_cursor == 0 {
        return;
    }
    let chars: Vec<char> = state.input_text.chars().collect();
    let pos = find_word_boundary_backward(&chars, state.input_cursor);
    let byte_start = char_pos_to_byte(&state.input_text, pos);
    let byte_end = state.input_cursor_byte_offset();
    let killed: String = state.input_text.drain(byte_start..byte_end).collect();
    state.push_kill_ring(killed);
    state.input_cursor = pos;
}

fn word_forward(state: &mut AppState) {
    let chars: Vec<char> = state.input_text.chars().collect();
    state.input_cursor = find_word_boundary_forward(&chars, state.input_cursor);
}

fn word_backward(state: &mut AppState) {
    let chars: Vec<char> = state.input_text.chars().collect();
    state.input_cursor = find_word_boundary_backward(&chars, state.input_cursor);
}

fn delete_word_forward(state: &mut AppState) {
    let chars: Vec<char> = state.input_text.chars().collect();
    if state.input_cursor >= chars.len() {
        return;
    }
    let pos = find_word_boundary_forward(&chars, state.input_cursor);
    let byte_start = state.input_cursor_byte_offset();
    let byte_end = char_pos_to_byte(&state.input_text, pos);
    let killed: String = state.input_text.drain(byte_start..byte_end).collect();
    state.push_kill_ring(killed);
}

fn open_external_editor(state: &mut AppState) -> HandleResult {
    let tmp = std::env::temp_dir().join(format!("slackslack-{}.txt", std::process::id()));

    if std::fs::write(&tmp, &state.input_text).is_err() {
        state.last_error = Some("Failed to write temp file".into());
        return HandleResult::Continue;
    }

    HandleResult::SuspendForEditor(tmp)
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
        let formatted = crate::ui::messages::resolve_slack_markup_pub(&ws_msg.text, state);
        if formatted.chars().count() > 100 {
            let truncated: String = formatted.chars().take(97).collect();
            format!("{}...", truncated)
        } else {
            formatted
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
        let near_top = if state.messages_scroll_override.is_some() {
            state.messages_scroll_override
                .map(|s| s + 20 >= state.max_scroll_offset && state.max_scroll_offset > 0)
                .unwrap_or(false)
        } else {
            let max = state.message_count().saturating_sub(1);
            state.selected_message_idx >= max.saturating_sub(5)
        };

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

fn spawn_remove_reaction<C: SlackApi>(
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
        if let Err(e) = client.reactions_remove(&channel_id, &ts, &emoji).await {
            error!("Failed to remove reaction: {}", e);
            let _ = tx.send(Event::ApiError(format!("Reaction remove: {}", e)));
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

fn spawn_download_emoji_preview<C: SlackApi>(
    client: &C,
    url: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let url = url.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        let send_empty = |tx: &mpsc::UnboundedSender<Event>| {
            let _ = tx.send(Event::EmojiPreviewImageLoaded {
                frames: vec![],
                frame_delays: vec![],
                width: 0,
                height: 0,
            });
        };
        match client.download_file(&url).await {
            Ok(data) => {
                if let Some((frames, delays, w, h)) = crate::ui::emoji_preview::decode_emoji_frames(&data) {
                    let _ = tx.send(Event::EmojiPreviewImageLoaded {
                        frames,
                        frame_delays: delays,
                        width: w,
                        height: h,
                    });
                } else {
                    send_empty(&tx);
                }
            }
            Err(e) => {
                error!("Failed to download emoji preview {}: {}", url, e);
                send_empty(&tx);
            }
        }
    });
}

fn spawn_download_emoji_preview_url(
    url: &str,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let url = url.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        let result = async {
            let http = reqwest::Client::new();
            let resp = http.get(&url).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("HTTP {}", resp.status());
            }
            let data = resp.bytes().await?;
            Ok::<_, anyhow::Error>(data)
        }
        .await;
        match result {
            Ok(data) => {
                if let Some((frames, delays, w, h)) = crate::ui::emoji_preview::decode_emoji_frames(&data) {
                    let _ = tx.send(Event::EmojiPreviewImageLoaded {
                        frames,
                        frame_delays: delays,
                        width: w,
                        height: h,
                    });
                } else {
                    let _ = tx.send(Event::EmojiPreviewImageLoaded {
                        frames: vec![],
                        frame_delays: vec![],
                        width: 0,
                        height: 0,
                    });
                }
            }
            Err(e) => {
                error!("Failed to download emoji preview {}: {}", url, e);
                let _ = tx.send(Event::EmojiPreviewImageLoaded {
                    frames: vec![],
                    frame_delays: vec![],
                    width: 0,
                    height: 0,
                });
            }
        }
    });
}

// ── Context menu ─────────────────────────────────────────────────────────────

fn handle_context_menu_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    let item_count = crate::ui::context_menu::MENU_ITEMS.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => {
            state.show_context_menu = false;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            state.context_menu_selected = (state.context_menu_selected + 1) % item_count;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.context_menu_selected =
                (state.context_menu_selected + item_count - 1) % item_count;
        }
        KeyCode::Enter | KeyCode::Char('l') => {
            let selected = state.context_menu_selected;
            state.show_context_menu = false;
            return dispatch_context_menu_action(selected, state, client, event_tx);
        }
        KeyCode::Char('R') => {
            state.show_context_menu = false;
            return dispatch_context_menu_action(1, state, client, event_tx);
        }
        KeyCode::Char('r') => {
            state.show_context_menu = false;
            return dispatch_context_menu_action(2, state, client, event_tx);
        }
        KeyCode::Char('y') => {
            state.show_context_menu = false;
            return dispatch_context_menu_action(3, state, client, event_tx);
        }
        _ => {}
    }
    HandleResult::Continue
}

fn dispatch_context_menu_action<C: SlackApi>(
    action_idx: usize,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    match action_idx {
        0 => {
            // Open thread
            open_thread_for_selected(state, client, event_tx);
        }
        1 => {
            // Reply in thread
            open_thread_for_selected(state, client, event_tx);
            state.reply_to_thread = true;
            state.input_mode = InputMode::Insert;
            state.focus = Focus::Input;
        }
        2 => {
            // React with emoji
            if state.selected_message().is_some() {
                let reactions = state.selected_message_reactions();
                state.open_emoji_picker(crate::state::EmojiPickerSource::Reaction, reactions);
            }
        }
        3 => {
            // Copy message
            if let Some(msg) = state.selected_message() {
                let text = msg.text.clone();
                state.clipboard_pending = Some(text);
            }
        }
        _ => {}
    }
    HandleResult::Continue
}

// ── File path input mode ─────────────────────────────────────────────────────

fn handle_file_path_key<C: SlackApi>(
    key: crossterm::event::KeyEvent,
    state: &mut AppState,
    client: &C,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> HandleResult {
    state.dirty = true;
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Insert;
            state.file_path_input.clear();
            state.file_path_cursor = 0;
            state.upload_status = None;
        }
        KeyCode::Enter => {
            let raw_path = state.file_path_input.trim().to_string();
            if raw_path.is_empty() {
                state.input_mode = InputMode::Insert;
                state.upload_status = None;
                return HandleResult::Continue;
            }
            let expanded = shellexpand::tilde(&raw_path).to_string();
            let path = std::path::PathBuf::from(&expanded);
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    state.upload_status = Some(format!("Read error: {}", e));
                    return HandleResult::Continue;
                }
            };
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());

            let thread_ts = if state.reply_to_thread {
                state.thread_parent_ts.clone()
            } else {
                None
            };

            if let Some(channel_id) = state.active_channel_id() {
                state.upload_status = Some(format!("Uploading {}...", filename));
                spawn_file_upload(
                    client,
                    &channel_id,
                    thread_ts.as_deref(),
                    &filename,
                    data,
                    event_tx,
                );
            }
        }
        KeyCode::Backspace => {
            if state.file_path_cursor > 0 {
                state.file_path_cursor -= 1;
                let byte_pos: usize = state.file_path_input.char_indices()
                    .nth(state.file_path_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(state.file_path_input.len());
                let ch = state.file_path_input[byte_pos..].chars().next().unwrap();
                state.file_path_input.drain(byte_pos..byte_pos + ch.len_utf8());
            }
        }
        KeyCode::Left => {
            if state.file_path_cursor > 0 {
                state.file_path_cursor -= 1;
            }
        }
        KeyCode::Right => {
            let char_count = state.file_path_input.chars().count();
            if state.file_path_cursor < char_count {
                state.file_path_cursor += 1;
            }
        }
        KeyCode::Home => state.file_path_cursor = 0,
        KeyCode::End => state.file_path_cursor = state.file_path_input.chars().count(),
        KeyCode::Char(c) => {
            let byte_pos: usize = state.file_path_input.char_indices()
                .nth(state.file_path_cursor)
                .map(|(i, _)| i)
                .unwrap_or(state.file_path_input.len());
            state.file_path_input.insert(byte_pos, c);
            state.file_path_cursor += 1;
            state.upload_status = None;
        }
        _ => {}
    }
    HandleResult::Continue
}

fn spawn_file_upload<C: SlackApi>(
    client: &C,
    channel_id: &str,
    thread_ts: Option<&str>,
    filename: &str,
    data: Vec<u8>,
    event_tx: &mpsc::UnboundedSender<Event>,
) {
    let client = client.clone();
    let channel_id = channel_id.to_string();
    let thread_ts = thread_ts.map(|s| s.to_string());
    let filename = filename.to_string();
    let tx = event_tx.clone();
    tokio::spawn(async move {
        match client
            .files_upload(&channel_id, thread_ts.as_deref(), &filename, data)
            .await
        {
            Ok(_) => {
                let _ = tx.send(Event::FileUploaded {
                    channel_id,
                    filename,
                });
            }
            Err(e) => {
                error!("Failed to upload file: {}", e);
                let _ = tx.send(Event::ApiError(format!("Upload: {}", e)));
            }
        }
    });
}

pub enum HandleResult {
    Continue,
    Quit,
    SuspendForEditor(std::path::PathBuf),
}
