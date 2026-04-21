mod cookie;
mod token;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Credentials {
    pub token: String,
    pub cookie: String,
}

pub fn extract_credentials() -> Result<Credentials> {
    let slack_config = dirs();

    let token = token::extract_token(&slack_config.local_storage)
        .context("Failed to extract xoxc token from Slack local storage")?;

    let cookie = cookie::extract_cookie(&slack_config.cookies)
        .context("Failed to extract session cookie from Slack cookies")?;

    Ok(Credentials { token, cookie })
}

struct SlackDirs {
    local_storage: std::path::PathBuf,
    cookies: std::path::PathBuf,
}

fn dirs() -> SlackDirs {
    let base = dirs_base();
    SlackDirs {
        local_storage: base.join("Local Storage").join("leveldb"),
        cookies: base.join("Cookies"),
    }
}

fn dirs_base() -> std::path::PathBuf {
    if let Ok(config) = std::env::var("SLACK_CONFIG_DIR") {
        return std::path::PathBuf::from(config);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    std::path::PathBuf::from(home)
        .join(".config")
        .join("Slack")
}
