use crate::auth::Credentials;
use crate::realtime::{self, RealtimeEvent};
use crate::types::*;
use anyhow::{Result, bail};
use reqwest::header::{COOKIE, HeaderMap, HeaderValue};
use tokio::sync::mpsc;

pub trait SlackApi: Clone + Send + Sync + 'static {
    fn conversations_list_all(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Channel>>> + Send;
    fn users_list(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> impl std::future::Future<Output = Result<UsersListData>> + Send;
    fn emoji_list(&self) -> impl std::future::Future<Output = Result<EmojiListData>> + Send;
    fn channel_sections_list(
        &self,
    ) -> impl std::future::Future<Output = Result<ChannelSectionsListData>> + Send;
    fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ConversationsHistoryData>> + Send;
    fn conversations_replies(
        &self,
        channel: &str,
        thread_ts: &str,
        limit: u32,
    ) -> impl std::future::Future<Output = Result<ConversationsHistoryData>> + Send;
    fn conversations_mark(
        &self,
        channel: &str,
        ts: &str,
    ) -> impl std::future::Future<Output = Result<ConversationsMarkData>> + Send;
    fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ChatPostMessageData>> + Send;
    fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> impl std::future::Future<Output = Result<ReactionsData>> + Send;
    fn reactions_remove(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> impl std::future::Future<Output = Result<ReactionsData>> + Send;
    fn download_file(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send;
    fn search_messages(
        &self,
        query: &str,
        page: u32,
        count: u32,
    ) -> impl std::future::Future<Output = Result<SearchMessagesData>> + Send;
    fn files_upload(
        &self,
        channel: &str,
        thread_ts: Option<&str>,
        filename: &str,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<FilesCompleteUploadData>> + Send;
    fn spawn_realtime(
        &self,
        tx: mpsc::UnboundedSender<RealtimeEvent>,
    ) -> tokio::task::JoinHandle<()>;
}

#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    token: String,
    pub(crate) cookie: String,
    base_url: String,
}

impl SlackClient {
    pub fn new(creds: &Credentials) -> Result<Self> {
        Self::new_with_base_url(creds, "https://slack.com/api")
    }

    pub fn new_with_base_url(creds: &Credentials, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let cookie_val = format!("d={}", creds.cookie);
        headers.insert(COOKIE, HeaderValue::from_str(&cookie_val)?);

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            http,
            token: creds.token.clone(),
            cookie: creds.cookie.clone(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn cookie(&self) -> &str {
        &self.cookie
    }

    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}/{}", self.base_url, method);

        let mut form: Vec<(&str, &str)> = vec![("token", &self.token)];
        form.extend_from_slice(params);

        let resp = self.http.post(&url).form(&form).send().await?;

        let status = resp.status();
        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            bail!("Rate limited, retry after {}s", retry_after);
        }

        if !status.is_success() {
            bail!("HTTP {} from {}", status, method);
        }

        let body = resp.text().await?;
        let parsed: SlackResponse<T> = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("Failed to parse {} response: {}\nBody: {}", method, e, &body[..body.len().min(500)]))?;

        if !parsed.ok {
            bail!(
                "Slack API error from {}: {}",
                method,
                parsed.error.unwrap_or_else(|| "unknown".into())
            );
        }

        Ok(parsed.data)
    }

    // === API Methods ===

    pub async fn auth_test(&self) -> Result<AuthTestData> {
        self.post("auth.test", &[]).await
    }

    pub async fn conversations_list(
        &self,
        types: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ConversationsListData> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("types", types),
            ("limit", &limit_str),
            ("exclude_archived", "true"),
        ];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.post("conversations.list", &params).await
    }

    /// Fetch all channels the user is a member of, paginating automatically.
    pub async fn conversations_list_all(&self) -> Result<Vec<Channel>> {
        let mut all_channels = Vec::new();
        let mut cursor: Option<String> = None;
        let types = "public_channel,private_channel,mpim,im";

        loop {
            let data = self
                .conversations_list(types, cursor.as_deref(), 200)
                .await?;

            all_channels.extend(data.channels);

            let next = data
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if next.is_none() {
                break;
            }
            cursor = next;
        }

        Ok(all_channels)
    }

    pub async fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<ConversationsHistoryData> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("channel", channel), ("limit", &limit_str)];
        if let Some(o) = oldest {
            params.push(("oldest", o));
        }
        if let Some(l) = latest {
            params.push(("latest", l));
        }
        self.post("conversations.history", &params).await
    }

    pub async fn conversations_replies(
        &self,
        channel: &str,
        thread_ts: &str,
        limit: u32,
    ) -> Result<ConversationsHistoryData> {
        let limit_str = limit.to_string();
        self.post(
            "conversations.replies",
            &[
                ("channel", channel),
                ("ts", thread_ts),
                ("limit", &limit_str),
            ],
        )
        .await
    }

    pub async fn conversations_mark(&self, channel: &str, ts: &str) -> Result<ConversationsMarkData> {
        self.post("conversations.mark", &[("channel", channel), ("ts", ts)])
            .await
    }

    pub async fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<ChatPostMessageData> {
        let mut params: Vec<(&str, &str)> = vec![("channel", channel), ("text", text)];
        if let Some(ts) = thread_ts {
            params.push(("thread_ts", ts));
        }
        self.post("chat.postMessage", &params).await
    }

    pub async fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<ReactionsData> {
        self.post(
            "reactions.add",
            &[("channel", channel), ("timestamp", timestamp), ("name", name)],
        )
        .await
    }

    pub async fn reactions_remove(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<ReactionsData> {
        self.post(
            "reactions.remove",
            &[("channel", channel), ("timestamp", timestamp), ("name", name)],
        )
        .await
    }

    pub async fn users_list(&self, cursor: Option<&str>, limit: u32) -> Result<UsersListData> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("limit", &limit_str)];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.post("users.list", &params).await
    }

    pub async fn users_info(&self, user_id: &str) -> Result<UserInfoData> {
        self.post("users.info", &[("user", user_id)]).await
    }

    pub async fn rtm_connect(&self) -> Result<RtmConnectData> {
        self.post("rtm.connect", &[]).await
    }

    pub async fn emoji_list(&self) -> Result<EmojiListData> {
        self.post("emoji.list", &[]).await
    }

    /// Fetch user-defined sidebar channel sections (undocumented API).
    pub async fn channel_sections_list(&self) -> Result<ChannelSectionsListData> {
        self.post("users.channelSections.list", &[]).await
    }

    pub async fn search_messages(
        &self,
        query: &str,
        page: u32,
        count: u32,
    ) -> Result<SearchMessagesData> {
        let page_str = page.to_string();
        let count_str = count.to_string();
        self.post(
            "search.messages",
            &[
                ("query", query),
                ("page", &page_str),
                ("count", &count_str),
                ("sort", "timestamp"),
                ("sort_dir", "desc"),
            ],
        )
        .await
    }

    pub async fn files_get_upload_url(
        &self,
        filename: &str,
        length: usize,
    ) -> Result<FilesGetUploadURLData> {
        let length_str = length.to_string();
        self.post(
            "files.getUploadURLExternal",
            &[("filename", filename), ("length", &length_str)],
        )
        .await
    }

    pub async fn files_complete_upload(
        &self,
        file_id: &str,
        channel_id: &str,
        thread_ts: Option<&str>,
    ) -> Result<FilesCompleteUploadData> {
        let files_json = serde_json::json!([{"id": file_id}]).to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("files", &files_json),
            ("channel_id", channel_id),
        ];
        if let Some(ts) = thread_ts {
            params.push(("thread_ts", ts));
        }
        self.post("files.completeUploadExternal", &params).await
    }

    pub async fn files_upload(
        &self,
        channel: &str,
        thread_ts: Option<&str>,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<FilesCompleteUploadData> {
        let url_data = self.files_get_upload_url(filename, data.len()).await?;
        let resp = self
            .http
            .post(&url_data.upload_url)
            .body(data)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("HTTP {} uploading file", resp.status());
        }
        self.files_complete_upload(&url_data.file_id, channel, thread_ts)
            .await
    }

    /// Download a file from a Slack private URL (uses cookie auth).
    pub async fn download_file_raw(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            bail!("HTTP {} downloading {}", resp.status(), url);
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}

impl SlackApi for SlackClient {
    async fn conversations_list_all(&self) -> Result<Vec<Channel>> {
        self.conversations_list_all().await
    }

    async fn users_list(&self, cursor: Option<&str>, limit: u32) -> Result<UsersListData> {
        self.users_list(cursor, limit).await
    }

    async fn emoji_list(&self) -> Result<EmojiListData> {
        self.emoji_list().await
    }

    async fn channel_sections_list(&self) -> Result<ChannelSectionsListData> {
        self.channel_sections_list().await
    }

    async fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<ConversationsHistoryData> {
        self.conversations_history(channel, limit, oldest, latest).await
    }

    async fn conversations_replies(
        &self,
        channel: &str,
        thread_ts: &str,
        limit: u32,
    ) -> Result<ConversationsHistoryData> {
        self.conversations_replies(channel, thread_ts, limit).await
    }

    async fn conversations_mark(&self, channel: &str, ts: &str) -> Result<ConversationsMarkData> {
        self.conversations_mark(channel, ts).await
    }

    async fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<ChatPostMessageData> {
        self.chat_post_message(channel, text, thread_ts).await
    }

    async fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<ReactionsData> {
        self.reactions_add(channel, timestamp, name).await
    }

    async fn reactions_remove(
        &self,
        channel: &str,
        timestamp: &str,
        name: &str,
    ) -> Result<ReactionsData> {
        self.reactions_remove(channel, timestamp, name).await
    }

    async fn download_file(&self, url: &str) -> Result<Vec<u8>> {
        self.download_file_raw(url).await
    }

    async fn search_messages(
        &self,
        query: &str,
        page: u32,
        count: u32,
    ) -> Result<SearchMessagesData> {
        self.search_messages(query, page, count).await
    }

    async fn files_upload(
        &self,
        channel: &str,
        thread_ts: Option<&str>,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<FilesCompleteUploadData> {
        self.files_upload(channel, thread_ts, filename, data).await
    }

    fn spawn_realtime(
        &self,
        tx: mpsc::UnboundedSender<RealtimeEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            realtime::run_websocket(client, tx).await;
        })
    }
}
