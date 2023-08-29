use std::collections::HashMap;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/*
#[derive(Serialize, Deserialize)]
struct GameDataMode {
    max_players: u8,
    crystal_value: f32,
    crystal_drop: f32,
    map_size: u16,
    map_density: Option<f32>,
    lives: u8,
    max_level: u8,
    friendly_colors: u8,
    close_time: u16,
    close_number: u16,
    map_name: String,
    unlisted: bool,
    survival_time: u16,
    survival_level: u8,
    starting_ship: u32,
    starting_ship_maxed: bool,
    asteroids_strength: f32,
    friction_ratio: f32,
    speed_mod: f32,
    rcs_toggle: bool,
    weapon_drop: f32,
    mines_self_destroy: bool,
    mines_destroy_delay: u32
}*/

#[derive(Serialize, Deserialize)]
struct GameDataTeamStation {
    phase: f32,
}

#[derive(Serialize, Deserialize)]
struct GameDataTeam {
    hue: u16,
    station: GameDataTeamStation,
    #[serde(rename = "totalScore")]
    total_score: Option<u32>,
    crystals: Option<u32>,
    open: Option<bool>,
    level: Option<u8>,
    color: Option<String>,
    #[serde(rename = "ecpCount")]
    ecp_count: Option<u8>,
}

#[derive(Serialize, Deserialize)]
struct GameDataPlayerCustom {
    badge: String,
    finish: String,
    laser: String,
    hue: u16,
}

#[derive(Serialize, Deserialize)]
struct GameDataPlayer {
    id: u8,
    hue: Option<u16>,
    friendly: Option<u8>,
    player_name: Option<String>,
    custom: Option<GameDataPlayerCustom>,
    x: Option<f32>,
    y: Option<f32>,
    score: Option<u32>,
    #[serde(rename = "type")]
    type_: Option<u16>,
    alive: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct GameDataModeSimplified {
    map_size: u16,
    friendly_colors: u8,
    unlisted: bool,
    id: String,
    teams: Option<Vec<GameDataTeam>>,
    root_mode: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ApiData {
    live: bool,
    provider: String,
    #[serde(rename = "type")]
    type_: String,
    version: String,
}

#[derive(Serialize, Deserialize)]
struct GameData {
    version: u8,
    seed: u16,
    servertime: u32,
    systemid: u16,
    size: u16,
    mode: GameDataModeSimplified,
    region: String,
    obtained: Option<u64>,
    players: Option<HashMap<u8, GameDataPlayer>>,
    api: Option<ApiData>,
}

#[derive(Serialize, Deserialize)]
struct WelcomeMessage {
    name: String,
    data: GameData,
}

#[derive(Deserialize)]
struct GenericJSONMessage {
    name: String,
    data: String,
}

pub struct Listener {
    state_rx: broadcast::Receiver<Vec<u8>>,
    json_req_tx: mpsc::Sender<oneshot::Sender<String>>,
    handle: JoinHandle<io::Result<()>>
}


pub fn new(address: String, game_id: u16, join_packet_name: String) -> Listener {
    let (blob_tx, blob_rx) = broadcast::channel::<Vec<u8>>(1);
    let (json_req_tx, json_req_rx) = mpsc::channel::<oneshot::Sender<String>>(1);
    return Listener {
        state_rx: blob_rx,
        json_req_tx,
        handle: tokio::spawn(listener_main(address, game_id, blob_tx, json_req_rx, join_packet_name))
    };
}


async fn listener_main(address: String, game_id: u16, blob_tx: broadcast::Sender<Vec<u8>>, mut json_req_rx: mpsc::Receiver<oneshot::Sender<String>>, join_packet_name: String) -> io::Result<()> {
    let mut request = address.into_client_request().unwrap();
    let headers = request.headers_mut();
    headers.insert("Origin", "https://starblast.io/".parse().unwrap());

    let (socket, _) = connect_async(request).await.expect("Failure connecting");

    let (mut socket_tx, mut socket_rx) = socket.split();

    socket_tx.send(Message::Text(json!({
        "name": join_packet_name,
        "data": {
            "spectate": false,
            "spectate_ship": 1,
            "player_name": "serverlist+",
            "hue": 240,
            "preferred": game_id,
            "bonus": true,
            "create": false
        }
    }).to_string())).await.expect("failure sending join packet");

    let mut welcome_msg: Option<GameData> = None;
    let mut map_size: u16 = 0;

    let time_start = SystemTime::now();
    let since_the_epoch = time_start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    loop {
        tokio::select! {
            next = socket_rx.next() => {
                match next {
                    None => {
                        return Ok(());
                    }
                    Some(res) => {
                        let message = res.expect("Error pulling message");
                        match message {
                            Message::Close(_) => {
                                return Ok(())
                            }
                            Message::Text(msg) => {
                                let msg: GenericJSONMessage = serde_json::from_str(msg.as_str()).expect("failed parsing msg");
                                match msg.name.as_str() {
                                    "welcome" => {
                                        welcome_msg = Some(serde_json::from_str(msg.data.as_str()).expect("failed parsing msg"));
                                        welcome_msg.as_mut().unwrap().players = Some(HashMap::new());
                                        welcome_msg.as_mut().unwrap().obtained = Some(since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000);

                                        let mode_id = welcome_msg.as_ref().unwrap().mode.id.as_str();
                                        let root_mode = welcome_msg.as_ref().unwrap().mode.root_mode.clone();
                                        if !(mode_id == "team" || (mode_id == "modding" && root_mode == Some("team".parse().unwrap()))) {
                                            for i in 1u8..=255 {
                                                socket_tx.send(Message::Text(json!({
                                                    "name": "get_name",
                                                    "data": {
                                                        "id": i
                                                    }
                                                }).to_string())).await.expect("failure sending get_name");
                                            }
                                        }
                                        map_size = welcome_msg.as_ref().unwrap().mode.map_size;

                                    }
                                    "player_name" => {
                                        let player_name: GameDataPlayer = serde_json::from_str(msg.data.as_str()).expect("failed parsing msg");
                                        if let Some(player) = welcome_msg.as_mut().unwrap().players.as_mut().unwrap().get_mut(&player_name.id) {
                                            player.player_name = player_name.player_name;
                                            player.custom = player_name.custom;
                                            player.hue = player_name.hue;
                                            player.friendly = player_name.friendly;
                                        } else {
                                            welcome_msg.as_mut().unwrap().players.as_mut().unwrap().insert(player_name.id, player_name);
                                        }
                                    }
                                    "shipgone" => {
                                        let id: u8 = msg.data.parse().unwrap();
                                        welcome_msg.as_mut().unwrap().players.as_mut().unwrap().remove(&id);
                                    }
                                    "cannot_join" => {
                                        socket_tx.close().await.expect("error closing socket");
                                        return Ok(());
                                    }
                                    _ => {}
                                }
                            }
                            Message::Binary(buf) => {
                                match buf[0] {
                                    0xc8 => {
                                        let len = buf.len();
                                        let mut encoded_byte_length = 1 + (15 * (len >> 3));
                                        let mut packet = vec![1u8; encoded_byte_length];

                                        for i in (2..len).step_by(8) {
                                            let id = buf[i];
                                            let x: f32 = (buf[i+1] as f32) / 128f32 * (map_size as f32);
                                            let y: f32 = (buf[i+2] as f32) / 128f32 * (map_size as f32);
                                            let score: u32 = (buf[i+4] as u32) + ((buf[i+5] as u32) << 8) + ((buf[i+6] as u32) << 16);
                                            let ship: u16 = 100 * (1 + ((buf[i + 3] as u16) >> 5 & 7)) + 1 + (buf[i+4] as u16);
                                            let alive: bool = buf[i+3] & 1 != 0;
                                            encoded_byte_length += 15;

                                            let players = welcome_msg.as_mut().unwrap().players.as_mut().unwrap();
                                            if players.contains_key(&id) {
                                                let player = players.get_mut(&id).unwrap();
                                                player.id = id;
                                                player.x = Some(x);
                                                player.y = Some(y);
                                                player.score = Some(score);
                                                player.type_ = Some(ship);
                                                player.alive = Some(alive);
                                            } else {
                                                players.insert(id, GameDataPlayer {
                                                    id,
                                                    x: Some(x),
                                                    y: Some(y),
                                                    score: Some(score),
                                                    type_: Some(ship),
                                                    alive: Some(alive),
                                                    hue: None,
                                                    player_name: None,
                                                    custom: None,
                                                    friendly: None
                                                });
                                                socket_tx.send(Message::Text(json!({
                                                    "name": "get_name",
                                                    "data": {
                                                        "id": id
                                                    }
                                                }).to_string())).await.expect("failure sending get_name");
                                            }

                                            let d = ((i >> 3) * 15) + 1;
                                            packet[d] = id;
                                            packet[d+1] = (x).to_le_bytes()[0];
                                            packet[d+2] = (x).to_le_bytes()[1];
                                            packet[d+3] = (x).to_le_bytes()[2];
                                            packet[d+4] = (x).to_le_bytes()[3];
                                            packet[d+5] = (y).to_le_bytes()[0];
                                            packet[d+6] = (y).to_le_bytes()[1];
                                            packet[d+7] = (y).to_le_bytes()[2];
                                            packet[d+8] = (y).to_le_bytes()[3];
                                            packet[d+9] = (score).to_le_bytes()[0];
                                            packet[d+10] = (score).to_le_bytes()[1];
                                            packet[d+11] = (score).to_le_bytes()[2];
                                            packet[d+12] = (score).to_le_bytes()[3];

                                            let mut p = ship;
                                            if alive {
                                                p = p & (1 << 15);
                                            }
                                            packet[d+13] = (p).to_le_bytes()[0];
                                            packet[d+14] = (p).to_le_bytes()[1];
                                        }
                                        blob_tx.send(packet).expect("failed to send ship info byte vec");
                                    }
                                    0xcd => {
                                        let friendly_colors = welcome_msg.as_ref().unwrap().mode.friendly_colors as usize;
                                        let size = buf.len() / friendly_colors;
                                        let mut packet = vec![2u8; friendly_colors*5 + 1];
                                        for i in 0..friendly_colors {
                                            let o = i * size + 1;
                                            let open = buf[o] > 0;
                                            let level = buf[o+1] + 1;
                                            let crystals = (buf[o+2] as u32) |
                                                ((buf[o+3] as u32) << 8) |
                                                ((buf[o+4] as u32) << 16) |
                                                ((buf[o+5] as u32) << 24);

                                            let teams = welcome_msg.as_mut().unwrap().mode.teams.as_mut().unwrap();
                                            teams[i].open = Some(open);
                                            teams[i].level = Some(level);
                                            teams[i].crystals = Some(crystals);

                                            let mut a = level;
                                            if open {
                                                a = a | 0xf0;
                                            }
                                            packet[i*5+1] = a;
                                            packet[i*5+2] = buf[o+2];
                                            packet[i*5+3] = buf[o+3];
                                            packet[i*5+4] = buf[o+4];
                                            packet[i*5+5] = buf[o+5];
                                        }
                                        blob_tx.send(packet).expect("failed to send team info byte vec");
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            maybe_sender = json_req_rx.recv() => {
                match maybe_sender {
                    Some(sender) => {
                        sender.send(serde_json::to_string(&welcome_msg).expect("failed constructing json")).expect("dropped oneshot receiver");
                    }
                    None => {
                        return Ok(());
                    }
                }
            }
        }
    }
}