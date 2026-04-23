use crate::client::SlackClient;
use crate::types::WsEvent;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::http;
use tracing::{error, info, warn};

#[derive(Debug)]
pub enum RealtimeEvent {
    Connected { self_id: String, team: String },
    Disconnected,
    WsEvent(WsEvent),
    ApiError(String),
}

pub async fn run_websocket(client: SlackClient, tx: mpsc::UnboundedSender<RealtimeEvent>) {
    let mut backoff_secs = 1u64;

    loop {
        match connect_and_run(&client, &tx).await {
            Ok(()) => {
                info!("WebSocket closed cleanly, reconnecting...");
                backoff_secs = 1;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                let _ = tx.send(RealtimeEvent::Disconnected);
            }
        }

        info!("Reconnecting in {}s...", backoff_secs);
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);
    }
}

async fn connect_and_run(
    client: &SlackClient,
    tx: &mpsc::UnboundedSender<RealtimeEvent>,
) -> Result<()> {
    let rtm = client.rtm_connect().await?;
    info!("RTM connected, url obtained");

    let request = http::Request::builder()
        .uri(&rtm.url)
        .header("Cookie", format!("d={}", client.cookie()))
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

    let _ = tx.send(RealtimeEvent::Connected {
        self_id: rtm.self_info.id.clone(),
        team: rtm.team.name.clone(),
    });

    let (mut write, mut read) = ws_stream.split();
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<String>();

    let ping_writer = ws_tx.clone();
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut id = 1u64;
        loop {
            interval.tick().await;
            let msg = serde_json::json!({"id": id, "type": "ping"}).to_string();
            if ping_writer.send(msg).is_err() {
                break;
            }
            id += 1;
        }
    });

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

    while let Some(msg) = read.next().await {
        match msg {
            Ok(tungstenite::Message::Text(text)) => {
                let text_str: &str = &text;
                match decode_text_event(text_str) {
                    DecodedTextEvent::Goodbye => {
                        info!("Received goodbye, reconnecting...");
                        break;
                    }
                    DecodedTextEvent::ApiError(msg) => {
                        error!("WebSocket error from Slack: {}", msg);
                        let _ = tx.send(RealtimeEvent::ApiError(format!("WS: {}", msg)));
                        break;
                    }
                    DecodedTextEvent::Event(event) => {
                        let _ = tx.send(RealtimeEvent::WsEvent(event));
                    }
                    DecodedTextEvent::Unknown(err) => {
                        warn!(
                            "Unknown WS event: {} (raw: {})",
                            err,
                            &text[..text.len().min(200)]
                        );
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
    let _ = tx.send(RealtimeEvent::Disconnected);
    Ok(())
}

enum DecodedTextEvent {
    Goodbye,
    ApiError(String),
    Event(WsEvent),
    Unknown(serde_json::Error),
}

fn decode_text_event(text: &str) -> DecodedTextEvent {
    match serde_json::from_str::<WsEvent>(text) {
        Ok(WsEvent::Goodbye) => DecodedTextEvent::Goodbye,
        Ok(WsEvent::Error(ws_err)) => {
            let msg = ws_err
                .error
                .as_ref()
                .and_then(|e| e.msg.as_deref())
                .unwrap_or("unknown")
                .to_string();
            DecodedTextEvent::ApiError(msg)
        }
        Ok(event) => DecodedTextEvent::Event(event),
        Err(e) => DecodedTextEvent::Unknown(e),
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodedTextEvent, decode_text_event};
    use crate::types::WsEvent;

    #[test]
    fn decodes_goodbye_event() {
        assert!(matches!(
            decode_text_event(r#"{"type":"goodbye"}"#),
            DecodedTextEvent::Goodbye
        ));
    }

    #[test]
    fn decodes_error_event_message() {
        match decode_text_event(r#"{"type":"error","error":{"msg":"boom"}}"#) {
            DecodedTextEvent::ApiError(msg) => assert_eq!(msg, "boom"),
            other => panic!(
                "expected api error, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn decodes_message_event() {
        match decode_text_event(r#"{"type":"message","channel":"C1","text":"hi","ts":"1.0"}"#) {
            DecodedTextEvent::Event(WsEvent::Message(msg)) => {
                assert_eq!(msg.channel.as_deref(), Some("C1"));
                assert_eq!(msg.text, "hi");
            }
            other => panic!(
                "expected ws message, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn decodes_presence_change_event() {
        match decode_text_event(r#"{"type":"presence_change","user":"U1","presence":"active"}"#) {
            DecodedTextEvent::Event(WsEvent::PresenceChange(event)) => {
                assert_eq!(event.user.as_deref(), Some("U1"));
                assert_eq!(event.presence.as_deref(), Some("active"));
            }
            other => panic!(
                "expected presence change, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }
}
