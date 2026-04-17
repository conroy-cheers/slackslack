use crate::event::Event;
use crate::slack::client::SlackClient;
use crate::slack::types::WsEvent;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::http;
use tracing::{error, info, warn};

pub async fn run_websocket(client: SlackClient, tx: mpsc::UnboundedSender<Event>) {
    let mut backoff_secs = 1u64;

    loop {
        match connect_and_run(&client, &tx).await {
            Ok(()) => {
                info!("WebSocket closed cleanly, reconnecting...");
                backoff_secs = 1;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                let _ = tx.send(Event::SlackDisconnected);
            }
        }

        info!("Reconnecting in {}s...", backoff_secs);
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);
    }
}

async fn connect_and_run(
    client: &SlackClient,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<()> {
    let rtm = client.rtm_connect().await?;
    info!("RTM connected, url obtained");

    // Build a request with the cookie header — Slack requires auth on the WS too
    let request = http::Request::builder()
        .uri(&rtm.url)
        .header("Cookie", format!("d={}", client.cookie))
        .header("Host", "wss-primary.slack.com")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;

    let _ = tx.send(Event::SlackConnected {
        self_id: rtm.self_info.id.clone(),
        team: rtm.team.name.clone(),
    });

    let (mut write, mut read) = ws_stream.split();

    let ping_tx = tx.clone();
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut id = 1u64;
        loop {
            interval.tick().await;
            let _ = ping_tx.send(Event::WsPing(id));
            id += 1;
        }
    });

    // Channel for outgoing WebSocket messages
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<String>();

    // Forward pings and outgoing messages to the WebSocket write half
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = ws_rx.recv().await {
            if write
                .send(tungstenite::Message::Text(msg.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Store ws_tx in the event for ping forwarding
    let _ = tx.send(Event::WsWriterReady(ws_tx));

    while let Some(msg) = read.next().await {
        match msg {
            Ok(tungstenite::Message::Text(text)) => {
                let text_str: &str = &text;
                match serde_json::from_str::<WsEvent>(text_str) {
                    Ok(WsEvent::Goodbye) => {
                        info!("Received goodbye, reconnecting...");
                        break;
                    }
                    Ok(WsEvent::Error(ws_err)) => {
                        let msg = ws_err
                            .error
                            .as_ref()
                            .and_then(|e| e.msg.as_deref())
                            .unwrap_or("unknown");
                        error!("WebSocket error from Slack: {}", msg);
                        let _ = tx.send(Event::ApiError(format!("WS: {}", msg)));
                        break;
                    }
                    Ok(event) => {
                        let _ = tx.send(Event::SlackWsEvent(event));
                    }
                    Err(e) => {
                        // Log unknown event types but don't crash
                        warn!("Unknown WS event: {} (raw: {})", e, &text[..text.len().min(200)]);
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                info!("WebSocket close frame received");
                break;
            }
            Err(e) => {
                error!("WebSocket read error: {}", e);
                break;
            }
            _ => {}
        }
    }

    ping_handle.abort();
    write_handle.abort();
    let _ = tx.send(Event::SlackDisconnected);
    Ok(())
}
