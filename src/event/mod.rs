pub mod handler;

use crate::slack::realtime::RealtimeEvent;
use crate::slack::types::WsEvent;
use crossterm::event::KeyEvent;

pub enum Event {
    // Terminal
    Key(KeyEvent),
    Resize(u16, u16),
    Mouse(crossterm::event::MouseEvent),

    // Slack real-time
    SlackConnected {
        self_id: String,
        team: String,
    },
    SlackDisconnected,
    SlackWsEvent(WsEvent),

    // API results
    ChannelsLoaded(Vec<crate::slack::types::Channel>),
    HistoryLoaded {
        channel_id: String,
        messages: Vec<crate::slack::types::Message>,
        has_more: bool,
    },
    OlderHistoryLoaded {
        channel_id: String,
        messages: Vec<crate::slack::types::Message>,
        has_more: bool,
    },
    ThreadLoaded {
        channel_id: String,
        thread_ts: String,
        messages: Vec<crate::slack::types::Message>,
    },
    UsersLoaded(Vec<crate::slack::types::User>),
    MessageSent {
        channel_id: String,
        ts: String,
    },
    ChannelMarked {
        channel_id: String,
    },
    ImageLoaded {
        url: String,
        png_data: Vec<u8>,
        width: u32,
        height: u32,
    },
    CustomEmojiLoaded(std::collections::HashMap<String, String>),
    StandardEmojiLoaded(std::collections::HashMap<String, String>),
    ChannelSectionsLoaded(Vec<crate::slack::types::ChannelSection>),
    CustomEmojiImageLoaded {
        name: String,
        png_data: Vec<u8>,
        width: u32,
        height: u32,
    },
    CustomEmojiImageFailed {
        name: String,
    },
    AvatarImageLoaded {
        user_id: String,
        png_data: Vec<u8>,
        width: u32,
        height: u32,
    },
    AvatarImageFailed {
        user_id: String,
    },
    FileUploaded {
        channel_id: String,
        filename: String,
    },
    ApiError(String),
    SearchResultsLoaded {
        query: String,
        matches: Vec<crate::slack::types::SearchMatch>,
        total: u32,
    },

    EmojiPreviewImageLoaded {
        frames: Vec<Vec<[u8; 4]>>,
        frame_delays: Vec<u32>,
        width: u32,
        height: u32,
    },

    // Internal
    Tick,
}

impl From<RealtimeEvent> for Event {
    fn from(value: RealtimeEvent) -> Self {
        match value {
            RealtimeEvent::Connected { self_id, team } => Self::SlackConnected { self_id, team },
            RealtimeEvent::Disconnected => Self::SlackDisconnected,
            RealtimeEvent::WsEvent(event) => Self::SlackWsEvent(event),
            RealtimeEvent::ApiError(err) => Self::ApiError(err),
        }
    }
}
