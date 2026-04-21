#![allow(dead_code)]

pub mod app;
pub mod cache;
pub mod event;
pub mod state;
pub mod ui;

#[cfg(test)]
mod testing;

mod auth {
    pub use libslack::auth::*;
}

mod slack {
    pub use libslack::client;
    pub use libslack::realtime;
    pub use libslack::types;
}

use anyhow::Result;
use tracing::info;

pub async fn run() -> Result<()> {
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

    let creds = if let (Ok(token), Ok(cookie)) =
        (std::env::var("SLACK_TOKEN"), std::env::var("SLACK_COOKIE"))
    {
        info!("Using credentials from environment variables");
        auth::Credentials { token, cookie }
    } else {
        info!("Extracting credentials from Slack desktop app...");
        auth::extract_credentials()?
    };

    let client = slack::client::SlackClient::new(&creds)?;
    let auth = client.auth_test().await?;

    info!(
        "Authenticated as {} ({}) on {}",
        auth.user, auth.user_id, auth.team
    );
    eprintln!("Authenticated as {} on {}", auth.user, auth.team);

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
