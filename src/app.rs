use crate::cache::DiskCache;
use crate::event::handler::{HandleResult, handle_event};
use crate::event::Event;
use crate::slack::client::SlackClient;
use crate::slack::websocket;
use crate::state::AppState;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self as ct_event, EventStream};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct App {
    state: AppState,
    client: SlackClient,
    event_tx: mpsc::UnboundedSender<Event>,
    event_rx: mpsc::UnboundedReceiver<Event>,
    team_id: String,
}

impl App {
    pub fn new(client: SlackClient, team_id: &str) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut state = AppState::new();
        state.team_id = team_id.to_string();

        // Load from disk cache for instant startup
        if let Some(cache) = DiskCache::load(team_id) {
            state.channel_activity = cache.channel_activity;
            if !cache.channels.is_empty() {
                state.set_channels(cache.channels);
                info!("Restored {} channels from cache", state.channels.len());
            }
            for user in cache.users {
                state.user_cache.insert(user.id.clone(), user);
            }
            if !cache.custom_emoji.is_empty() {
                state.custom_emoji = cache.custom_emoji;
                info!("Restored {} custom emoji from cache", state.custom_emoji.len());
            }
            if !cache.channel_sections.is_empty() {
                state.channel_sections = cache.channel_sections;
                info!("Restored {} channel sections from cache", state.channel_sections.len());
            }
            state.dirty = true;
        }

        Self {
            state,
            client,
            event_tx,
            event_rx,
            team_id: team_id.to_string(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Install panic hook to restore terminal before printing panic
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            ratatui::restore();
            original_hook(panic_info);
        }));

        let mut terminal = ratatui::init();

        // Spawn terminal input reader
        let input_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(ct_event::Event::Key(key))) => {
                        if input_tx.send(Event::Key(key)).is_err() {
                            break;
                        }
                    }
                    Some(Ok(ct_event::Event::Resize(w, h))) => {
                        let _ = input_tx.send(Event::Resize(w, h));
                    }
                    Some(Err(e)) => {
                        error!("Terminal event error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        });

        // Spawn WebSocket task
        let ws_client = self.client.clone();
        let ws_tx = self.event_tx.clone();
        tokio::spawn(async move {
            websocket::run_websocket(ws_client, ws_tx).await;
        });

        // Load fresh data from API (replaces cache data when it arrives)
        self.spawn_initial_loads();

        // Main event loop
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));

        loop {
            tokio::select! {
                Some(event) = self.event_rx.recv() => {
                    let mut should_quit = false;
                    let result = handle_event(event, &mut self.state, &self.client, &self.event_tx);
                    if matches!(result, HandleResult::Quit) { should_quit = true; }

                    // Drain any pending events before rendering
                    if !should_quit {
                        while let Ok(event) = self.event_rx.try_recv() {
                            let result = handle_event(event, &mut self.state, &self.client, &self.event_tx);
                            if matches!(result, HandleResult::Quit) { should_quit = true; break; }
                        }
                    }

                    if should_quit { break; }

                    // Render immediately — don't wait for tick
                    Self::render_frame(&mut self.state, &mut terminal)?;
                    crate::event::handler::process_emoji_load_queue(&mut self.state, &self.client, &self.event_tx);
                }
                _ = tick.tick() => {
                    // Periodic maintenance
                    self.state.expire_typing();
                    Self::render_frame(&mut self.state, &mut terminal)?;
                    crate::event::handler::process_emoji_load_queue(&mut self.state, &self.client, &self.event_tx);
                }
            }
        }

        // Save cache on exit
        self.save_cache();

        ratatui::restore();
        Ok(())
    }

    fn render_frame(
        state: &mut AppState,
        terminal: &mut ratatui::DefaultTerminal,
    ) -> Result<()> {
        if !state.dirty {
            return Ok(());
        }

        let frame_start = std::time::Instant::now();

        if state.channels_need_resort {
            state.resort_channels();
            state.channels_need_resort = false;
        }

        terminal.draw(|frame| ui::render(frame, state))?;
        state.dirty = false;
        state.last_frame_time = frame_start.elapsed();
        state.frame_count += 1;

        // Post-frame writes (images, clipboard)
        let has_modal = state.show_help || state.input_mode == crate::state::InputMode::EmojiPicker;
        {
            use std::io::Write;
            let mut buf: Vec<u8> = Vec::new();

            if has_modal || state.image_placements.is_empty() {
                ui::images::clear_images(&mut buf)?;
            } else {
                ui::images::clear_images(&mut buf)?;
                ui::images::render_visible_images(&mut buf, state)?;
            }

            if let Some(text) = state.clipboard_pending.take() {
                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    text.as_bytes(),
                );
                write!(buf, "\x1b]52;c;{}\x07", b64)?;
            }

            if !buf.is_empty() {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(&buf)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }

    fn spawn_initial_loads(&self) {
        // Load channels
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            match client.conversations_list_all().await {
                Ok(channels) => {
                    info!("Loaded {} channels", channels.len());
                    let _ = tx.send(Event::ChannelsLoaded(channels));
                }
                Err(e) => {
                    error!("Failed to load channels: {}", e);
                    let _ = tx.send(Event::ApiError(format!("Channels: {}", e)));
                }
            }
        });

        // Load users
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            match client.users_list(None, 200).await {
                Ok(data) => {
                    info!("Loaded {} users", data.members.len());
                    let _ = tx.send(Event::UsersLoaded(data.members));
                }
                Err(e) => {
                    error!("Failed to load users: {}", e);
                    let _ = tx.send(Event::ApiError(format!("Users: {}", e)));
                }
            }
        });

        // Load custom emoji
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            match client.emoji_list().await {
                Ok(data) => {
                    info!("Loaded {} custom emoji", data.emoji.len());
                    let _ = tx.send(Event::CustomEmojiLoaded(data.emoji));
                }
                Err(e) => {
                    error!("Failed to load emoji: {}", e);
                    let _ = tx.send(Event::ApiError(format!("Emoji: {}", e)));
                }
            }
        });

        // Load channel sections
        let client = self.client.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            match client.channel_sections_list().await {
                Ok(data) => {
                    info!("Loaded {} channel sections", data.channel_sections.len());
                    let _ = tx.send(Event::ChannelSectionsLoaded(data.channel_sections));
                }
                Err(e) => {
                    // Not fatal — sections are optional (may not exist for all workspaces)
                    error!("Failed to load channel sections: {}", e);
                }
            }
        });
    }

    fn save_cache(&self) {
        let mut cache = DiskCache::new(&self.team_id);
        cache.channels = self.state.channels.clone();
        cache.users = self.state.user_cache.values().cloned().collect();
        cache.channel_activity = self.state.channel_activity.clone();
        cache.custom_emoji = self.state.custom_emoji.clone();
        cache.channel_sections = self.state.channel_sections.clone();
        cache.save(&self.team_id);
        info!("Saved cache");
    }
}
