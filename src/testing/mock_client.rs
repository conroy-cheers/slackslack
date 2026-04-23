use crate::slack::client::SlackApi;
use crate::slack::realtime::RealtimeEvent;
use crate::slack::types::*;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ApiCall {
    PostMessage {
        channel: String,
        text: String,
        thread_ts: Option<String>,
    },
    AddReaction {
        channel: String,
        timestamp: String,
        name: String,
    },
    RemoveReaction {
        channel: String,
        timestamp: String,
        name: String,
    },
    LoadHistory {
        channel: String,
        limit: u32,
    },
    LoadReplies {
        channel: String,
        thread_ts: String,
    },
    MarkRead {
        channel: String,
        ts: String,
    },
    DownloadFile {
        url: String,
    },
    SearchMessages {
        query: String,
    },
    FilesUpload {
        channel: String,
        filename: String,
        thread_ts: Option<String>,
    },
}

#[derive(Clone)]
pub struct MockSlackClient {
    pub messages: Arc<Mutex<HashMap<String, Vec<Message>>>>,
    pub thread_replies: Arc<Mutex<HashMap<(String, String), Vec<Message>>>>,
    pub calls: Arc<Mutex<Vec<ApiCall>>>,
}

impl MockSlackClient {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(HashMap::new())),
            thread_replies: Arc::new(Mutex::new(HashMap::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_channel_messages(&self, channel_id: &str, msgs: Vec<Message>) {
        self.messages
            .lock()
            .unwrap()
            .insert(channel_id.to_string(), msgs);
    }

    pub fn add_thread_replies(&self, channel_id: &str, thread_ts: &str, msgs: Vec<Message>) {
        self.thread_replies
            .lock()
            .unwrap()
            .insert((channel_id.to_string(), thread_ts.to_string()), msgs);
    }

    pub fn take_calls(&self) -> Vec<ApiCall> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    pub fn last_call(&self) -> Option<ApiCall> {
        self.calls.lock().unwrap().last().cloned()
    }

    fn record(&self, call: ApiCall) {
        self.calls.lock().unwrap().push(call);
    }
}

impl SlackApi for MockSlackClient {
    fn conversations_info(
        &self,
        _channel: &str,
    ) -> impl std::future::Future<Output = Result<ConversationsInfoData>> + Send {
        async { Ok(ConversationsInfoData::default()) }
    }

    fn conversations_members(
        &self,
        _channel: &str,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = Result<ConversationsMembersData>> + Send {
        async { Ok(ConversationsMembersData::default()) }
    }

    fn conversations_open(
        &self,
        _users: &str,
    ) -> impl std::future::Future<Output = Result<ConversationsOpenData>> + Send {
        async { Ok(ConversationsOpenData::default()) }
    }

    fn users_conversations(
        &self,
        _types: &str,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = Result<UsersConversationsData>> + Send {
        async { Ok(UsersConversationsData::default()) }
    }

    fn conversations_list_all(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Channel>>> + Send {
        async { Ok(Vec::new()) }
    }

    fn users_list(
        &self,
        _cursor: Option<&str>,
        _limit: u32,
    ) -> impl std::future::Future<Output = Result<UsersListData>> + Send {
        async { Ok(UsersListData::default()) }
    }

    fn emoji_list(&self) -> impl std::future::Future<Output = Result<EmojiListData>> + Send {
        async { Ok(EmojiListData::default()) }
    }

    fn channel_sections_list(
        &self,
    ) -> impl std::future::Future<Output = Result<ChannelSectionsListData>> + Send {
        async { Ok(ChannelSectionsListData::default()) }
    }

    fn users_profile_get(
        &self,
        _user_id: Option<&str>,
        _include_labels: bool,
    ) -> impl std::future::Future<Output = Result<UserProfileGetData>> + Send {
        async { Ok(UserProfileGetData::default()) }
    }

    fn team_profile_get(
        &self,
    ) -> impl std::future::Future<Output = Result<TeamProfileGetData>> + Send {
        async { Ok(TeamProfileGetData::default()) }
    }

    fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        _oldest: Option<&str>,
        _latest: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ConversationsHistoryData>> + Send {
        let channel = channel.to_string();
        let msgs = self
            .messages
            .lock()
            .unwrap()
            .get(&channel)
            .cloned()
            .unwrap_or_default();
        self.record(ApiCall::LoadHistory { channel, limit });
        async move {
            Ok(ConversationsHistoryData {
                messages: msgs,
                has_more: false,
                response_metadata: None,
            })
        }
    }

    fn conversations_replies(
        &self,
        channel: &str,
        thread_ts: &str,
        _limit: u32,
    ) -> impl std::future::Future<Output = Result<ConversationsHistoryData>> + Send {
        let key = (channel.to_string(), thread_ts.to_string());
        let msgs = self
            .thread_replies
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.record(ApiCall::LoadReplies {
            channel: channel.to_string(),
            thread_ts: thread_ts.to_string(),
        });
        async move {
            Ok(ConversationsHistoryData {
                messages: msgs,
                has_more: false,
                response_metadata: None,
            })
        }
    }

    fn conversations_mark(
        &self,
        channel: &str,
        ts: &str,
    ) -> impl std::future::Future<Output = Result<ConversationsMarkData>> + Send {
        self.record(ApiCall::MarkRead {
            channel: channel.to_string(),
            ts: ts.to_string(),
        });
        async { Ok(ConversationsMarkData {}) }
    }

    fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ChatPostMessageData>> + Send {
        let ts = format!("{}.000001", chrono::Utc::now().timestamp());
        self.record(ApiCall::PostMessage {
            channel: channel.to_string(),
            text: text.to_string(),
            thread_ts: thread_ts.map(|s| s.to_string()),
        });
        async move {
            Ok(ChatPostMessageData {
                ts: Some(ts),
                channel: None,
                message: None,
            })
        }
    }

    fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> impl std::future::Future<Output = Result<ReactionsData>> + Send {
        self.record(ApiCall::AddReaction {
            channel: channel.to_string(),
            timestamp: timestamp.to_string(),
            name: name.to_string(),
        });
        async { Ok(ReactionsData {}) }
    }

    fn reactions_remove(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> impl std::future::Future<Output = Result<ReactionsData>> + Send {
        self.record(ApiCall::RemoveReaction {
            channel: channel.to_string(),
            timestamp: timestamp.to_string(),
            name: name.to_string(),
        });
        async { Ok(ReactionsData {}) }
    }

    fn download_file(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send {
        self.record(ApiCall::DownloadFile {
            url: url.to_string(),
        });
        async { Ok(Vec::new()) }
    }

    fn search_messages(
        &self,
        query: &str,
        _page: u32,
        _count: u32,
    ) -> impl std::future::Future<Output = Result<SearchMessagesData>> + Send {
        self.record(ApiCall::SearchMessages {
            query: query.to_string(),
        });
        async { Ok(SearchMessagesData::default()) }
    }

    fn search_files(
        &self,
        _query: &str,
        _page: u32,
        _count: u32,
    ) -> impl std::future::Future<Output = Result<SearchFilesData>> + Send {
        async { Ok(SearchFilesData::default()) }
    }

    fn files_upload(
        &self,
        channel: &str,
        thread_ts: Option<&str>,
        filename: &str,
        _data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<FilesCompleteUploadData>> + Send {
        self.record(ApiCall::FilesUpload {
            channel: channel.to_string(),
            filename: filename.to_string(),
            thread_ts: thread_ts.map(|s| s.to_string()),
        });
        async { Ok(FilesCompleteUploadData::default()) }
    }

    fn files_info(
        &self,
        _file: &str,
        _cursor: Option<&str>,
        _limit: Option<u32>,
    ) -> impl std::future::Future<Output = Result<FilesInfoData>> + Send {
        async { Ok(FilesInfoData::default()) }
    }

    fn files_list(
        &self,
        _cursor: Option<&str>,
        _limit: Option<u32>,
    ) -> impl std::future::Future<Output = Result<FilesListData>> + Send {
        async { Ok(FilesListData::default()) }
    }

    fn pins_list(
        &self,
        _channel: &str,
    ) -> impl std::future::Future<Output = Result<PinsListData>> + Send {
        async { Ok(PinsListData::default()) }
    }

    fn spawn_realtime(
        &self,
        _tx: mpsc::UnboundedSender<RealtimeEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async {})
    }
}
