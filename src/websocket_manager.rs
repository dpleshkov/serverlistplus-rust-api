use std::sync::{Arc, RwLock};

use futures_util::{SinkExt, StreamExt};
use hyper_tungstenite::HyperWebsocket;
use hyper_tungstenite::hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::{Error, Message};
use serde_json;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::WebSocketStream;
use serde::{Deserialize};
use crate::listener::{ListenerRequest, ListenerResponse, Listener};

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

async fn pull_subscribe_message(next: Option<Result<Message, Error>>, listeners: Arc<ListenerManager>) -> Option<Listener> {
    if let Some(result) = next {
        if let Ok(message) = result {
            if let Message::Text(text) = message {
                if let Ok(subscribe_message) = serde_json::from_str::<SocketMessage>(text.as_str()) {
                    if let Some(listener) = listeners.
                }
            }

        }
    }
}
async fn manage_ws(mut ws: WebSocketStream<Upgraded>, listeners: Arc<ListenerManager>) -> Result<(), Error> {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let listener_rx;
    match ws_rx.next().await {
        None => {
            return Ok(());
        }
        Some(res) => {
            if let Message::Text(data) = res? {
                if let Ok(msg) = serde_json::from_str::<SocketMessage>(data.as_str()) {
                    if msg.name.as_str() == "subscribe" {

                    }
                }
            }
        }
    }

    loop {
        tokio::select! {
            next = ws_rx.next() => {
                match next {
                    None => {
                        return Ok(());
                    }
                    Some(maybe_message) => {
                        let message = maybe_message?;

                    }
                }
            }
        }
    }
}