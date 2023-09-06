use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use reqwest;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::http_utils::{get_join_packet_name, get_sim_status, to_wss_address, Location};
use crate::listener::{GameData, Listener};

pub enum ManagerResponse {
    Receiver(broadcast::Receiver<Vec<u8>>),
    Json(String),
    Listener(Arc<Listener>),
    SimStatus(Vec<Location>),
    GameState(GameData),
    None
}

pub enum ManagerRequest {
    GetState(String),
    GetListener(String),
    GetSimStatus
}

pub struct ListenerManager {
    handle: JoinHandle<()>,
    tx: mpsc::Sender<(ManagerRequest, oneshot::Sender<ManagerResponse>)>
}

impl ListenerManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<(ManagerRequest, oneshot::Sender<ManagerResponse>)>(8);
        let handle = tokio::spawn(listener_manager_task(rx, None));
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

    pub async fn get_sim_status(&self) -> Option<Vec<Location>> {
        if let Some(ManagerResponse::SimStatus(sim_status)) = self.request(ManagerRequest::GetSimStatus).await {
            Some(sim_status)
        } else {
            None
        }
    }

    pub async fn get_state(&self, id: String) -> Option<GameData> {
        if let Some(ManagerResponse::GameState(state)) = self.request(ManagerRequest::GetState(id)).await {
            Some(state)
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

async fn listener_manager_task(rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, optional_proxies: Option<Vec<String>>) {
    let listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>> = Arc::new(Mutex::new(HashMap::new()));
    let client = reqwest::Client::new();

    let (sim_status_tx, sim_status_rx) = mpsc::channel::<Vec<Location>>(1);

    tokio::spawn(listener_signaling_task(rx, Arc::clone(&listeners), sim_status_rx));

    println!("Fetching join packet...");

    let join_packet_name = get_join_packet_name(Some(&client)).await.expect("Failed retrieving join packet");

    let mut proxy_counter = 0;

    loop {
        println!("Checking listeners...");

        let sim_status = get_sim_status(Some(&client)).await;

        // Remove all listeners that have expired
        {
            let mut listeners_ = listeners.lock().expect("couldn't lock listeners");
            let keys: Vec<String> = listeners_.keys().cloned().filter(|l| listeners_.get(l).unwrap().is_finished()).collect();
            for key in keys {
                println!("Removing stale listener for {}", key);
                listeners_.remove(&key);
            }

            for location in sim_status.iter() {
                for system in location.systems.iter() {
                    let id = format!("{}@{}", system.id, location.address);
                    if system.open && !system.survival && !listeners_.contains_key(&id) && system.mode == "team" {
                        println!("Putting new listener into {}", id);
                        if let Some(proxies) = optional_proxies.as_ref() {
                            listeners_.insert(id, Arc::new(Listener::new(to_wss_address(&location.address), system.id, join_packet_name.clone(), Some(proxies[proxy_counter].clone()))));
                            proxy_counter += 1;
                            proxy_counter = proxy_counter % proxies.len();
                        } else {
                            listeners_.insert(id, Arc::new(Listener::new(to_wss_address(&location.address), system.id, join_packet_name.clone(), None)));
                        }
                    }
                }
            }
        }

        if let Err(_) = sim_status_tx.send(sim_status).await {
            return;
        }

        sleep(Duration::from_secs(15)).await;


    }
}

async fn listener_signaling_task(mut rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>>, mut sim_status_rx: mpsc::Receiver<Vec<Location>>) {
    let mut sim_status: Option<Vec<Location>> = None;
    loop {
        tokio::select! {
            req = rx.recv() => {
                match req {
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
                                println!("Got req for state {}", id);
                                {
                                    let guard = listeners.lock().expect("Failed locking listeners");
                                    println!("Locked listener guard");
                                    if let Some(listener) = guard.get(&id) {
                                        println!("Listener {} exists", id);
                                        // ignore result of sending it back
                                        maybe_listener = Some(Arc::clone(listener));
                                    }
                                }
                                if let Some(listener) = maybe_listener {
                                    if let Some(state) = listener.get_game_state().await {
                                        output = ManagerResponse::GameState(state);
                                    }
                                }
                                let _ = req.1.send(output);
                            }
                            ManagerRequest::GetSimStatus => {
                                println!("received req for simstatus");
                                if let Some(status) = sim_status.as_ref() {
                                    let _ = req.1.send(ManagerResponse::SimStatus((*status).clone()));
                                } else {
                                    let _ = req.1.send(ManagerResponse::None);
                                }
                            }
                        }
                    }
                }
            }
            maybe_sim_status = sim_status_rx.recv() => {
                match maybe_sim_status {
                    None => {
                        return;
                    }
                    Some(status) => {
                        sim_status = Some(status);
                    }
                }
            }
        }
    }
}