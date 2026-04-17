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
            warn!("Cache version mismatch (got {}, want {}), invalidating", cache.version, CACHE_VERSION);
            let _ = std::fs::remove_file(&path);
            return None;
        }
        if cache.team_id != team_id {
            return None;
        }
        info!("Loaded cache: {} channels, {} users", cache.channels.len(), cache.users.len());
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
    PathBuf::from(format!(
        "{}/.cache/slackslack/{}/cache.json",
        home, team_id
    ))
}
