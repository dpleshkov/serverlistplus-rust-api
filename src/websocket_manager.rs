use std::sync::{Arc, RwLock};

use futures_util::{SinkExt, StreamExt};
use hyper_tungstenite::HyperWebsocket;
use hyper_tungstenite::hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::{Error, Message};
use serde_json;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;

#[derive(Deserialize)]
struct MessageData {
    id: String
}

#[derive(Deserialize)]
struct SocketMessage {
    name: String,
    data: MessageData
}

async fn manage_ws(mut ws: WebSocketStream<Upgraded>, listeners: Arc<RwLock<Listeners>>) -> Result<(), Error> {
    let (mut ws_tx, mut ws_rx) = ws.split();

    match ws_rx.next().await {
        None => {
            return Ok(());
        }
        Some(res) => {
            if let Message::Text(msg) = res? {

            } else {
                ws_tx.close().await?;
                return Ok(());
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

async fn manager(mut ws_rx: mpsc::Receiver<HyperWebsocket>) {
    loop {
        match ws_rx.recv().await {
            None => {
                return;
            }
            Some(ws) => {
                // tokio::spawn(manage_ws(ws));
            }
        }
    }
}

pub async fn start() -> mpsc::Sender<HyperWebsocket> {
    let (ws_tx, ws_rx) = mpsc::channel::<HyperWebsocket>(1);


    // tokio::spawn(manager(ws_rx));

    return ws_tx;
}