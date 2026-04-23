#![allow(dead_code)]

mod app;
mod auth;
mod cache;
mod event;
mod slack;
mod state;
#[cfg(test)]
mod testing;
mod ui;

use anyhow::{Context, Result};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (only to file, not to terminal — we own the terminal for TUI)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("slackslack=info".parse().unwrap()),
        )
        .with_writer(|| {
            let log_dir = dirs_log();
            std::fs::create_dir_all(&log_dir).ok();
            let path = log_dir.join("slackslack.log");
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("Failed to open log file")
        })
        .init();

    info!("Starting slackslack...");

    let candidates = auth::credential_candidates()?;
    if candidates.is_empty() {
        anyhow::bail!(
            "No Slack credentials found.\n\
             Tried environment, cached credentials, Slack desktop, and Chromium-family browser profiles."
        );
    }

    let mut selected = None;
    for candidate in candidates {
        info!("Trying Slack credentials from {}", candidate.label);
        let client = match slack::client::SlackClient::new(&candidate.credentials) {
            Ok(client) => client,
            Err(err) => {
                info!("Skipping {}: {}", candidate.label, err);
                continue;
            }
        };
        match client.auth_test().await {
            Ok(auth) => {
                info!("Authenticated with credentials from {}", candidate.label);
                auth::save_credentials(&candidate.credentials).ok();
                selected = Some((client, auth));
                break;
            }
            Err(err) => {
                info!("Credential source {} failed auth_test: {}", candidate.label, err);
            }
        }
    }

    let (client, auth) = selected.context("Failed to authenticate with any discovered Slack credentials")?;

    info!(
        "Authenticated as {} ({}) on {}",
        auth.user, auth.user_id, auth.team
    );
    eprintln!(
        "Authenticated as {} on {}",
        auth.user, auth.team
    );

    // Run the TUI
    let mut app = app::App::new(client, &auth.team_id);
    app.run().await?;

    Ok(())
}

fn dirs_log() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    std::path::PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("slackslack")
}
