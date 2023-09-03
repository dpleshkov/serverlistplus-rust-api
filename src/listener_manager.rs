use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use reqwest;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::http_utils::{get_join_packet_name, get_sim_status, to_wss_address};
use crate::listener::Listener;

pub enum ManagerResponse {
    Receiver(broadcast::Receiver<Vec<u8>>),
    Json(String),
    Listener(Arc<Listener>),
    None
}

pub enum ManagerRequest {
    GetState(String),
    GetListener(String)
}

pub struct ListenerManager {
    handle: JoinHandle<()>,
    tx: mpsc::Sender<(ManagerRequest, oneshot::Sender<ManagerResponse>)>
}

impl ListenerManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<(ManagerRequest, oneshot::Sender<ManagerResponse>)>(8);
        let handle = tokio::spawn(listener_manager_task(rx));
        return ListenerManager {
            handle,
            tx
        }
    }

    pub async fn get_listener(&self, id: String) -> Option<Arc<Listener>> {
        if let Some(ManagerResponse::Listener(listener)) = self.request(ManagerRequest::GetListener(id)).await {
            Some(listener)
        } else {
            None
        }
    }

    async fn request(&self, req: ManagerRequest) -> Option<ManagerResponse> {
        if self.handle.is_finished() {
            return None;
        }

        let (tx, rx) = oneshot::channel::<ManagerResponse>();

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

async fn listener_signaling_task(mut rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>>) {
    loop {
        match rx.recv().await {
            None => {
                return;
            }
            Some(req) => {
                match req.0 {
                    ManagerRequest::GetListener(id) => {
                        let guard = listeners.lock().expect("Failed locking listeners");
                        if let Some(listener) = guard.get(&id) {
                            // ignore result of sending it back
                            let _ = req.1.send(ManagerResponse::Listener(Arc::clone(listener)));
                        } else {
                            let _ = req.1.send(ManagerResponse::None);
                        }
                    }
                    ManagerRequest::GetState(id) => {
                        // Might be a more elegant way to do this with more restructuring done
                        let mut maybe_listener = None;
                        let mut output = ManagerResponse::None;
                        {
                            let guard = listeners.lock().expect("Failed locking listeners");
                            if let Some(listener) = guard.get(&id) {
                                // ignore result of sending it back
                                maybe_listener = Some(Arc::clone(listener));
                            }
                        }
                        if let Some(listener) = maybe_listener {
                            if let Some(state) = listener.get_game_state_json().await {
                                output = ManagerResponse::Json(state);
                            }
                        }
                        let _ = req.1.send(output);
                    }
                }
            }
        }
    }
}