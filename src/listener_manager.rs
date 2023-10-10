use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hyper::Client;
use hyper_tls::HttpsConnector;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::listener::{GameData, Listener};
use crate::utils::{get_join_packet_name, get_ms_since_epoch, get_sim_status, Location, System, to_wss_address};

pub enum ManagerResponse {
    Listener(Arc<Listener>),
    SimStatus(Vec<Location>),
    GameState(GameData),
    NewListenerResult(ListenerAdditionResponse),
    None,
}

pub enum ManagerRequest {
    GetState(String),
    GetListener(String),
    GetSimStatus,
    AddListener(u16, String),
}

pub enum ListenerAdditionResponse {
    IsPublic,
    Success,
    CannotJoin,
    AlreadyExists,
    BadFormat,
}

pub struct ListenerManager {
    handle: JoinHandle<()>,
    tx: mpsc::Sender<(ManagerRequest, oneshot::Sender<ManagerResponse>)>,
}

impl ListenerManager {
    pub fn new(optional_proxies: Option<Vec<String>>) -> Self {
        let (tx, rx) = mpsc::channel::<(ManagerRequest, oneshot::Sender<ManagerResponse>)>(8);
        let handle = tokio::spawn(listener_manager_task(rx, optional_proxies));
        return ListenerManager {
            handle,
            tx,
        };
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

    pub async fn add_custom_game(&self, id: u16, address: String) -> Option<ListenerAdditionResponse> {
        if let Some(ManagerResponse::NewListenerResult(result)) = self.request(ManagerRequest::AddListener(id, address)).await {
            Some(result)
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

fn try_add(id: &String, state: &GameData, locations: &mut Vec<Location>) -> bool {
    let addr = String::from(id.split('@').collect::<Vec<&str>>()[1]);
    for location in locations.iter_mut() {
        if location.address == *addr {
            let now = get_ms_since_epoch();
            dbg!(state.obtained);
            dbg!(now);
            location.systems.push(System {
                name: state.name.clone(),
                id: state.systemid,
                mode: state.mode.id.clone(),
                players: if let Some(p) = state.players.as_ref() { p.len() as u8 } else { 0 },
                unlisted: state.mode.unlisted,
                open: true,
                survival: false,
                time: if let Some(t) = state.obtained { (state.servertime + (now - t) as u32) / 1000 } else { 0 },
                criminal_activity: 0,
                mod_id: None,
            });
            return true;
        }
    }
    return false;
}

async fn listener_manager_task(rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, optional_proxies: Option<Vec<String>>) {
    let listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>> = Arc::new(Mutex::new(HashMap::new()));
    let custom_listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut custom_locations: Vec<Location> = vec![];
    let client = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());

    let (sim_status_tx, sim_status_rx) = mpsc::channel::<Vec<Location>>(1);


    println!("Fetching join packet...");

    let join_packet_name = get_join_packet_name(Some(client.clone())).await.expect("Failed retrieving join packet");

    tokio::spawn(listener_signaling_task(rx, Arc::clone(&listeners), Arc::clone(&custom_listeners), sim_status_rx, join_packet_name.clone()));

    let mut proxy_counter = 0;

    loop {
        println!("Checking listeners...");

        let mut sim_status_res = get_sim_status(Some(client.clone())).await;
        while sim_status_res.is_err() {
            println!("Error fetching simstatus.json. Retrying...");
            sim_status_res = get_sim_status(Some(client.clone())).await;
        }

        let mut sim_status = sim_status_res.unwrap();

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
                    if !system.survival && !listeners_.contains_key(&id) && system.mode != "invasion" {
                        println!("Putting new listener into {}", id);
                        if let Some(proxies) = optional_proxies.as_ref() {
                            listeners_.insert(id, Arc::new(Listener::new(to_wss_address(&location.address).expect("invalid address in simstatus???"), system.id, join_packet_name.clone(), Some(proxies[proxy_counter].clone()))));
                            proxy_counter += 1;
                            proxy_counter = proxy_counter % proxies.len();
                        } else {
                            listeners_.insert(id, Arc::new(Listener::new(to_wss_address(&location.address).expect("invalid address in simstatus???"), system.id, join_packet_name.clone(), None)));
                        }
                    }
                }
            }
        }
        // Remove all expired custom listeners
        let cl: Vec<(String, Arc<Listener>)>;
        {
            let mut custom_listeners_ = custom_listeners.lock().expect("Couldn't lock listeners");
            let keys: Vec<String> = custom_listeners_.keys().cloned().filter(|l| custom_listeners_.get(l).unwrap().is_finished()).collect();
            for key in keys {
                println!("Removing stale custom listener for {}", key);
                custom_listeners_.remove(&key);
            }

            cl = custom_listeners_.iter().map(|l| (l.0.clone(), l.1.clone())).collect();
        }
        for (id, listener) in cl.iter() {
            if let Some(state) = listener.get_game_state().await {
                let mut found = try_add(id, &state, &mut sim_status);
                if !found {
                    found = try_add(id, &state, &mut custom_locations);
                }
                if !found {
                    let new_location = Location {
                        location: state.region.clone(),
                        address: String::from(id.split('@').collect::<Vec<&str>>()[1]),
                        current_players: 0,
                        systems: Vec::new(),
                        modding: None,
                    };
                    custom_locations.push(new_location);
                    try_add(id, &state, &mut custom_locations);
                }
            }
        }

        for location in custom_locations.iter() {
            sim_status.push(location.clone());
        }

        if let Err(_) = sim_status_tx.send(sim_status).await {
            return;
        }

        sleep(Duration::from_secs(15)).await;
    }
}

async fn listener_signaling_task(mut rx: mpsc::Receiver<(ManagerRequest, oneshot::Sender<ManagerResponse>)>, listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>>, custom_listeners: Arc<Mutex<HashMap<String, Arc<Listener>>>>, mut sim_status_rx: mpsc::Receiver<Vec<Location>>, join_packet_name: String) {
    let sim_status: Arc<Mutex<Vec<Location>>> = Arc::new(Mutex::new(vec![]));
    loop {
        tokio::select! {
            req = rx.recv() => {
                match req {
                    None => {
                        return;
                    }
                    // Respond to a manager request. These NEED to be quick, any usage of .await
                    // stalls the whole responder thread, so these .awaits should be just listener
                    // info requests
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
                                if maybe_listener.is_none() {
                                    let guard = custom_listeners.lock().expect("Failed locking listeners");
                                    if let Some(listener) = guard.get(&id) {
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
                                let status;
                                {
                                    status = sim_status.lock().expect("Failed locking sim_status").clone();
                                }
                                let _ = req.1.send(ManagerResponse::SimStatus(status));
                            }
                            ManagerRequest::AddListener(id, address) => {

                                // check if said listener already exists
                                let listener_id = format!("{}@{}", id, address);
                                dbg!(listener_id.clone());
                                let mut listener_exists = false;
                                {
                                    let guard = listeners.lock().expect("Failed locking listeners");
                                    if let Some(_) = guard.get(&listener_id) {
                                        listener_exists = true;
                                    }
                                }
                                if !listener_exists {
                                    let guard = custom_listeners.lock().expect("Failed locking listeners");
                                    if let Some(_) = guard.get(&listener_id) {
                                        listener_exists = true;
                                    }
                                }
                                dbg!(listener_exists);
                                if !listener_exists {
                                    if let Some(wss_address) = to_wss_address(&address) {
                                        // wss address appears to be valid.
                                        // since listeners take time to connect, we want to handle this
                                        // in a separate thread
                                        let join_packet_ = join_packet_name.clone();
                                        let custom_listeners_ = Arc::clone(&custom_listeners);
                                        let sim_status_ = Arc::clone(&sim_status);
                                        tokio::spawn(async move {
                                            // TODO: get the proxies in here
                                            let listener = Listener::new(wss_address, id, join_packet_, None);
                                            if let Some(info) = listener.get_game_state().await {
                                                if info.mode.unlisted && info.mode.id != String::from("invasion") {

                                                    let mut guard = custom_listeners_.lock().expect("Failed locking listeners");
                                                    // We need to re-check because some time has passed and another listener may have been made
                                                    if guard.contains_key(&format!("{}@{}", id, address)) {
                                                        let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::AlreadyExists));
                                                        tokio::spawn(async move {listener.stop().await;});
                                                    } else {
                                                        // Finally, success
                                                        guard.insert(format!("{}@{}", id, address), Arc::new(listener));
                                                        let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::Success));

                                                        // TODO: Have the sim_status vec be shared between the two threads instead of clumsily adding the new listener here as well
                                                        {
                                                            let mut guard = sim_status_.lock().expect("Failed lock on simstatus");
                                                            if !try_add(&listener_id, &info, guard.as_mut()) {
                                                                let new_location = Location {
                                                                    location: info.region.clone(),
                                                                    address: String::from(address.split('@').collect::<Vec<&str>>()[1]),
                                                                    current_players: 0,
                                                                    systems: Vec::new(),
                                                                    modding: None
                                                                };
                                                                guard.push(new_location);
                                                                try_add(&listener_id, &info, &mut guard.as_mut());
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    tokio::spawn(async move {listener.stop().await;});
                                                    let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::IsPublic));
                                                }
                                            } else {
                                                tokio::spawn(async move {listener.stop().await;});
                                                let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::CannotJoin));
                                            }
                                        });
                                    } else {
                                        let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::BadFormat));
                                    }
                                } else {
                                    let _ = req.1.send(ManagerResponse::NewListenerResult(ListenerAdditionResponse::AlreadyExists));
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
                        let mut guard = sim_status.lock().expect("Failed lock on sim status");
                        *guard.as_mut() = status;
                    }
                }
            }
        }
    }
}