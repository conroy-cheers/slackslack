use crate::slack::types::{Channel, ChannelSection, User};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

const CACHE_VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
pub struct DiskCache {
    pub version: u32,
    pub team_id: String,
    pub channels: Vec<Channel>,
    pub users: Vec<User>,
    /// channel_id -> timestamp of last known activity
    pub channel_activity: HashMap<String, String>,
    /// Custom emoji: name -> URL or "alias:target"
    #[serde(default)]
    pub custom_emoji: HashMap<String, String>,
    /// User-defined sidebar sections
    #[serde(default)]
    pub channel_sections: Vec<ChannelSection>,
}

impl DiskCache {
    pub fn new(team_id: &str) -> Self {
        Self {
            version: CACHE_VERSION,
            team_id: team_id.to_string(),
            channels: Vec::new(),
            users: Vec::new(),
            channel_activity: HashMap::new(),
            custom_emoji: HashMap::new(),
            channel_sections: Vec::new(),
        }
    }

    pub fn load(team_id: &str) -> Option<Self> {
        let path = cache_path(team_id);
        let data = std::fs::read(&path).ok()?;
        let cache: Self = serde_json::from_slice(&data).ok()?;
        if cache.version != CACHE_VERSION {
            warn!(
                "Cache version mismatch (got {}, want {}), invalidating",
                cache.version, CACHE_VERSION
            );
            let _ = std::fs::remove_file(&path);
            return None;
        }
        if cache.team_id != team_id {
            return None;
        }
        info!(
            "Loaded cache: {} channels, {} users",
            cache.channels.len(),
            cache.users.len()
        );
        Some(cache)
    }

    pub fn save(&self, team_id: &str) {
        let path = cache_path(team_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec(self) {
            Ok(data) => {
                if let Err(e) = std::fs::write(&path, data) {
                    warn!("Failed to write cache: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize cache: {}", e),
        }
    }
}

fn cache_path(team_id: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(format!("{}/.cache/slackslack/{}/cache.json", home, team_id))
}

// --- Standard emoji cache (team-independent) ---

#[derive(Serialize, Deserialize)]
struct StandardEmojiCache {
    version: u32,
    emoji: HashMap<String, String>,
}

const STANDARD_EMOJI_CACHE_VERSION: u32 = 1;

fn standard_emoji_cache_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(format!("{}/.cache/slackslack/standard_emoji.json", home))
}

pub fn load_standard_emoji_cache() -> Option<HashMap<String, String>> {
    let path = standard_emoji_cache_path();
    let data = std::fs::read(&path).ok()?;
    let cache: StandardEmojiCache = serde_json::from_slice(&data).ok()?;
    if cache.version != STANDARD_EMOJI_CACHE_VERSION {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    info!("Loaded {} standard emoji from cache", cache.emoji.len());
    Some(cache.emoji)
}

pub fn save_standard_emoji_cache(emoji: &HashMap<String, String>) {
    let path = standard_emoji_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let cache = StandardEmojiCache {
        version: STANDARD_EMOJI_CACHE_VERSION,
        emoji: emoji.clone(),
    };
    match serde_json::to_vec(&cache) {
        Ok(data) => {
            if let Err(e) = std::fs::write(&path, data) {
                warn!("Failed to write standard emoji cache: {}", e);
            }
        }
        Err(e) => warn!("Failed to serialize standard emoji cache: {}", e),
    }
}
