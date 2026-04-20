use crate::event::handler::{HandleResult, handle_event};
use crate::event::Event;
use crate::slack::types::{Channel, Message, User, UserProfile};
use crate::state::{AppState, Focus, InputMode};
use crate::testing::mock_client::{ApiCall, MockSlackClient};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tokio::sync::mpsc;

pub struct TestHarness {
    pub state: AppState,
    pub client: MockSlackClient,
    pub event_tx: mpsc::UnboundedSender<Event>,
    event_rx: mpsc::UnboundedReceiver<Event>,
}

impl TestHarness {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            state: AppState::new(),
            client: MockSlackClient::new(),
            event_tx,
            event_rx,
        }
    }

    // ── Workspace setup ────────────────────────────────────────────────

    pub fn add_channel(&mut self, id: &str, name: &str) -> &mut Self {
        self.state.channels.push(Channel {
            id: id.into(),
            name: Some(name.into()),
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
        });
        self
    }

    pub fn add_dm(&mut self, id: &str, user_id: &str) -> &mut Self {
        self.state.channels.push(Channel {
            id: id.into(),
            name: None,
            is_channel: false,
            is_im: true,
            is_mpim: false,
            is_private: false,
            is_member: true,
            user: Some(user_id.into()),
            topic: None,
            purpose: None,
            last_read: None,
            unread_count: 0,
            unread_count_display: 0,
        });
        self
    }

    pub fn add_user(&mut self, id: &str, name: &str, display_name: &str) -> &mut Self {
        self.state.user_cache.insert(
            id.into(),
            User {
                id: id.into(),
                name: name.into(),
                real_name: Some(display_name.into()),
                profile: Some(UserProfile {
                    display_name: Some(display_name.into()),
                    real_name: Some(display_name.into()),
                    image_48: None,
                }),
                is_bot: false,
                deleted: false,
                color: None,
            },
        );
        self
    }

    pub fn set_self_user(&mut self, user_id: &str) -> &mut Self {
        self.state.self_user_id = user_id.into();
        self
    }

    pub fn add_messages(&mut self, channel_id: &str, messages: Vec<Message>) -> &mut Self {
        let cd = self.state.channel_data.entry(channel_id.into())
            .or_insert_with(crate::state::ChannelData::new);
        cd.messages = messages.into();
        self
    }

    // ── Input simulation ───────────────────────────────────────────────

    pub fn send_event(&mut self, event: Event) -> HandleResult {
        handle_event(event, &mut self.state, &self.client, &self.event_tx)
    }

    pub fn press_key(&mut self, code: KeyCode) -> HandleResult {
        self.send_event(Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    pub fn press_ctrl(&mut self, c: char) -> HandleResult {
        self.send_event(Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    pub fn press_alt(&mut self, c: char) -> HandleResult {
        self.send_event(Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::ALT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    pub fn press_char(&mut self, c: char) -> HandleResult {
        self.press_key(KeyCode::Char(c))
    }

    pub fn press_enter(&mut self) -> HandleResult {
        self.press_key(KeyCode::Enter)
    }

    pub fn press_esc(&mut self) -> HandleResult {
        self.press_key(KeyCode::Esc)
    }

    pub fn press_tab(&mut self) -> HandleResult {
        self.press_key(KeyCode::Tab)
    }

    pub fn press_backtab(&mut self) -> HandleResult {
        self.press_key(KeyCode::BackTab)
    }

    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press_char(c);
        }
    }

    pub fn scroll_up_in(&mut self, area: ratatui::layout::Rect) -> HandleResult {
        self.send_event(Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollUp,
            column: area.x + 1,
            row: area.y + 1,
            modifiers: KeyModifiers::NONE,
        }))
    }

    pub fn scroll_down_in(&mut self, area: ratatui::layout::Rect) -> HandleResult {
        self.send_event(Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: area.x + 1,
            row: area.y + 1,
            modifiers: KeyModifiers::NONE,
        }))
    }

    // ── Event processing ───────────────────────────────────────────────

    pub fn drain_spawned_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            handle_event(event, &mut self.state, &self.client, &self.event_tx);
        }
    }

    pub async fn yield_to_spawned_tasks(&mut self) {
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        self.drain_spawned_events();
    }

    // ── State queries ──────────────────────────────────────────────────

    pub fn focus(&self) -> Focus {
        self.state.focus
    }

    pub fn mode(&self) -> InputMode {
        self.state.input_mode
    }

    pub fn active_channel_id(&self) -> Option<String> {
        self.state.active_channel_id()
    }

    pub fn active_channel_name(&self) -> Option<String> {
        self.state.active_channel().map(|c| c.display_name().to_string())
    }

    pub fn selected_channel_idx(&self) -> usize {
        self.state.selected_channel_idx
    }

    pub fn selected_message_idx(&self) -> usize {
        self.state.selected_message_idx
    }

    pub fn selected_message_text(&self) -> Option<String> {
        self.state.selected_message().map(|m| m.text.clone())
    }

    pub fn input_text(&self) -> &str {
        &self.state.input_text
    }

    pub fn thread_open(&self) -> bool {
        self.state.thread_channel_id.is_some()
    }

    pub fn thread_channel_id(&self) -> Option<&str> {
        self.state.thread_channel_id.as_deref()
    }

    pub fn thread_parent_ts(&self) -> Option<&str> {
        self.state.thread_parent_ts.as_deref()
    }

    pub fn thread_message_count(&self) -> usize {
        self.state.thread_message_count()
    }

    pub fn reply_to_thread(&self) -> bool {
        self.state.reply_to_thread
    }

    pub fn message_count(&self) -> usize {
        self.state.message_count()
    }

    pub fn is_connected(&self) -> bool {
        self.state.connected
    }

    pub fn last_error(&self) -> Option<&str> {
        self.state.last_error.as_deref()
    }

    pub fn channel_filter_active(&self) -> bool {
        self.state.channel_filter_active
    }

    pub fn channel_filter(&self) -> &str {
        &self.state.channel_filter
    }

    pub fn api_calls(&self) -> Vec<ApiCall> {
        self.client.take_calls()
    }

    // ── Assertions ─────────────────────────────────────────────────────

    pub fn assert_focus(&self, expected: Focus) {
        assert_eq!(
            self.focus(),
            expected,
            "expected focus {:?}, got {:?}",
            expected,
            self.focus()
        );
    }

    pub fn assert_mode(&self, expected: InputMode) {
        assert_eq!(
            self.mode(),
            expected,
            "expected mode {:?}, got {:?}",
            expected,
            self.mode()
        );
    }

    pub fn assert_active_channel(&self, channel_id: &str) {
        assert_eq!(
            self.active_channel_id().as_deref(),
            Some(channel_id),
            "expected active channel {}, got {:?}",
            channel_id,
            self.active_channel_id()
        );
    }

    pub fn assert_active_channel_name(&self, name: &str) {
        assert_eq!(
            self.active_channel_name().as_deref(),
            Some(name),
            "expected active channel name '{}', got {:?}",
            name,
            self.active_channel_name()
        );
    }

    pub fn assert_thread_open(&self, channel_id: &str, parent_ts: &str) {
        assert!(self.thread_open(), "expected thread to be open");
        assert_eq!(
            self.thread_channel_id(),
            Some(channel_id),
            "expected thread channel {}, got {:?}",
            channel_id,
            self.thread_channel_id()
        );
        assert_eq!(
            self.thread_parent_ts(),
            Some(parent_ts),
            "expected thread parent ts {}, got {:?}",
            parent_ts,
            self.thread_parent_ts()
        );
    }

    pub fn assert_thread_closed(&self) {
        assert!(
            !self.thread_open(),
            "expected thread to be closed, but it's open on {:?}",
            self.thread_parent_ts()
        );
    }

    pub fn assert_selected_message(&self, expected_text: &str) {
        assert_eq!(
            self.selected_message_text().as_deref(),
            Some(expected_text),
            "expected selected message '{}', got {:?}",
            expected_text,
            self.selected_message_text()
        );
    }

    pub fn assert_input_empty(&self) {
        assert!(
            self.input_text().is_empty(),
            "expected empty input, got '{}'",
            self.input_text()
        );
    }

    pub fn assert_input_text(&self, expected: &str) {
        assert_eq!(
            self.input_text(),
            expected,
            "expected input '{}', got '{}'",
            expected,
            self.input_text()
        );
    }
}

// ── Test data builders ─────────────────────────────────────────────────

pub fn msg(text: &str, ts: &str) -> Message {
    Message {
        user: Some("U_TEST".into()),
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

pub fn msg_from(user: &str, text: &str, ts: &str) -> Message {
    Message {
        user: Some(user.into()),
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

pub fn thread_msg(text: &str, ts: &str, thread_ts: &str) -> Message {
    Message {
        user: Some("U_TEST".into()),
        text: text.into(),
        ts: ts.into(),
        thread_ts: Some(thread_ts.into()),
        reply_count: None,
        reactions: Vec::new(),
        edited: None,
        subtype: None,
        bot_id: None,
        username: None,
        files: Vec::new(),
    }
}
