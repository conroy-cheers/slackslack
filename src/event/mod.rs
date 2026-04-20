pub mod handler;

use crate::slack::types::WsEvent;
use crossterm::event::KeyEvent;
use tokio::sync::mpsc;

pub enum Event {
    // Terminal
    Key(KeyEvent),
    Resize(u16, u16),
    Mouse(crossterm::event::MouseEvent),

    // Slack real-time
    SlackConnected { self_id: String, team: String },
    SlackDisconnected,
    SlackWsEvent(WsEvent),

    // WebSocket plumbing
    WsPing(u64),
    WsWriterReady(mpsc::UnboundedSender<String>),

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
    ApiError(String),

    // Internal
    Tick,
}
