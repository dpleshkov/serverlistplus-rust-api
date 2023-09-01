use futures_util::{SinkExt, StreamExt};
use crate::listener;
use crate::http_utils::{get_sim_status, get_join_packet_name};
use tokio::sync::{mpsc};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use std::collections::HashMap;
use std::io;
use crate::listener::Listener;
use std::sync::{Arc, Mutex, RwLock};
use hyper_tungstenite::{HyperWebsocket};
use hyper_tungstenite::hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::{Error, Message, WebSocket};
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;


struct ListenerManager {
    handle: JoinHandle<()>
}

type Listeners = HashMap<String, Listener>;

async fn listener_manager_task() {
    let mut listeners: Listeners = HashMap::new();
    loop {}
}