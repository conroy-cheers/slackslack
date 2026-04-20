use serde::{Deserialize, Serialize};

// === Identifiers ===

pub type UserId = String;
pub type ChannelId = String;
pub type Timestamp = String;

// === API Response Wrapper ===

#[derive(Debug, Deserialize)]
pub struct SlackResponse<T> {
    pub ok: bool,
    pub error: Option<String>,
    #[serde(flatten)]
    pub data: T,
}

// === auth.test ===

#[derive(Debug, Deserialize, Default)]
pub struct AuthTestData {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub url: String,
}

// === conversations.list ===

#[derive(Debug, Deserialize, Default)]
pub struct ConversationsListData {
    #[serde(default)]
    pub channels: Vec<Channel>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: ChannelId,
    pub name: Option<String>,
    #[serde(default)]
    pub is_channel: bool,
    #[serde(default)]
    pub is_im: bool,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_member: bool,
    pub user: Option<UserId>,
    pub topic: Option<TopicOrPurpose>,
    pub purpose: Option<TopicOrPurpose>,
    pub last_read: Option<Timestamp>,
    #[serde(default)]
    pub unread_count: u32,
    #[serde(default)]
    pub unread_count_display: u32,
}

impl Channel {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("unknown")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicOrPurpose {
    pub value: String,
}

// === conversations.history ===

#[derive(Debug, Deserialize, Default)]
pub struct ConversationsHistoryData {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub has_more: bool,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub user: Option<UserId>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub ts: Timestamp,
    pub thread_ts: Option<Timestamp>,
    pub reply_count: Option<u32>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    pub edited: Option<Edited>,
    pub subtype: Option<String>,
    pub bot_id: Option<String>,
    pub username: Option<String>,
    #[serde(default)]
    pub files: Vec<SlackFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackFile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub mimetype: Option<String>,
    pub filetype: Option<String>,
    pub url_private: Option<String>,
    pub thumb_360: Option<String>,
    pub thumb_480: Option<String>,
    pub thumb_160: Option<String>,
    #[serde(default)]
    pub thumb_360_w: u32,
    #[serde(default)]
    pub thumb_360_h: u32,
}

impl SlackFile {
    /// Return the best thumbnail URL available.
    pub fn best_thumb_url(&self) -> Option<&str> {
        self.thumb_480
            .as_deref()
            .or(self.thumb_360.as_deref())
            .or(self.thumb_160.as_deref())
    }

    /// Is this file an image?
    pub fn is_image(&self) -> bool {
        self.mimetype
            .as_deref()
            .map(|m| m.starts_with("image/"))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub name: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub users: Vec<UserId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edited {
    pub user: Option<UserId>,
    pub ts: Option<Timestamp>,
}

// === users.list / users.info ===

#[derive(Debug, Deserialize, Default)]
pub struct UsersListData {
    #[serde(default)]
    pub members: Vec<User>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UserInfoData {
    pub user: Option<User>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    #[serde(default)]
    pub name: String,
    pub real_name: Option<String>,
    pub profile: Option<UserProfile>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub deleted: bool,
    pub color: Option<String>,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.profile
            .as_ref()
            .and_then(|p| p.display_name.as_deref())
            .filter(|s| !s.is_empty())
            .or(self.real_name.as_deref())
            .unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub display_name: Option<String>,
    pub real_name: Option<String>,
    pub image_48: Option<String>,
}

// === rtm.connect ===

#[derive(Debug, Deserialize, Default)]
pub struct RtmConnectData {
    #[serde(default)]
    pub url: String,
    #[serde(rename = "self", default)]
    pub self_info: RtmSelf,
    #[serde(default)]
    pub team: RtmTeam,
}

#[derive(Debug, Deserialize, Default)]
pub struct RtmSelf {
    #[serde(default)]
    pub id: UserId,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct RtmTeam {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub domain: String,
}

// === chat.postMessage ===

#[derive(Debug, Deserialize, Default)]
pub struct ChatPostMessageData {
    pub ts: Option<Timestamp>,
    pub channel: Option<ChannelId>,
    pub message: Option<Message>,
}

// === reactions ===

#[derive(Debug, Deserialize, Default)]
pub struct ReactionsData {}

// === conversations.mark ===

#[derive(Debug, Deserialize, Default)]
pub struct ConversationsMarkData {}

// === files.getUploadURLExternal ===

#[derive(Debug, Deserialize, Default)]
pub struct FilesGetUploadURLData {
    #[serde(default)]
    pub upload_url: String,
    #[serde(default)]
    pub file_id: String,
}

// === files.completeUploadExternal ===

#[derive(Debug, Deserialize, Default)]
pub struct FilesCompleteUploadData {
    #[serde(default)]
    pub files: Vec<SlackFile>,
}

// === emoji.list ===

#[derive(Debug, Deserialize, Default)]
pub struct EmojiListData {
    #[serde(default)]
    pub emoji: std::collections::HashMap<String, String>, // name -> URL or "alias:target"
}

// === Channel sections (undocumented API) ===

#[derive(Debug, Deserialize, Default)]
pub struct ChannelSectionsListData {
    #[serde(default)]
    pub channel_sections: Vec<ChannelSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSection {
    #[serde(default)]
    pub channel_section_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub channel_ids_page: ChannelIdsPage,
    #[serde(default)]
    pub is_collapsed: bool,
    #[serde(default)]
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelIdsPage {
    #[serde(default)]
    pub channel_ids: Vec<ChannelId>,
}

// === Shared ===

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMetadata {
    pub next_cursor: Option<String>,
}

// === WebSocket Events ===

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "hello")]
    Hello,
    #[serde(rename = "goodbye")]
    Goodbye,
    #[serde(rename = "message")]
    Message(WsMessage),
    #[serde(rename = "reaction_added")]
    ReactionAdded(WsReaction),
    #[serde(rename = "reaction_removed")]
    ReactionRemoved(WsReaction),
    #[serde(rename = "channel_marked")]
    ChannelMarked(WsChannelMarked),
    #[serde(rename = "user_typing")]
    UserTyping(WsUserTyping),
    #[serde(rename = "presence_change")]
    PresenceChange(WsPresenceChange),
    #[serde(rename = "error")]
    Error(WsError),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsError {
    pub error: Option<WsErrorDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsErrorDetail {
    pub msg: Option<String>,
    pub code: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsMessage {
    pub channel: Option<ChannelId>,
    pub user: Option<UserId>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub ts: Timestamp,
    pub thread_ts: Option<Timestamp>,
    pub subtype: Option<String>,
    // For message_changed subtypes
    pub message: Option<Box<WsMessage>>,
    pub previous_message: Option<Box<WsMessage>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsReaction {
    pub user: Option<UserId>,
    pub reaction: Option<String>,
    pub item: Option<WsReactionItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsReactionItem {
    pub channel: Option<ChannelId>,
    pub ts: Option<Timestamp>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsChannelMarked {
    pub channel: Option<ChannelId>,
    pub ts: Option<Timestamp>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsUserTyping {
    pub channel: Option<ChannelId>,
    pub user: Option<UserId>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsPresenceChange {
    pub user: Option<UserId>,
    pub presence: Option<String>,
}

// === search.messages ===

#[derive(Debug, Deserialize, Default)]
pub struct SearchMessagesData {
    #[serde(default)]
    pub messages: SearchMessages,
}

#[derive(Debug, Deserialize, Default)]
pub struct SearchMessages {
    #[serde(default)]
    pub matches: Vec<SearchMatch>,
    pub paging: Option<SearchPaging>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatch {
    pub text: String,
    pub ts: Timestamp,
    pub user: Option<UserId>,
    pub username: Option<String>,
    pub channel: Option<SearchChannel>,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchChannel {
    pub id: ChannelId,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPaging {
    pub count: Option<u32>,
    pub total: Option<u32>,
    pub page: Option<u32>,
    pub pages: Option<u32>,
}
