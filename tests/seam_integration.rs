use libslack::client::SlackApi;
use libslack::realtime::RealtimeEvent;
use libslack::types::{Channel, ConversationsHistoryData};
use slackslack::event::Event;
use slackslack::event::handler::{HandleResult, handle_event};
use slackslack::state::{AppState, Focus};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
enum Call {
    LoadHistory { channel: String, limit: u32 },
}

#[derive(Clone, Default)]
struct MockSlackClient {
    calls: Arc<Mutex<Vec<Call>>>,
}

impl SlackApi for MockSlackClient {
    fn conversations_info(
        &self,
        _channel: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ConversationsInfoData>> + Send
    {
        async { Ok(libslack::types::ConversationsInfoData::default()) }
    }

    fn conversations_members(
        &self,
        _channel: &str,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ConversationsMembersData>>
    + Send {
        async { Ok(libslack::types::ConversationsMembersData::default()) }
    }

    fn conversations_open(
        &self,
        _users: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ConversationsOpenData>> + Send
    {
        async { Ok(libslack::types::ConversationsOpenData::default()) }
    }

    fn users_conversations(
        &self,
        _types: &str,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::UsersConversationsData>> + Send
    {
        async { Ok(libslack::types::UsersConversationsData::default()) }
    }

    fn conversations_list_all(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<Vec<Channel>>> + Send {
        async { Ok(Vec::new()) }
    }

    fn users_list(
        &self,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::UsersListData>> + Send
    {
        async { Ok(libslack::types::UsersListData::default()) }
    }

    fn emoji_list(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::EmojiListData>> + Send
    {
        async { Ok(libslack::types::EmojiListData::default()) }
    }

    fn channel_sections_list(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ChannelSectionsListData>> + Send
    {
        async { Ok(libslack::types::ChannelSectionsListData::default()) }
    }

    fn users_profile_get(
        &self,
        _user_id: Option<&str>,
        _include_labels: bool,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::UserProfileGetData>> + Send
    {
        async { Ok(libslack::types::UserProfileGetData::default()) }
    }

    fn team_profile_get(
        &self,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::TeamProfileGetData>> + Send
    {
        async { Ok(libslack::types::TeamProfileGetData::default()) }
    }

    fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        _oldest: Option<&str>,
        _latest: Option<&str>,
    ) -> impl std::future::Future<Output = anyhow::Result<ConversationsHistoryData>> + Send {
        self.calls.lock().unwrap().push(Call::LoadHistory {
            channel: channel.to_string(),
            limit,
        });
        async move {
            Ok(ConversationsHistoryData {
                messages: vec![],
                has_more: false,
                response_metadata: None,
            })
        }
    }

    fn conversations_replies(
        &self,
        _channel: &str,
        _thread_ts: &str,
        _limit: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<ConversationsHistoryData>> + Send {
        async { Ok(ConversationsHistoryData::default()) }
    }

    fn conversations_mark(
        &self,
        _channel: &str,
        _ts: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ConversationsMarkData>> + Send
    {
        async { Ok(libslack::types::ConversationsMarkData::default()) }
    }

    fn chat_post_message(
        &self,
        _channel: &str,
        _text: &str,
        _thread_ts: Option<&str>,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ChatPostMessageData>> + Send
    {
        async { Ok(libslack::types::ChatPostMessageData::default()) }
    }

    fn reactions_add(
        &self,
        _channel: &str,
        _timestamp: &str,
        _name: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ReactionsData>> + Send
    {
        async { Ok(libslack::types::ReactionsData::default()) }
    }

    fn reactions_remove(
        &self,
        _channel: &str,
        _timestamp: &str,
        _name: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::ReactionsData>> + Send
    {
        async { Ok(libslack::types::ReactionsData::default()) }
    }

    fn download_file(
        &self,
        _url: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send {
        async { Ok(Vec::new()) }
    }

    fn search_messages(
        &self,
        _query: &str,
        _page: u32,
        _count: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::SearchMessagesData>> + Send
    {
        async { Ok(libslack::types::SearchMessagesData::default()) }
    }

    fn search_files(
        &self,
        _query: &str,
        _page: u32,
        _count: u32,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::SearchFilesData>> + Send
    {
        async { Ok(libslack::types::SearchFilesData::default()) }
    }

    fn files_upload(
        &self,
        _channel: &str,
        _thread_ts: Option<&str>,
        _filename: &str,
        _data: Vec<u8>,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::FilesCompleteUploadData>> + Send
    {
        async { Ok(libslack::types::FilesCompleteUploadData::default()) }
    }

    fn files_info(
        &self,
        _file: &str,
        _cursor: Option<&str>,
        _limit: Option<u32>,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::FilesInfoData>> + Send
    {
        async { Ok(libslack::types::FilesInfoData::default()) }
    }

    fn files_list(
        &self,
        _cursor: Option<&str>,
        _limit: Option<u32>,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::FilesListData>> + Send
    {
        async { Ok(libslack::types::FilesListData::default()) }
    }

    fn pins_list(
        &self,
        _channel: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<libslack::types::PinsListData>> + Send
    {
        async { Ok(libslack::types::PinsListData::default()) }
    }

    fn spawn_realtime(
        &self,
        _tx: tokio::sync::mpsc::UnboundedSender<RealtimeEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async {})
    }
}

#[tokio::test]
async fn opening_a_channel_loads_history_via_libslack_trait_seam() {
    let client = MockSlackClient::default();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut state = AppState::new();
    state.channels.push(Channel {
        id: "C1".into(),
        name: Some("general".into()),
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

    let result = handle_event(
        Event::Key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Enter,
        )),
        &mut state,
        &client,
        &event_tx,
    );
    assert!(matches!(result, HandleResult::Continue));
    assert_eq!(state.focus, Focus::Messages);

    tokio::task::yield_now().await;
    let queued = event_rx.recv().await.expect("history load event");
    match queued {
        Event::HistoryLoaded { channel_id, .. } => assert_eq!(channel_id, "C1"),
        other => panic!(
            "expected HistoryLoaded, got {:?}",
            std::mem::discriminant(&other)
        ),
    }

    let calls = client.calls.lock().unwrap();
    assert!(
        matches!(calls.as_slice(), [Call::LoadHistory { channel, limit }] if channel == "C1" && *limit == 50)
    );
}
