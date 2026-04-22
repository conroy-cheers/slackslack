use super::harness::*;
use super::mock_client::ApiCall;
use crate::event::Event;
use crate::slack::types::{Reaction, WsEvent, WsMessage};
use crate::state::{Focus, InputMode};
use crossterm::event::KeyCode;

fn ws_msg(channel: &str, user: &str, text: &str, ts: &str, thread_ts: Option<&str>) -> Event {
    Event::SlackWsEvent(WsEvent::Message(WsMessage {
        channel: Some(channel.into()),
        user: Some(user.into()),
        text: text.into(),
        ts: ts.into(),
        thread_ts: thread_ts.map(|s| s.into()),
        subtype: None,
        message: None,
        previous_message: None,
    }))
}

fn setup_workspace() -> TestHarness {
    let mut h = TestHarness::new();
    h.set_self_user("U_ME");
    h.add_user("U_ME", "me", "Me");
    h.add_user("U_ALICE", "alice", "Alice");
    h.add_user("U_BOB", "bob", "Bob");
    h.add_channel("C_GEN", "general");
    h.add_channel("C_RAND", "random");
    h.add_dm("D_ALICE", "U_ALICE");
    h.add_messages(
        "C_GEN",
        vec![
            msg("oldest message", "1000.000"),
            msg("middle message", "1001.000"),
            msg("newest message", "1002.000"),
        ],
    );
    h.add_messages(
        "C_RAND",
        vec![
            msg("random old", "2000.000"),
            msg("random new", "2001.000"),
        ],
    );
    h.add_messages(
        "D_ALICE",
        vec![
            msg_from("U_ALICE", "hey there", "3000.000"),
            msg_from("U_ME", "hi alice", "3001.000"),
        ],
    );
    // Pre-populate mock for thread replies
    h.client.add_thread_replies(
        "C_GEN",
        "1001.000",
        vec![
            msg("middle message", "1001.000"),
            thread_msg("reply one", "1001.001", "1001.000"),
            thread_msg("reply two", "1001.002", "1001.000"),
        ],
    );
    h
}

// ── Focus & navigation ─────────────────────────────────────────────────

#[tokio::test]
async fn initial_state_is_channel_list_normal() {
    let h = setup_workspace();
    h.assert_focus(Focus::ChannelList);
    h.assert_mode(InputMode::Normal);
    h.assert_active_channel_name("general");
}

#[tokio::test]
async fn enter_opens_messages_pane() {
    let mut h = setup_workspace();
    h.press_enter();
    h.assert_focus(Focus::Messages);
    h.assert_active_channel("C_GEN");
}

#[tokio::test]
async fn l_key_opens_messages_from_channel_list() {
    let mut h = setup_workspace();
    h.press_char('l');
    h.assert_focus(Focus::Messages);
}

#[tokio::test]
async fn h_key_returns_to_channel_list_from_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('h');
    h.assert_focus(Focus::ChannelList);
}

#[tokio::test]
async fn esc_returns_to_channel_list_from_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_esc();
    h.assert_focus(Focus::ChannelList);
}

#[tokio::test]
async fn j_k_navigate_channels() {
    let mut h = setup_workspace();
    h.assert_active_channel("C_GEN");
    h.press_char('j');
    h.assert_active_channel("C_RAND");
    h.press_char('j');
    h.assert_active_channel("D_ALICE");
    h.press_char('k');
    h.assert_active_channel("C_RAND");
}

#[tokio::test]
async fn tab_cycles_focus_from_channel_list_to_messages() {
    let mut h = setup_workspace();
    h.press_tab();
    h.assert_focus(Focus::Messages);
}

#[tokio::test]
async fn tab_from_messages_goes_to_channel_list_when_no_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.assert_thread_closed();
    h.press_tab();
    h.assert_focus(Focus::ChannelList);
}

#[tokio::test]
async fn tab_from_messages_goes_to_thread_when_thread_open() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    // Select middle message (idx 1 = second newest)
    h.press_char('k');
    h.press_enter(); // open thread
    h.assert_focus(Focus::Thread);
    // Tab from thread -> Messages
    h.press_tab();
    h.assert_focus(Focus::Messages);
    // Tab from messages -> Thread (because thread is open)
    h.press_tab();
    h.assert_focus(Focus::Thread);
}

#[tokio::test]
async fn backtab_always_goes_to_channel_list() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_backtab();
    h.assert_focus(Focus::ChannelList);
}

// ── Channel switching from messages ────────────────────────────────────

#[tokio::test]
async fn bracket_keys_switch_channels_from_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.assert_active_channel("C_GEN");
    h.press_char(']');
    h.assert_active_channel("C_RAND");
    h.assert_focus(Focus::Messages);
    h.press_char('[');
    h.assert_active_channel("C_GEN");
}

// ── Insert mode ────────────────────────────────────────────────────────

#[tokio::test]
async fn i_enters_insert_mode_from_channel_list() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
    assert!(!h.reply_to_thread());
}

#[tokio::test]
async fn i_enters_insert_mode_from_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('i');
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
    assert!(!h.reply_to_thread());
}

#[tokio::test]
async fn esc_from_insert_returns_to_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('i'); // -> Insert
    h.press_esc();
    h.assert_mode(InputMode::Normal);
    h.assert_focus(Focus::Messages);
}

#[tokio::test]
async fn esc_from_insert_returns_to_thread_when_thread_open() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle message
    h.press_enter(); // -> Thread
    h.press_char('i'); // -> Insert (replying to thread)
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
    assert!(h.reply_to_thread());
    h.press_esc();
    h.assert_mode(InputMode::Normal);
    h.assert_focus(Focus::Thread);
}

#[tokio::test]
async fn typing_text_in_insert_mode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello world");
    h.assert_input_text("hello world");
}

// ── Sending messages ───────────────────────────────────────────────────

#[tokio::test]
async fn send_message_clears_input_and_stays_in_insert() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("hello");
    h.press_enter(); // send
    h.assert_input_empty();
    // After sending, we stay in insert mode (input was cleared, not mode changed)
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
}

#[tokio::test]
async fn send_message_posts_to_active_channel() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("test msg");
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    match post {
        Some(ApiCall::PostMessage { channel, text, thread_ts }) => {
            assert_eq!(channel, "C_GEN");
            assert_eq!(text, "test msg");
            assert!(thread_ts.is_none());
        }
        _ => panic!("expected PostMessage call, got {:?}", calls),
    }
}

#[tokio::test]
async fn send_message_to_dm() {
    let mut h = setup_workspace();
    // Navigate to DM
    h.press_char('j'); // C_RAND
    h.press_char('j'); // D_ALICE
    h.assert_active_channel("D_ALICE");
    h.press_enter(); // -> Messages
    h.press_char('i');
    h.type_text("hey alice");
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    match post {
        Some(ApiCall::PostMessage { channel, text, .. }) => {
            assert_eq!(channel, "D_ALICE");
            assert_eq!(text, "hey alice");
        }
        _ => panic!("expected PostMessage to D_ALICE"),
    }
}

// ── Thread ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn enter_on_message_opens_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages (selected = newest, idx 0)
    h.press_char('k'); // select middle message (idx 1)
    h.assert_selected_message("middle message");
    h.press_enter(); // open thread
    h.assert_focus(Focus::Thread);
    h.assert_thread_open("C_GEN", "1001.000");
}

#[tokio::test]
async fn esc_from_thread_closes_it() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_enter(); // -> Thread
    h.assert_thread_open("C_GEN", "1001.000");
    h.press_esc(); // close thread
    h.assert_thread_closed();
    h.assert_focus(Focus::Messages);
}

#[tokio::test]
async fn h_from_thread_closes_it() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k');
    h.press_enter(); // -> Thread
    h.press_char('h');
    h.assert_thread_closed();
    h.assert_focus(Focus::Messages);
}

#[tokio::test]
async fn big_r_opens_thread_in_reply_mode() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle message
    h.press_char('R'); // reply in thread
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
    assert!(h.reply_to_thread());
    h.assert_thread_open("C_GEN", "1001.000");
}

#[tokio::test]
async fn thread_reply_posts_with_thread_ts() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_char('R'); // reply in thread
    h.type_text("thread reply");
    h.press_enter(); // send
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    match post {
        Some(ApiCall::PostMessage { channel, text, thread_ts }) => {
            assert_eq!(channel, "C_GEN");
            assert_eq!(text, "thread reply");
            assert_eq!(thread_ts.as_deref(), Some("1001.000"));
        }
        _ => panic!("expected PostMessage with thread_ts"),
    }
}

#[tokio::test]
async fn insert_from_thread_pane_replies_to_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k');
    h.press_enter(); // -> Thread
    h.assert_focus(Focus::Thread);
    h.press_char('i'); // insert from thread
    assert!(h.reply_to_thread());
}

#[tokio::test]
async fn channel_switch_from_thread_closes_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k');
    h.press_enter(); // -> Thread on C_GEN
    h.assert_thread_open("C_GEN", "1001.000");
    h.press_char(']'); // next channel
    h.assert_thread_closed();
    h.assert_active_channel("C_RAND");
}

// ── Message navigation ─────────────────────────────────────────────────

#[tokio::test]
async fn j_k_navigate_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.assert_selected_message("newest message"); // idx 0 = newest
    h.press_char('k'); // older
    h.assert_selected_message("middle message");
    h.press_char('k'); // older
    h.assert_selected_message("oldest message");
    h.press_char('j'); // newer
    h.assert_selected_message("middle message");
}

#[tokio::test]
async fn big_g_goes_to_newest_message() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k');
    h.press_char('k'); // at oldest
    h.press_char('G'); // jump to newest
    h.assert_selected_message("newest message");
}

#[tokio::test]
async fn small_g_goes_to_oldest_message() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('g'); // jump to oldest
    h.assert_selected_message("oldest message");
}

// ── Channel search ─────────────────────────────────────────────────────

#[tokio::test]
async fn slash_from_channel_list_starts_search() {
    let mut h = setup_workspace();
    h.press_char('/');
    h.assert_mode(InputMode::Search);
    assert!(h.channel_filter_active());
}

#[tokio::test]
async fn channel_search_filters_and_selects() {
    let mut h = setup_workspace();
    h.press_char('/');
    h.type_text("rand");
    assert_eq!(h.channel_filter(), "rand");
    h.assert_active_channel("C_RAND");
    h.press_enter(); // confirm
    h.assert_mode(InputMode::Normal);
    h.assert_focus(Focus::Messages);
    h.assert_active_channel("C_RAND");
}

#[tokio::test]
async fn channel_search_esc_cancels() {
    let mut h = setup_workspace();
    h.press_char('/');
    h.type_text("rand");
    h.press_esc();
    h.assert_mode(InputMode::Normal);
    assert!(!h.channel_filter_active());
}

// ── Message search ─────────────────────────────────────────────────────

#[tokio::test]
async fn slash_from_messages_starts_message_search() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('/');
    h.assert_mode(InputMode::MessageSearch);
}

#[tokio::test]
async fn message_search_finds_matching_message() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('/');
    h.type_text("oldest");
    h.press_enter(); // confirm search
    h.assert_mode(InputMode::Normal);
    h.assert_selected_message("oldest message");
}

// ── WebSocket events ───────────────────────────────────────────────────

#[tokio::test]
async fn ws_message_pushes_to_channel() {
    let mut h = setup_workspace();
    let initial_count = h.state.channel_data.get("C_GEN").map(|cd| cd.messages.len()).unwrap_or(0);
    h.send_event(Event::SlackWsEvent(WsEvent::Message(
        crate::slack::types::WsMessage {
            channel: Some("C_GEN".into()),
            user: Some("U_ALICE".into()),
            text: "ws hello".into(),
            ts: "1003.000".into(),
            thread_ts: None,
            subtype: None,
            message: None,
            previous_message: None,
        },
    )));
    let new_count = h.state.channel_data.get("C_GEN").map(|cd| cd.messages.len()).unwrap_or(0);
    assert_eq!(new_count, initial_count + 1);
}

#[tokio::test]
async fn slack_connected_event_sets_state() {
    let mut h = setup_workspace();
    assert!(!h.is_connected());
    h.send_event(Event::SlackConnected {
        self_id: "U_ME".into(),
        team: "TestTeam".into(),
    });
    assert!(h.is_connected());
}

#[tokio::test]
async fn api_error_sets_last_error() {
    let mut h = setup_workspace();
    h.send_event(Event::ApiError("something broke".into()));
    assert_eq!(h.last_error(), Some("something broke"));
}

// ── Help overlay ───────────────────────────────────────────────────────

#[tokio::test]
async fn question_mark_shows_help_any_key_dismisses() {
    let mut h = setup_workspace();
    h.press_char('?');
    assert!(h.state.show_help);
    h.press_char('x'); // any key dismisses
    assert!(!h.state.show_help);
}

// ── Reaction / emoji picker ────────────────────────────────────────────

#[tokio::test]
async fn r_opens_emoji_picker_on_message() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('r');
    h.assert_mode(InputMode::EmojiPicker);
}

#[tokio::test]
async fn emoji_picker_esc_cancels() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('r');
    h.press_esc();
    h.assert_mode(InputMode::Normal);
}

// ── Quit ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn q_from_channel_list_quits() {
    let mut h = setup_workspace();
    let result = h.press_char('q');
    assert!(matches!(result, crate::event::handler::HandleResult::Quit));
}

#[tokio::test]
async fn q_from_messages_quits() {
    let mut h = setup_workspace();
    h.press_enter();
    let result = h.press_char('q');
    assert!(matches!(result, crate::event::handler::HandleResult::Quit));
}

#[tokio::test]
async fn ctrl_c_always_quits() {
    let mut h = setup_workspace();
    h.press_char('i'); // insert mode
    let result = h.press_ctrl('c');
    assert!(matches!(result, crate::event::handler::HandleResult::Quit));
}

// ── Complex scenarios ──────────────────────────────────────────────────

#[tokio::test]
async fn send_channel_message_then_navigate_stays_in_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("hello");
    h.press_enter(); // send
    h.press_esc(); // back to normal/messages
    h.assert_focus(Focus::Messages);
    h.assert_active_channel("C_GEN");
    h.press_char('j'); // navigate messages (should not jump to another pane)
    h.assert_focus(Focus::Messages);
    h.assert_active_channel("C_GEN");
}

#[tokio::test]
async fn thread_reply_then_esc_returns_to_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_char('R'); // reply in thread
    h.type_text("my reply");
    h.press_enter(); // send
    // After sending, still in insert mode
    h.assert_mode(InputMode::Insert);
    h.press_esc(); // exit insert
    // Should return to thread, not messages
    h.assert_focus(Focus::Thread);
    h.assert_thread_open("C_GEN", "1001.000");
}

#[tokio::test]
async fn switch_channel_resets_message_selection() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k'); // select older message
    h.press_char('k'); // select oldest
    assert!(h.selected_message_idx() > 0);
    h.press_char(']'); // switch to C_RAND
    assert_eq!(h.selected_message_idx(), 0); // reset to newest
}

#[tokio::test]
async fn full_workflow_channel_to_thread_to_send_to_close() {
    let mut h = setup_workspace();
    // Start at channel list
    h.assert_focus(Focus::ChannelList);

    // Navigate to general and open messages
    h.press_enter();
    h.assert_focus(Focus::Messages);
    h.assert_active_channel("C_GEN");

    // Navigate to middle message and open thread
    h.press_char('k');
    h.assert_selected_message("middle message");
    h.press_enter();
    h.assert_focus(Focus::Thread);
    h.assert_thread_open("C_GEN", "1001.000");

    // Reply in thread
    h.press_char('i');
    h.assert_mode(InputMode::Insert);
    assert!(h.reply_to_thread());
    h.type_text("my reply");
    h.press_enter();
    h.assert_input_empty();

    // Exit insert, should be in thread
    h.press_esc();
    h.assert_focus(Focus::Thread);
    h.assert_mode(InputMode::Normal);

    // Close thread
    h.press_esc();
    h.assert_thread_closed();
    h.assert_focus(Focus::Messages);

    // Go back to channel list
    h.press_char('h');
    h.assert_focus(Focus::ChannelList);
}

#[tokio::test]
async fn dm_thread_workflow() {
    let mut h = setup_workspace();
    // Navigate to Alice DM
    h.press_char('j');
    h.press_char('j');
    h.assert_active_channel("D_ALICE");

    // Open messages
    h.press_enter();
    h.assert_focus(Focus::Messages);

    // Selected message should be newest in DM
    h.assert_selected_message("hi alice");

    // Reply in thread on the DM message
    h.press_char('R');
    h.assert_focus(Focus::Input);
    h.assert_mode(InputMode::Insert);
    assert!(h.reply_to_thread());
    h.assert_thread_open("D_ALICE", "3001.000");
}

// ── Thread reply display (regression tests) ────────────────────────────

#[tokio::test]
async fn ws_thread_reply_appears_in_open_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k'); // select middle message
    h.press_enter(); // open thread on 1001.000
    h.assert_thread_open("C_GEN", "1001.000");

    // Simulate the ThreadLoaded response arriving
    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![
            msg("middle message", "1001.000"),
            thread_msg("reply one", "1001.001", "1001.000"),
        ],
    });
    assert_eq!(h.thread_message_count(), 2);

    // Now a new WS message arrives in this thread
    h.send_event(ws_msg("C_GEN", "U_ALICE", "new reply via ws", "1001.003", Some("1001.000")));

    // The new reply must appear in thread_messages
    assert_eq!(
        h.thread_message_count(),
        3,
        "WS reply should be appended to open thread_messages"
    );
    let thread_msgs = h.state.thread_messages().unwrap();
    assert_eq!(thread_msgs.last().unwrap().text, "new reply via ws");
}

#[tokio::test]
async fn ws_thread_reply_for_different_thread_does_not_pollute_open_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k'); // select middle message
    h.press_enter(); // open thread on 1001.000
    h.assert_thread_open("C_GEN", "1001.000");

    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![
            msg("middle message", "1001.000"),
        ],
    });
    assert_eq!(h.thread_message_count(), 1);

    // WS reply for a DIFFERENT thread in the same channel
    h.send_event(ws_msg("C_GEN", "U_BOB", "other thread reply", "9999.001", Some("9999.000")));

    // Should NOT appear in the open thread
    assert_eq!(
        h.thread_message_count(),
        1,
        "reply to a different thread should not appear in the open thread"
    );
}

#[tokio::test]
async fn ws_thread_reply_for_different_channel_does_not_pollute_open_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k');
    h.press_enter(); // open thread on 1001.000 in C_GEN

    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![msg("middle message", "1001.000")],
    });

    // WS reply for a thread in C_RAND, not C_GEN
    h.send_event(ws_msg("C_RAND", "U_BOB", "wrong channel", "5000.001", Some("1001.000")));

    assert_eq!(
        h.thread_message_count(),
        1,
        "reply in a different channel should not appear in the open thread"
    );
}

#[tokio::test]
async fn ws_channel_message_without_thread_ts_does_not_go_to_thread() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('k');
    h.press_enter(); // open thread on 1001.000

    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![msg("middle message", "1001.000")],
    });

    // A regular (non-threaded) message in the same channel
    h.send_event(ws_msg("C_GEN", "U_ALICE", "top-level msg", "8000.000", None));

    assert_eq!(
        h.thread_message_count(),
        1,
        "top-level channel message should not appear in thread"
    );
}

#[tokio::test]
async fn send_thread_reply_appears_in_thread_after_ws_echo() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k'); // select middle
    h.press_char('R'); // open thread + insert mode

    // Let the spawned thread load complete
    h.yield_to_spawned_tasks().await;
    // The mock returns 3 messages for this thread (parent + 2 replies from setup)
    let baseline = h.thread_message_count();
    assert!(baseline > 0, "thread should have loaded messages");

    // Type and send a reply
    h.type_text("my new reply");
    h.press_enter();
    h.yield_to_spawned_tasks().await;

    // Simulate the WS echo of our own sent message
    h.send_event(ws_msg("C_GEN", "U_ME", "my new reply", "1001.005", Some("1001.000")));

    assert_eq!(
        h.thread_message_count(),
        baseline + 1,
        "sent reply should appear in thread after WS echo"
    );
    let thread_msgs = h.state.thread_messages().unwrap();
    assert_eq!(thread_msgs.last().unwrap().text, "my new reply");
}

// ── Thread reply should not appear in channel messages ─────────────────

#[tokio::test]
async fn ws_thread_reply_does_not_appear_in_channel_messages() {
    let mut h = setup_workspace();
    let initial_count = h.state.channel_data.get("C_GEN").unwrap().messages.len();

    // A thread reply arrives via WS (thread_ts != ts, so it's a reply not a parent)
    h.send_event(ws_msg("C_GEN", "U_ALICE", "thread only reply", "1001.003", Some("1001.000")));

    let cd = h.state.channel_data.get("C_GEN").unwrap();
    assert_eq!(
        cd.messages.len(),
        initial_count,
        "thread reply should NOT be added to channel messages list"
    );
    // But it should be in the thread
    let thread = cd.threads.get("1001.000").unwrap();
    assert!(
        thread.iter().any(|m| m.text == "thread only reply"),
        "thread reply should be in the thread's replies"
    );
}

// ── Reaction workflow ──────────────────────────────────────────────────

#[tokio::test]
async fn emoji_picker_confirm_sends_reaction() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('r'); // open emoji picker
    h.assert_mode(InputMode::EmojiPicker);
    // Type to search for an emoji, then confirm
    h.type_text("thumbsup");
    h.press_enter(); // confirm selection
    h.yield_to_spawned_tasks().await;
    // Should return to normal mode
    h.assert_mode(InputMode::Normal);
    // Should have sent a reaction API call
    let calls = h.api_calls();
    let reaction = calls.iter().find(|c| matches!(c, ApiCall::AddReaction { .. }));
    assert!(reaction.is_some(), "expected AddReaction call, got {:?}", calls);
}


// ── Input editing ──────────────────────────────────────────────────────

#[tokio::test]
async fn backspace_deletes_character() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello");
    h.press_key(KeyCode::Backspace);
    h.assert_input_text("hell");
}

#[tokio::test]
async fn cursor_movement_left_right() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("abc");
    h.press_key(KeyCode::Left);
    h.press_key(KeyCode::Left);
    // Cursor is now after 'a', typing inserts at cursor
    h.type_text("X");
    h.assert_input_text("aXbc");
}

#[tokio::test]
async fn home_end_keys() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello");
    h.press_key(KeyCode::Home);
    h.type_text("X");
    h.assert_input_text("Xhello");
    h.press_key(KeyCode::End);
    h.type_text("Y");
    h.assert_input_text("XhelloY");
}

#[tokio::test]
async fn empty_message_not_sent() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('i');
    h.press_enter(); // send with empty input
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    assert!(post.is_none(), "empty message should not be sent");
}

#[tokio::test]
async fn whitespace_only_message_not_sent() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    h.type_text("   ");
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    assert!(post.is_none(), "whitespace-only message should not be sent");
}

// ── Input history ──────────────────────────────────────────────────────

#[tokio::test]
async fn input_history_up_down() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    // Send two messages to populate history
    h.press_char('i');
    h.type_text("first msg");
    h.press_enter();
    h.type_text("second msg");
    h.press_enter();
    // Now up arrow should recall history
    h.press_key(KeyCode::Up);
    h.assert_input_text("second msg");
    h.press_key(KeyCode::Up);
    h.assert_input_text("first msg");
    h.press_key(KeyCode::Down);
    h.assert_input_text("second msg");
}

// ── Unread counts ──────────────────────────────────────────────────────


// ── Channel navigation edge cases ─────────────────────────────────────

#[tokio::test]
async fn j_at_last_channel_wraps_to_first() {
    let mut h = setup_workspace();
    h.press_char('j'); // C_RAND
    h.press_char('j'); // D_ALICE
    h.assert_active_channel("D_ALICE");
    h.press_char('j'); // wraps to C_GEN
    h.assert_active_channel("C_GEN");
}

#[tokio::test]
async fn k_at_first_channel_wraps_to_last() {
    let mut h = setup_workspace();
    h.assert_active_channel("C_GEN");
    h.press_char('k'); // wraps to D_ALICE
    h.assert_active_channel("D_ALICE");
}

// ── Message navigation edge cases ─────────────────────────────────────

#[tokio::test]
async fn message_nav_on_empty_channel_does_not_panic() {
    let mut h = TestHarness::new();
    h.add_channel("C_EMPTY", "empty");
    h.press_enter(); // -> Messages on empty channel
    h.press_char('j'); // should not panic
    h.press_char('k'); // should not panic
    h.press_char('G'); // should not panic
    h.press_char('g'); // should not panic
    assert_eq!(h.selected_message_idx(), 0);
}

#[tokio::test]
async fn message_nav_boundary_oldest_stays() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('g'); // oldest message
    h.assert_selected_message("oldest message");
    h.press_char('k'); // try to go older
    h.assert_selected_message("oldest message"); // should stay
}

#[tokio::test]
async fn message_nav_boundary_newest_stays() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.assert_selected_message("newest message");
    h.press_char('j'); // try to go newer
    h.assert_selected_message("newest message"); // should stay
}

// ── Thread on message without existing thread ──────────────────────────

#[tokio::test]
async fn enter_on_non_threaded_message_opens_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    // Select newest message (no thread_ts, no reply_count)
    h.assert_selected_message("newest message");
    h.press_enter(); // open thread
    h.assert_focus(Focus::Thread);
    // Thread should open with the message's own ts as parent_ts
    h.assert_thread_open("C_GEN", "1002.000");
}

// ── Multiple WS messages ───────────────────────────────────────────────

#[tokio::test]
async fn rapid_ws_messages_all_appended() {
    let mut h = setup_workspace();
    let initial = h.state.channel_data.get("C_GEN").unwrap().messages.len();
    for i in 0..10 {
        h.send_event(ws_msg("C_GEN", "U_ALICE", &format!("rapid {}", i), &format!("5000.{:03}", i), None));
    }
    let final_count = h.state.channel_data.get("C_GEN").unwrap().messages.len();
    assert_eq!(final_count, initial + 10, "all 10 rapid messages should be appended");
}

#[tokio::test]
async fn duplicate_ws_message_not_added_twice() {
    let mut h = setup_workspace();
    let initial = h.state.channel_data.get("C_GEN").unwrap().messages.len();
    h.send_event(ws_msg("C_GEN", "U_ALICE", "dup msg", "5000.000", None));
    h.send_event(ws_msg("C_GEN", "U_ALICE", "dup msg", "5000.000", None));
    let final_count = h.state.channel_data.get("C_GEN").unwrap().messages.len();
    assert_eq!(final_count, initial + 1, "duplicate message should not be added twice");
}

// ── WS events for unknown channels ────────────────────────────────────

#[tokio::test]
async fn ws_message_for_unknown_channel_creates_channel_data() {
    let mut h = setup_workspace();
    assert!(!h.state.channel_data.contains_key("C_UNKNOWN"));
    h.send_event(ws_msg("C_UNKNOWN", "U_ALICE", "hello unknown", "9000.000", None));
    assert!(h.state.channel_data.contains_key("C_UNKNOWN"));
    assert_eq!(h.state.channel_data.get("C_UNKNOWN").unwrap().messages.len(), 1);
}

// ── Disconnect / reconnect ────────────────────────────────────────────

#[tokio::test]
async fn disconnect_event_sets_connected_false() {
    let mut h = setup_workspace();
    h.send_event(Event::SlackConnected { self_id: "U_ME".into(), team: "T".into() });
    assert!(h.is_connected());
    h.send_event(Event::SlackDisconnected);
    assert!(!h.is_connected());
}

// ── Insert mode from different contexts ────────────────────────────────

#[tokio::test]
async fn a_enters_insert_mode() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('a');
    h.assert_mode(InputMode::Insert);
    h.assert_focus(Focus::Input);
}

#[tokio::test]
async fn insert_from_channel_list_does_not_reply_to_thread() {
    let mut h = setup_workspace();
    h.press_char('i'); // enter insert from channel list
    assert!(!h.reply_to_thread());
    h.type_text("msg");
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    if let Some(ApiCall::PostMessage { thread_ts, .. }) = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. })) {
        assert!(thread_ts.is_none(), "message from channel list should not have thread_ts");
    }
}

// ── Section collapse ───────────────────────────────────────────────────

#[tokio::test]
async fn section_collapse_via_state() {
    let mut h = setup_workspace();
    assert!(!h.state.collapsed_sections.contains("SEC1"));
    h.state.toggle_section_collapse("SEC1");
    assert!(h.state.collapsed_sections.contains("SEC1"));
    h.state.toggle_section_collapse("SEC1");
    assert!(!h.state.collapsed_sections.contains("SEC1"));
}

// ── History loading ────────────────────────────────────────────────────

#[tokio::test]
async fn history_loaded_event_sets_messages() {
    let mut h = setup_workspace();
    h.send_event(Event::HistoryLoaded {
        channel_id: "C_GEN".into(),
        messages: vec![msg("loaded msg", "100.000")],
        has_more: true,
    });
    let cd = h.state.channel_data.get("C_GEN").unwrap();
    assert_eq!(cd.messages.len(), 1);
    assert_eq!(cd.messages[0].text, "loaded msg");
    assert!(cd.has_more_history);
}

#[tokio::test]
async fn older_history_prepended() {
    let mut h = setup_workspace();
    // Already has 3 messages in C_GEN from setup
    let initial = h.state.channel_data.get("C_GEN").unwrap().messages.len();
    h.send_event(Event::OlderHistoryLoaded {
        channel_id: "C_GEN".into(),
        messages: vec![msg("pretty old", "600.000"), msg("very old", "500.000")],
        has_more: false,
    });
    let cd = h.state.channel_data.get("C_GEN").unwrap();
    assert_eq!(cd.messages.len(), initial + 2);
    // Older messages should be at the front (oldest first)
    assert_eq!(cd.messages[0].text, "very old");
    assert_eq!(cd.messages[1].text, "pretty old");
}

// ── Thread loaded event ────────────────────────────────────────────────

#[tokio::test]
async fn thread_loaded_replaces_thread_messages() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_enter(); // -> Thread

    // First load
    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![msg("parent", "1001.000"), thread_msg("r1", "1001.001", "1001.000")],
    });
    assert_eq!(h.thread_message_count(), 2);

    // Second load replaces
    h.send_event(Event::ThreadLoaded {
        channel_id: "C_GEN".into(),
        thread_ts: "1001.000".into(),
        messages: vec![
            msg("parent", "1001.000"),
            thread_msg("r1", "1001.001", "1001.000"),
            thread_msg("r2", "1001.002", "1001.000"),
            thread_msg("r3", "1001.003", "1001.000"),
        ],
    });
    assert_eq!(h.thread_message_count(), 4);
}

// ── Resize event ───────────────────────────────────────────────────────

#[tokio::test]
async fn resize_event_marks_dirty() {
    let mut h = setup_workspace();
    h.state.dirty = false;
    h.send_event(Event::Resize(120, 40));
    assert!(h.state.dirty);
}

// ── User picker ────────────────────────────────────────────────────────

#[tokio::test]
async fn at_in_insert_opens_user_picker() {
    let mut h = setup_workspace();
    h.press_char('i'); // -> Insert
    h.press_char('@');
    h.assert_mode(InputMode::UserPicker);
}

#[tokio::test]
async fn user_picker_esc_inserts_literal_at() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.press_char('@');
    h.assert_mode(InputMode::UserPicker);
    h.press_esc();
    h.assert_mode(InputMode::Insert);
    h.assert_input_text("@");
}

#[tokio::test]
async fn user_picker_confirm_inserts_mention() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.press_char('@');
    h.assert_mode(InputMode::UserPicker);
    // Type to filter, then confirm
    h.type_text("alice");
    h.press_enter();
    h.assert_mode(InputMode::Insert);
    // Should have inserted a mention
    let text = h.input_text().to_string();
    assert!(text.contains("<@U_ALICE|Alice>"), "expected mention in input, got: {}", text);
}

// ── Message sent event ─────────────────────────────────────────────────

#[tokio::test]
async fn message_sent_event_handled() {
    let mut h = setup_workspace();
    // MessageSent is fired after a successful chat.postMessage
    h.send_event(Event::MessageSent {
        channel_id: "C_GEN".into(),
        ts: "9999.000".into(),
    });
    // Should not crash; state should remain functional
    h.assert_focus(Focus::ChannelList);
}

// ── Channel with no messages ───────────────────────────────────────────

#[tokio::test]
async fn open_channel_with_no_history_does_not_crash() {
    let mut h = TestHarness::new();
    h.add_channel("C_EMPTY", "empty");
    h.press_enter(); // -> Messages
    h.assert_focus(Focus::Messages);
    h.press_char('i'); // -> Insert
    h.assert_mode(InputMode::Insert);
    h.press_esc();
    h.assert_focus(Focus::Messages);
}

// ── Thread reply from Messages pane vs Thread pane ─────────────────────

#[tokio::test]
async fn i_from_messages_does_not_set_reply_to_thread_even_when_thread_open() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_enter(); // -> Thread
    h.press_tab(); // -> Messages (thread still open)
    h.assert_focus(Focus::Messages);
    assert!(h.thread_open());
    h.press_char('i'); // insert from Messages
    assert!(!h.reply_to_thread(), "i from Messages should NOT reply to thread");
}

#[tokio::test]
async fn big_r_from_messages_sets_reply_to_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k'); // select middle
    h.press_char('R');
    assert!(h.reply_to_thread(), "R from Messages should reply to thread");
}

// ── Switching channels clears thread state ─────────────────────────────

#[tokio::test]
async fn bracket_switch_from_messages_with_thread_open_closes_thread() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('k');
    h.press_enter(); // -> Thread on C_GEN
    h.press_tab(); // -> Messages
    h.assert_focus(Focus::Messages);
    assert!(h.thread_open());
    h.press_char(']'); // switch to C_RAND
    h.assert_active_channel("C_RAND");
    h.assert_thread_closed();
}

// ── DM display ─────────────────────────────────────────────────────────

#[tokio::test]
async fn dm_channel_shows_user_display_name() {
    let h = setup_workspace();
    let dm = h.state.channels.iter().find(|c| c.id == "D_ALICE").unwrap();
    assert!(dm.is_im);
    assert_eq!(dm.user.as_deref(), Some("U_ALICE"));
    // The display name for DMs comes from user_display_name
    let name = h.state.user_display_name("U_ALICE");
    assert_eq!(name, "Alice");
}

// ── Self messages should not increment unread ──────────────────────────

#[tokio::test]
async fn own_message_does_not_increment_unread() {
    let mut h = setup_workspace();
    h.set_self_user("U_ME");
    let before = h.state.channels.iter().find(|c| c.id == "C_RAND").unwrap().unread_count_display;
    h.send_event(ws_msg("C_RAND", "U_ME", "my own msg", "2002.000", None));
    let after = h.state.channels.iter().find(|c| c.id == "C_RAND").unwrap().unread_count_display;
    assert_eq!(after, before, "own messages should not increment unread count");
}

// ── Unicode cursor safety ─────────────────────────────────────────────

#[tokio::test]
async fn typing_emoji_cursor_position() {
    let mut h = setup_workspace();
    h.press_char('i');
    // Type an emoji (multi-byte char)
    h.press_char('\u{1F600}'); // 😀
    assert_eq!(h.state.input_cursor, 1);
    assert_eq!(h.input_text(), "\u{1F600}");
    h.press_char('a');
    assert_eq!(h.state.input_cursor, 2);
    assert_eq!(h.input_text(), "\u{1F600}a");
}

#[tokio::test]
async fn backspace_multibyte() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("a\u{1F600}b");
    assert_eq!(h.state.input_cursor, 3);
    h.press_key(KeyCode::Backspace);
    assert_eq!(h.input_text(), "a\u{1F600}");
    h.press_key(KeyCode::Backspace);
    assert_eq!(h.input_text(), "a");
}

#[tokio::test]
async fn word_nav_unicode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("héllo wörld");
    // cursor at end (char count)
    assert_eq!(h.state.input_cursor, h.state.input_char_count());
    h.press_alt('b'); // word backward
    assert_eq!(h.state.input_cursor, 6); // before 'wörld'
    h.press_alt('b');
    assert_eq!(h.state.input_cursor, 0); // beginning
    h.press_alt('f'); // word forward
    assert_eq!(h.state.input_cursor, 6); // after 'héllo '
}

#[tokio::test]
async fn delete_word_backward_unicode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("café latte");
    h.press_ctrl('w');
    assert_eq!(h.input_text(), "café ");
    h.press_ctrl('w');
    assert_eq!(h.input_text(), "");
}

#[tokio::test]
async fn ctrl_u_unicode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("🎉hello");
    // Move cursor to after the emoji (position 1)
    h.press_key(KeyCode::Home);
    h.press_key(KeyCode::Right);
    assert_eq!(h.state.input_cursor, 1);
    h.press_ctrl('u');
    assert_eq!(h.input_text(), "hello");
    assert_eq!(h.state.input_cursor, 0);
}

#[tokio::test]
async fn ctrl_k_unicode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hi🌍bye");
    h.press_key(KeyCode::Home);
    h.press_key(KeyCode::Right);
    h.press_key(KeyCode::Right);
    assert_eq!(h.state.input_cursor, 2);
    h.press_ctrl('k');
    assert_eq!(h.input_text(), "hi");
}

// ── Multiline input ───────────────────────────────────────────────────

#[tokio::test]
async fn shift_enter_inserts_newline() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello");
    h.send_event(Event::Key(crossterm::event::KeyEvent {
        code: KeyCode::Enter,
        modifiers: crossterm::event::KeyModifiers::SHIFT,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }));
    h.type_text("world");
    assert_eq!(h.input_text(), "hello\nworld");
}

#[tokio::test]
async fn enter_sends_multiline() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("line1");
    h.send_event(Event::Key(crossterm::event::KeyEvent {
        code: KeyCode::Enter,
        modifiers: crossterm::event::KeyModifiers::SHIFT,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }));
    h.type_text("line2");
    assert_eq!(h.input_text(), "line1\nline2");
    h.press_enter(); // send
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let post = calls.iter().find(|c| matches!(c, ApiCall::PostMessage { .. }));
    match post {
        Some(ApiCall::PostMessage { text, .. }) => {
            assert_eq!(text, "line1\nline2");
        }
        _ => panic!("expected PostMessage"),
    }
}

// ── Tab toggle reply target ───────────────────────────────────────────

#[tokio::test]
async fn tab_toggles_reply_target() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('k'); // select middle
    h.press_char('R'); // open thread + insert mode, reply_to_thread = true
    assert!(h.reply_to_thread());
    h.press_tab(); // toggle to channel
    assert!(!h.reply_to_thread());
    h.press_tab(); // toggle back to thread
    assert!(h.reply_to_thread());
}

#[tokio::test]
async fn tab_without_thread_noop() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages (no thread)
    h.press_char('i'); // insert mode
    assert!(!h.reply_to_thread());
    h.press_tab(); // no thread open, should be a no-op
    assert!(!h.reply_to_thread());
}

// ── Draft per channel ─────────────────────────────────────────────────

#[tokio::test]
async fn draft_saved_on_switch() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("draft text");
    h.press_esc(); // -> Normal
    h.press_char(']'); // switch to C_RAND
    h.assert_active_channel("C_RAND");
    h.assert_input_empty();
    h.press_char('['); // switch back to C_GEN
    h.assert_active_channel("C_GEN");
    h.assert_input_text("draft text");
}

#[tokio::test]
async fn draft_cleared_after_send() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.type_text("will send");
    h.press_enter(); // send
    h.assert_input_empty();
    h.press_esc(); // -> Normal
    h.press_char(']'); // C_RAND
    h.press_char('['); // back to C_GEN
    h.assert_input_empty(); // draft was cleared on send
}

// ── Kill ring ─────────────────────────────────────────────────────────

#[tokio::test]
async fn ctrl_w_y_roundtrip() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello world");
    h.press_ctrl('w'); // kill "world"
    assert_eq!(h.input_text(), "hello ");
    h.press_ctrl('y'); // yank it back
    assert_eq!(h.input_text(), "hello world");
}

#[tokio::test]
async fn ctrl_u_y_roundtrip() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.type_text("hello world");
    h.press_key(KeyCode::Home);
    h.press_key(KeyCode::Right);
    h.press_key(KeyCode::Right);
    h.press_key(KeyCode::Right);
    h.press_key(KeyCode::Right);
    h.press_key(KeyCode::Right); // cursor after "hello"
    h.press_ctrl('u'); // kill "hello"
    assert_eq!(h.input_text(), " world");
    h.press_key(KeyCode::End);
    h.press_ctrl('y'); // yank "hello" at end
    assert_eq!(h.input_text(), " worldhello");
}

#[tokio::test]
async fn kill_ring_bounded() {
    let mut h = setup_workspace();
    h.press_char('i');
    for i in 0..20 {
        h.type_text(&format!("word{} ", i));
        h.press_ctrl('w');
    }
    assert!(h.state.kill_ring.len() <= 16);
}

// ── Line-based scroll ────────────────────────────────────────────────

#[tokio::test]
async fn scroll_wheel_does_not_change_selection() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    let sel_before = h.selected_message_idx();
    h.state.max_scroll_offset = 100;
    // Positive delta increases scroll_y (toward newer/bottom)
    assert!(h.state.messages_scroll_lines(3));
    assert_eq!(h.selected_message_idx(), sel_before);
    assert_eq!(h.state.messages_scroll_override, Some(3));
}

#[tokio::test]
async fn jk_clears_scroll_override() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.state.max_scroll_offset = 100;
    h.state.messages_scroll_override = Some(50);
    h.press_char('k'); // select older
    assert!(h.state.messages_scroll_override.is_none());
}

#[tokio::test]
async fn scroll_lines_clamps_to_max() {
    let mut h = setup_workspace();
    h.state.max_scroll_offset = 10;
    assert!(h.state.messages_scroll_lines(100));
    assert_eq!(h.state.messages_scroll_override, Some(10));
}

#[tokio::test]
async fn scroll_lines_clamps_to_zero() {
    let mut h = setup_workspace();
    h.state.max_scroll_offset = 100;
    h.state.messages_scroll_override = Some(2);
    assert!(h.state.messages_scroll_lines(-100));
    assert_eq!(h.state.messages_scroll_override, Some(0));
}

/// scroll_y=0 shows the top (oldest). Higher scroll_y shows newer/bottom.
/// ScrollUp (wheel up) should DECREASE scroll_y (show older).
/// ScrollDown (wheel down) should INCREASE scroll_y (show newer).
#[tokio::test]
async fn mouse_scroll_up_decreases_scroll_y() {
    use ratatui::layout::Rect;
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    // Set up messages area and a starting scroll position in the middle
    let area = Rect::new(20, 0, 60, 30);
    h.state.messages_area = area;
    h.state.max_scroll_offset = 200;
    h.state.messages_scroll_override = Some(100);
    h.state.messages_render_info = Some(crate::state::MessagesRenderInfo {
        inner_x: area.x + 1,
        inner_y: area.y + 1,
        inner_height: area.height - 2,
        scroll_y: 100,
    });

    h.scroll_up_in(area);
    // ScrollUp should decrease scroll_y (toward older/top)
    let new_scroll = h.state.messages_scroll_override.unwrap();
    assert!(new_scroll < 100, "ScrollUp should decrease scroll_y, got {}", new_scroll);
}

#[tokio::test]
async fn mouse_scroll_down_increases_scroll_y() {
    use ratatui::layout::Rect;
    let mut h = setup_workspace();
    h.press_enter();
    let area = Rect::new(20, 0, 60, 30);
    h.state.messages_area = area;
    h.state.max_scroll_offset = 200;
    h.state.messages_scroll_override = Some(100);
    h.state.messages_render_info = Some(crate::state::MessagesRenderInfo {
        inner_x: area.x + 1,
        inner_y: area.y + 1,
        inner_height: area.height - 2,
        scroll_y: 100,
    });

    h.scroll_down_in(area);
    let new_scroll = h.state.messages_scroll_override.unwrap();
    assert!(new_scroll > 100, "ScrollDown should increase scroll_y, got {}", new_scroll);
}

#[tokio::test]
async fn mouse_scroll_up_at_zero_stays_at_zero() {
    use ratatui::layout::Rect;
    let mut h = setup_workspace();
    h.press_enter();
    let area = Rect::new(20, 0, 60, 30);
    h.state.messages_area = area;
    h.state.max_scroll_offset = 200;
    h.state.messages_scroll_override = Some(0);
    h.state.messages_render_info = Some(crate::state::MessagesRenderInfo {
        inner_x: area.x + 1,
        inner_y: area.y + 1,
        inner_height: area.height - 2,
        scroll_y: 0,
    });

    h.scroll_up_in(area);
    // At the top, can't scroll further up
    assert_eq!(h.state.messages_scroll_override, Some(0));
}

#[tokio::test]
async fn mouse_scroll_down_at_max_stays_at_max() {
    use ratatui::layout::Rect;
    let mut h = setup_workspace();
    h.press_enter();
    let area = Rect::new(20, 0, 60, 30);
    h.state.messages_area = area;
    h.state.max_scroll_offset = 200;
    h.state.messages_scroll_override = Some(200);
    h.state.messages_render_info = Some(crate::state::MessagesRenderInfo {
        inner_x: area.x + 1,
        inner_y: area.y + 1,
        inner_height: area.height - 2,
        scroll_y: 200,
    });

    h.scroll_down_in(area);
    assert_eq!(h.state.messages_scroll_override, Some(200));
}

/// Channel list scroll must not wrap around.
#[tokio::test]
async fn channel_scroll_does_not_wrap() {
    use ratatui::layout::Rect;
    let mut h = setup_workspace();
    let area = Rect::new(0, 0, 20, 30);
    h.state.channel_list_area = area;

    // Go to last channel
    let last = h.state.channels.len() - 1;
    h.state.selected_channel_idx = last;
    let before = h.state.selected_channel_idx;
    h.scroll_down_in(area);
    // Should NOT wrap to first channel
    assert!(
        h.state.selected_channel_idx >= before || h.state.selected_channel_idx == before,
        "channel scroll down at bottom should not wrap"
    );

    // Go to first channel
    h.state.selected_channel_idx = 0;
    h.state.selected_visual_idx = 0;
    let before = h.state.selected_channel_idx;
    h.scroll_up_in(area);
    assert_eq!(
        h.state.selected_channel_idx, before,
        "channel scroll up at top should not wrap"
    );
}

// ── File upload ──────────────────────────────────────────────────────

#[tokio::test]
async fn ctrl_o_opens_file_path_mode() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.press_ctrl('o');
    h.assert_mode(InputMode::FilePath);
    assert!(h.state.file_path_input.is_empty());
}

#[tokio::test]
async fn file_path_esc_returns_to_insert() {
    let mut h = setup_workspace();
    h.press_char('i');
    h.press_ctrl('o');
    h.assert_mode(InputMode::FilePath);
    h.type_text("/some/path");
    h.press_esc();
    h.assert_mode(InputMode::Insert);
    assert!(h.state.file_path_input.is_empty());
}

#[tokio::test]
async fn file_path_enter_nonexistent_shows_error() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages
    h.press_char('i');
    h.press_ctrl('o');
    h.type_text("/nonexistent/file.png");
    h.press_enter();
    assert!(h.state.upload_status.as_ref().unwrap().contains("Read error"));
    h.assert_mode(InputMode::FilePath);
}

#[tokio::test]
async fn file_path_upload_real_file() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    h.press_char('i');
    h.press_ctrl('o');

    // Upload Cargo.toml as a test file (it exists)
    h.type_text("Cargo.toml");
    h.press_enter();
    h.yield_to_spawned_tasks().await;

    let calls = h.api_calls();
    let upload = calls.iter().find(|c| matches!(c, ApiCall::FilesUpload { .. }));
    match upload {
        Some(ApiCall::FilesUpload { channel, filename, .. }) => {
            assert_eq!(channel, "C_GEN");
            assert_eq!(filename, "Cargo.toml");
        }
        _ => panic!("expected FilesUpload call"),
    }
}

// ── Context menu ─────────────────────────────────────────────────────

#[tokio::test]
async fn spacebar_opens_context_menu() {
    let mut h = setup_workspace();
    h.press_enter(); // -> Messages on C_GEN
    assert!(!h.state.show_context_menu);
    h.press_char(' ');
    assert!(h.state.show_context_menu);
    assert_eq!(h.state.context_menu_selected, 0);
}

#[tokio::test]
async fn context_menu_esc_closes() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char(' ');
    assert!(h.state.show_context_menu);
    h.press_esc();
    assert!(!h.state.show_context_menu);
}

#[tokio::test]
async fn context_menu_navigate_and_select() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char(' ');
    // Navigate down to "Reply in thread" (index 1)
    h.press_char('j');
    assert_eq!(h.state.context_menu_selected, 1);
    // Navigate down to "React with emoji" (index 2)
    h.press_char('j');
    assert_eq!(h.state.context_menu_selected, 2);
    // Navigate up
    h.press_char('k');
    assert_eq!(h.state.context_menu_selected, 1);
    // Select "Reply in thread"
    h.press_enter();
    assert!(!h.state.show_context_menu);
    assert!(h.state.reply_to_thread);
    h.assert_mode(InputMode::Insert);
}

#[tokio::test]
async fn context_menu_wraps_around() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char(' ');
    // Navigate up from first item -> should wrap to last
    h.press_char('k');
    assert_eq!(h.state.context_menu_selected, 3); // MENU_ITEMS.len() - 1
}

// ── Reaction toggle tests ─────────────────────────────────────────────

fn msg_with_reactions(text: &str, ts: &str, reactions: Vec<Reaction>) -> crate::slack::types::Message {
    let mut m = msg(text, ts);
    m.reactions = reactions;
    m
}

fn setup_workspace_with_reactions() -> TestHarness {
    let mut h = TestHarness::new();
    h.set_self_user("U_ME");
    h.add_user("U_ME", "me", "Me");
    h.add_user("U_ALICE", "alice", "Alice");
    h.add_channel("C_GEN", "general");
    h.add_messages(
        "C_GEN",
        vec![
            msg("no reactions", "1000.000"),
            msg_with_reactions("has reactions", "1001.000", vec![
                Reaction { name: "thumbsup".into(), count: 2, users: vec!["U_ME".into(), "U_ALICE".into()] },
                Reaction { name: "fire".into(), count: 1, users: vec!["U_ALICE".into()] },
            ]),
        ],
    );
    h
}

#[tokio::test]
async fn emoji_picker_toggle_removes_own_reaction() {
    let mut h = setup_workspace_with_reactions();
    h.press_enter(); // -> Messages (newest = "has reactions" at idx 0)
    h.assert_selected_message("has reactions");
    h.press_char('r'); // open emoji picker
    h.assert_mode(InputMode::EmojiPicker);
    // First result should be "thumbsup" (message reaction, prioritized)
    assert_eq!(h.state.emoji_picker_results[0].0, "thumbsup");
    // Confirm the first result — user has reacted, so this should remove
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    h.assert_mode(InputMode::Normal);
    let calls = h.api_calls();
    let remove = calls.iter().find(|c| matches!(c, ApiCall::RemoveReaction { .. }));
    assert!(remove.is_some(), "expected RemoveReaction call, got {:?}", calls);
}

#[tokio::test]
async fn emoji_picker_toggle_adds_others_reaction() {
    let mut h = setup_workspace_with_reactions();
    h.press_enter(); // -> Messages
    h.assert_selected_message("has reactions");
    h.press_char('r');
    // Navigate to "fire" (2nd result) — user hasn't reacted
    h.press_char('j');
    assert_eq!(h.state.emoji_picker_results[h.state.emoji_picker_selected].0, "fire");
    h.press_enter();
    h.yield_to_spawned_tasks().await;
    let calls = h.api_calls();
    let add = calls.iter().find(|c| matches!(c, ApiCall::AddReaction { .. }));
    assert!(add.is_some(), "expected AddReaction call, got {:?}", calls);
}

#[tokio::test]
async fn emoji_picker_prepends_message_reactions() {
    let mut h = setup_workspace_with_reactions();
    h.press_enter();
    h.assert_selected_message("has reactions");
    h.press_char('r');
    // First two results should be the message's reactions in order
    assert_eq!(h.state.emoji_picker_results[0].0, "thumbsup");
    assert_eq!(h.state.emoji_picker_results[1].0, "fire");
    // Remaining results should be other emoji
    assert_ne!(h.state.emoji_picker_results[2].0, "thumbsup");
    assert_ne!(h.state.emoji_picker_results[2].0, "fire");
    h.press_esc();
}

#[tokio::test]
async fn emoji_picker_message_reactions_tracks_self() {
    let mut h = setup_workspace_with_reactions();
    h.press_enter();
    h.press_char('r');
    // thumbsup: user reacted, fire: user did not
    let reactions = &h.state.emoji_picker_message_reactions;
    assert_eq!(reactions.len(), 2);
    assert_eq!(reactions[0], ("thumbsup".into(), true));
    assert_eq!(reactions[1], ("fire".into(), false));
    h.press_esc();
}

// --- Inline emoji picker tests ---

#[tokio::test]
async fn colon_in_insert_opens_inline_emoji_picker() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    assert_eq!(h.state.input_mode, InputMode::Insert);
    h.press_char(':');
    assert_eq!(h.state.input_mode, InputMode::EmojiPicker);
    assert_eq!(h.state.emoji_picker_inline_colon_pos, Some(0));
    assert_eq!(h.state.input_text, ":");
}

#[tokio::test]
async fn inline_emoji_picker_confirm_replaces_query() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    h.press_char('h');
    h.press_char('i');
    h.press_char(' ');
    h.press_char(':');
    assert_eq!(h.state.input_mode, InputMode::EmojiPicker);
    assert_eq!(h.state.emoji_picker_inline_colon_pos, Some(3));
    // Type "rock" to filter to "rocket"
    h.press_char('r');
    h.press_char('o');
    h.press_char('c');
    h.press_char('k');
    assert_eq!(h.state.input_text, "hi :rock");
    assert!(h.state.emoji_picker_results.iter().any(|(name, _, _)| name == "rocket"));
    // Select rocket and confirm
    let rocket_idx = h.state.emoji_picker_results.iter().position(|(n, _, _)| n == "rocket").unwrap();
    h.state.emoji_picker_selected = rocket_idx;
    h.press_enter();
    assert_eq!(h.state.input_mode, InputMode::Insert);
    assert_eq!(h.state.input_text, "hi :rocket:");
}

#[tokio::test]
async fn inline_emoji_picker_esc_keeps_partial() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    h.press_char(':');
    h.press_char('r');
    h.press_char('o');
    assert_eq!(h.state.input_text, ":ro");
    h.press_esc();
    assert_eq!(h.state.input_mode, InputMode::Insert);
    assert_eq!(h.state.input_text, ":ro");
}

#[tokio::test]
async fn inline_emoji_picker_backspace_empty_removes_colon() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    h.press_char('h');
    h.press_char(':');
    assert_eq!(h.state.input_text, "h:");
    h.press_key(KeyCode::Backspace);
    assert_eq!(h.state.input_mode, InputMode::Insert);
    assert_eq!(h.state.input_text, "h");
}

#[tokio::test]
async fn colon_inside_backticks_does_not_open_picker() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('i');
    h.press_char('`');
    h.press_char(':');
    assert_eq!(h.state.input_mode, InputMode::Insert);
    assert_eq!(h.state.input_text, "`:");
}

#[tokio::test]
async fn ctrl_p_in_emoji_picker_opens_3d_preview() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('r');
    assert_eq!(h.state.input_mode, InputMode::EmojiPicker);
    assert!(!h.state.emoji_picker_results.is_empty());
    let expected_name = h.state.emoji_picker_results[0].0.clone();
    h.press_ctrl('p');
    assert_eq!(h.state.input_mode, InputMode::EmojiPreview);
    assert_eq!(h.state.emoji_preview_name, expected_name);
    assert_eq!(h.state.emoji_preview_time, 0.0);
}

#[tokio::test]
async fn emoji_preview_esc_returns_to_picker() {
    let mut h = setup_workspace();
    h.press_enter();
    h.press_char('r');
    h.press_ctrl('p');
    assert_eq!(h.state.input_mode, InputMode::EmojiPreview);
    h.press_esc();
    assert_eq!(h.state.input_mode, InputMode::EmojiPicker);
}
