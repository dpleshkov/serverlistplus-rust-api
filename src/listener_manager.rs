use futures_util::{SinkExt, StreamExt};
use crate::listener;
use tokio::sync::{broadcast, mpsc, oneshot};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use std::collections::HashMap;
use std::io;
use std::mem::forget;
use crate::listener::{Listener, ListenerResponse, ListenerRequest};
use std::sync::{Arc, Mutex, RwLock};
use hyper_tungstenite::{HyperWebsocket};
use hyper_tungstenite::hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::{Error, Message, WebSocket};
use tokio::task::JoinHandle;
use reqwest;
use tokio_tungstenite::WebSocketStream;
use crate::http_utils::{get_sim_status, get_join_packet_name, to_wss_address};
use tokio::time::sleep;
use std::time::Duration;

pub enum ManagerResponse {
    Receiver(broadcast::Receiver<Vec<u8>>),
    Json(String),
    Listener(Listener),
    None
}

pub enum ManagerRequest {
    Subscribe(u16),
    GetName(u16, u8),
    GetState(u16),
    GetListener(u16)
}

pub struct ListenerManager {
    handle: JoinHandle<()>,
    tx: mpsc::Sender<(ListenerRequest, oneshot::Sender<ListenerResponse>)>
}

impl ListenerManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<(ListenerRequest, oneshot::Sender<ListenerResponse>)>(8);
        let handle = tokio::spawn(listener_manager_task(rx));
        return ListenerManager {
            handle,
            tx
        }
    }

    async fn get_listener(&self, id: u16) -> Option<Arc<Listener>> {
        if let Some(ListenerResponse::L(id)) = self.request()
    }

    async fn request(&self, req: ManagerRequest) -> Option<ManagerResponse> {
        if self.handle.is_finished() {
            return None;
        }

        let (tx, rx) = oneshot::channel::<ListenerResponse>();

        if let Err(_) = self.tx.send((req, tx)).await {
            return None;
        }

        match rx.await {
            Ok(res) => {
                Some(res)
            }
            Err(_) => {
                None
            }
        }
    }

    pub async fn subscribe(&self, id: u16) -> Option<broadcast::Receiver<Vec<u8>>> {
        if let Some(res) = self.request(ManagerRequest::Subscribe(id)).await {
            if let ManagerResponse::Receiver(receiver) = res {
                return Some(receiver);
            }
        }
        None
    }
}

async fn listener_manager_task(rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>) {
    let listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>> = Arc::new(Mutex::new(HashMap::new()));
    let client = reqwest::Client::new();

    tokio::spawn(listener_signaling_task(rx, Arc::clone(&listeners)));

    let join_packet_name = get_join_packet_name(Some(&client)).await.expect("Failed retrieving join packet");
    let mut sim_status = get_sim_status(Some(&client)).await.expect("Failed fetching sim status");
    loop {
        println!("Checking listeners...");
        // Remove all listeners that have expired
        {
            let mut guard = listeners.lock().expect("couldn't lock listeners");
            let keys: Vec<String> = guard.keys().cloned().filter(|l| guard.get(l).unwrap().is_finished()).collect();
            for key in keys {
                println!("Removing stale listener for {}", key);
                guard.remove(&key);
            }

            for location in sim_status {
                for system in location.systems {
                    let id = format!("{}@{}", system.id, location.address);
                    if !guard.contains_key(&id) {
                        // TODO: implement proxy selection
                        println!("Putting new listener into {}", id);
                        guard.insert(id, Arc::new(Listener::new(to_wss_address(&location.address), system.id, join_packet_name.clone(), None)));
                    }
                }
            }
        }

        sleep(Duration::from_secs(15)).await;
        sim_status = get_sim_status(Some(&client)).await.expect("Failed fetching sim status");

    }
}

async fn listener_signaling_task(rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, listeners: Arc<Mutex<HashMap<String, Listener>>>) {

}