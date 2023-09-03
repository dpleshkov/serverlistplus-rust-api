use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use hyper_tungstenite::hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::{Error, Message};
use serde::Deserialize;
use serde_json;
use serde_json::json;
use tokio_tungstenite::WebSocketStream;

use crate::listener::Listener;
use crate::listener_manager::ListenerManager;

#[derive(Deserialize)]
struct MessageData {
    id: String
}

#[derive(Deserialize)]
struct SocketMessage {
    name: String,
    data: MessageData
}

async fn pull_subscribe_message(next: Option<Result<Message, Error>>, listeners: Arc<ListenerManager>) -> Option<Arc<Listener>> {
    if let Some(result) = next {
        if let Ok(message) = result {
            if let Message::Text(text) = message {
                if let Ok(subscribe_message) = serde_json::from_str::<SocketMessage>(text.as_str()) {
                    if let Some(listener) = listeners.get_listener(subscribe_message.data.id).await {
                        return Some(listener);
                    }
                }
            }
        }
    }
    None
}
pub async fn manage_ws(ws: WebSocketStream<Upgraded>, listeners: Arc<ListenerManager>) -> Result<(), Error> {
    let (mut ws_tx, mut ws_rx) = ws.split();

    // Might be a more elegant way to write this out
    let maybe_listener = pull_subscribe_message(ws_rx.next().await, Arc::clone(&listeners)).await;
    if maybe_listener.is_none() {
        return Ok(());
    }
    let listener = maybe_listener.unwrap();

    let maybe_listener_rx = listener.subscribe().await;
    if maybe_listener_rx.is_none() {
        return Ok(());
    }
    let mut listener_rx = maybe_listener_rx.unwrap();

    let maybe_game_state = listener.get_game_state_json().await;
    if maybe_game_state.is_none() {
        return Ok(());
    }
    let game_state = maybe_game_state.unwrap();

    ws_tx.send(Message::Text(json!({
        "name": "mode_info",
        "data": game_state
    }).to_string())).await?;

    loop {
        tokio::select! {
            next = ws_rx.next() => {
                match next {
                    None => {
                        return Ok(());
                    }
                    Some(res) => {
                        if let Message::Text(data) = res? {
                            if let Ok(msg) = serde_json::from_str::<SocketMessage>(data.as_str()) {
                                if msg.name.as_str() == "get_name" {
                                    let id: u8 = msg.data.id.parse().unwrap_or(0);
                                    if let Some(player) = listener.get_name(id).await {
                                        ws_tx.send(Message::Text(player)).await?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            maybe_info = listener_rx.recv() => {
                match maybe_info {
                    Err(_) => {
                        return Ok(());
                    }
                    Ok(info) => {
                        ws_tx.send(Message::Binary(info)).await?;
                    }
                }
            }
        }
    }
}