mod browser;
mod cookie;
mod token;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Credentials {
    pub token: String,
    pub cookie: String,
}

#[derive(Debug, Clone)]
pub struct CredentialCandidate {
    pub label: String,
    pub credentials: Credentials,
}

pub fn credential_candidates() -> Result<Vec<CredentialCandidate>> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if let (Ok(token), Ok(cookie)) = (std::env::var("SLACK_TOKEN"), std::env::var("SLACK_COOKIE")) {
        push_candidate(
            &mut candidates,
            &mut seen,
            "environment".to_string(),
            Credentials { token, cookie },
        );
    }

    if let Ok(creds) = load_cached_credentials() {
        push_candidate(
            &mut candidates,
            &mut seen,
            "cached credentials".to_string(),
            creds,
        );
    }

    if let Ok(creds) = extract_desktop_credentials() {
        push_candidate(
            &mut candidates,
            &mut seen,
            "Slack desktop".to_string(),
            creds,
        );
    }

    for profile in browser::browser_profiles() {
        let token = token::extract_token(&profile.local_storage);
        let cookie = cookie::extract_cookie(&profile.cookies);
        if let (Ok(token), Ok(cookie)) = (token, cookie) {
            push_candidate(
                &mut candidates,
                &mut seen,
                profile.label,
                Credentials { token, cookie },
            );
        }
    }

    Ok(candidates)
}

pub fn extract_credentials() -> Result<Credentials> {
    credential_candidates()?
        .into_iter()
        .next()
        .map(|candidate| candidate.credentials)
        .context("No Slack credentials found from environment, cache, Slack desktop, or browser profiles")
}

pub fn save_credentials(creds: &Credentials) -> Result<()> {
    let path = credentials_cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(creds)?;
    std::fs::write(path, data)?;
    Ok(())
}

fn load_cached_credentials() -> Result<Credentials> {
    let path = credentials_cache_path();
    let data = std::fs::read(&path)
        .with_context(|| format!("Failed to read cached credentials from {}", path.display()))?;
    let creds = serde_json::from_slice(&data)
        .with_context(|| format!("Failed to parse cached credentials from {}", path.display()))?;
    Ok(creds)
}

fn extract_desktop_credentials() -> Result<Credentials> {
    let slack_config = desktop_dirs();

    let token = token::extract_token(&slack_config.local_storage)
        .context("Failed to extract xoxc token from Slack local storage")?;

    let cookie = cookie::extract_cookie(&slack_config.cookies)
        .context("Failed to extract session cookie from Slack cookies")?;

    Ok(Credentials { token, cookie })
}

fn push_candidate(
    out: &mut Vec<CredentialCandidate>,
    seen: &mut HashSet<Credentials>,
    label: String,
    credentials: Credentials,
) {
    if seen.insert(credentials.clone()) {
        out.push(CredentialCandidate { label, credentials });
    }
}

struct SlackDirs {
    local_storage: std::path::PathBuf,
    cookies: std::path::PathBuf,
}

fn desktop_dirs() -> SlackDirs {
    let base = desktop_dirs_base();
    SlackDirs {
        local_storage: base.join("Local Storage").join("leveldb"),
        cookies: base.join("Cookies"),
    }
}

fn desktop_dirs_base() -> std::path::PathBuf {
    if let Ok(config) = std::env::var("SLACK_CONFIG_DIR") {
        return std::path::PathBuf::from(config);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    std::path::PathBuf::from(home)
        .join(".config")
        .join("Slack")
}

fn credentials_cache_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    std::path::PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("slackslack")
        .join("credentials.json")
}
