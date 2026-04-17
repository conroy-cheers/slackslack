use super::harness::*;
use super::mock_client::ApiCall;
use crate::event::Event;
use crate::slack::types::{WsEvent, WsMessage};
use crate::state::{Focus, InputMode};

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
