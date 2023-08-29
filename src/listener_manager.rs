use futures_util::StreamExt;
use crate::listener;
use crate::http_utils;
use warp::ws::WebSocket;
use tokio::sync::{mpsc};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use std::collections::HashMap;

#[derive(Deserialize)]
struct MessageData {
    id: String
}

#[derive(Deserialize)]
struct SocketMessage {
    name: String,
    data: MessageData
}

async fn manage_ws(mut ws: WebSocket) {
    loop {
        match ws.next().await {
            None => {
                return;
            }
            Some(maybe_message) => {
                let message = maybe_message.expect("websocket error");
                if message.is_close() {
                    return;
                } else if message.is_text() {
                    let maybe_msg = serde_json::from_str::<SocketMessage>(&message.to_str().unwrap());
                    if maybe_msg.is_err() {
                        ws.close().await.expect("can't close ws????");
                        return;
                    }
                    let msg = maybe_msg.unwrap();
                    if msg.name == "subscribe" {

                    }
                }
            }
        }
    }
}

async fn manager(mut ws_rx: mpsc::Receiver<WebSocket>) {
    loop {
        match ws_rx.recv().await {
            None => {
                return;
            }
            Some(ws) => {
                tokio::spawn(manage_ws(ws));
            }
        }
    }
}

pub async fn start() -> mpsc::Sender<WebSocket> {
    let (ws_tx, ws_rx) = mpsc::channel::<WebSocket>(1);

    tokio::spawn(manager(ws_rx));

    return ws_tx;
}