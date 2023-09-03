use std::collections::HashMap;
use std::fmt::Formatter;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::{SinkExt, StreamExt};
use futures_enum::{Sink, Stream};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{client_async_tls, connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::error::Error as WsError;
use tokio_tungstenite::tungstenite::Message;

use crate::proxy::{InnerProxy, ProxyStream};

enum ListenerResponse {
    Receiver(broadcast::Receiver<Vec<u8>>),
    Json(String),
    None
}

enum ListenerRequest {
    Subscribe,
    GetName(u8),
    GetState
}

enum ListenerError {
    SocketError(WsError),
    CannotJoin(u16),
    InvalidVersion,
}

impl std::fmt::Display for ListenerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ListenerError::SocketError(err) => {
                write!(f, "{}", err)
            }
            ListenerError::CannotJoin(id) => {
                write!(f, "Cannot join {}", id)
            }
            ListenerError::InvalidVersion => {
                write!(f, "Invalid join packet version")
            }
        }
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for ListenerError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        ListenerError::SocketError(err)
    }
}

type Result<T> = std::result::Result<T, ListenerError>;

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
    tx: mpsc::Sender<(ListenerRequest, oneshot::Sender<ListenerResponse>)>,
    handle: JoinHandle<Result<()>>,
}

impl Listener {
    pub fn new(address: String, game_id: u16, join_packet_name: String, proxy: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel::<(ListenerRequest, oneshot::Sender<ListenerResponse>)>(16);
        Listener {
            tx,
            handle: tokio::spawn(listener_main(address, proxy, game_id, rx, join_packet_name)),
        }
    }

    pub fn get_tx(&self) -> mpsc::Sender<(ListenerRequest, oneshot::Sender<ListenerResponse>)> {
        self.tx.clone()
    }

    async fn req(&self, request: ListenerRequest) -> Option<ListenerResponse> {
        if self.handle.is_finished() {
            return None;
        }

        let (tx, rx) = oneshot::channel::<ListenerResponse>();

        if let Err(_) = self.tx.send((request, tx)).await {
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

    pub async fn subscribe(&self) -> Option<broadcast::Receiver<Vec<u8>>> {
        if let Some(res) = self.req(ListenerRequest::Subscribe).await {
            if let ListenerResponse::Receiver(receiver) = res {
                return Some(receiver);
            }
        }
        None
    }

    pub async fn get_name(&self, id: u8) -> Option<String> {
        if let Some(res) = self.req(ListenerRequest::GetName(id)).await {
            if let ListenerResponse::Json(data) = res {
                return Some(data);
            }
        }
        None
    }

    pub async fn get_game_state_json(&self) -> Option<String> {
        if let Some(res) = self.req(ListenerRequest::GetState).await {
            if let ListenerResponse::Json(data) = res {
                return Some(data);
            }
        }
        None
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

#[derive(Stream, Sink)]
enum MaybeProxiedStream {
    Proxied(WebSocketStream<MaybeTlsStream<ProxyStream>>),
    Unproxied(WebSocketStream<MaybeTlsStream<TcpStream>>)
}

async fn listener_main(address: String, proxy: Option<String>, game_id: u16, mut rx: mpsc::Receiver<(ListenerRequest, oneshot::Sender<ListenerResponse>)>, join_packet_name: String) -> Result<()> {
    let mut request = address.clone().into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Origin", "https://starblast.io/".parse().unwrap());

    let socket: MaybeProxiedStream = match proxy {
        Some(proxy_address) => {
            let proxy = InnerProxy::from_proxy_str(proxy_address.as_str()).expect("Bad proxy config");
            let tcp_stream = proxy.connect_async(address.as_str()).await.expect("Failed to create proxy stream");
            MaybeProxiedStream::Proxied(client_async_tls(request, tcp_stream).await.expect("Failure connecting").0)
        }
        None => {
            MaybeProxiedStream::Unproxied(connect_async(request).await.expect("Failure connecting").0)
        }
    };

    let (blob_tx, _) = broadcast::channel::<Vec<u8>>(16);
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
    }).to_string())).await?;

    let mut welcome_msg: GameData;

    let time_start = SystemTime::now();
    let since_the_epoch = time_start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    match socket_rx.next().await {
        None => {
            return Ok(());
        }
        Some(res) => {
            let message = res?;
            match message {
                Message::Text(msg) => {
                    let msg: GenericJSONMessage = serde_json::from_str(msg.as_str()).expect("failed parsing msg");
                    match msg.name.as_str() {
                        "welcome" => {
                            // This is if the welcome message only contains the version number
                            // meaning our join packet name is invalid
                            if msg.data.len() < 45 {
                                return Err(ListenerError::InvalidVersion);
                            }
                            welcome_msg = serde_json::from_str(msg.data.as_str()).expect("failed parsing msg");
                            welcome_msg.players = Some(HashMap::new());
                            welcome_msg.obtained = Some(since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000);

                            let mode_id = welcome_msg.mode.id.as_str();
                            let root_mode = welcome_msg.mode.root_mode.clone();
                            if !(mode_id == "team" || (mode_id == "modding" && root_mode == Some("team".parse().unwrap()))) {
                                for i in 1u8..=255 {
                                    socket_tx.send(Message::Text(json!({
                                        "name": "get_name",
                                        "data": {
                                            "id": i
                                        }
                                    }).to_string())).await?;
                                }
                            }
                        }
                        _ => {
                            return Ok(());
                        }
                    }
                }
                _ => return Ok(())
            }
        }
    }

    loop {
        tokio::select! {
            next = socket_rx.next() => {
                match next {
                    None => {
                        return Ok(());
                    }
                    Some(res) => {
                        let message = res?;
                        match message {
                            Message::Close(_) => {
                                return Ok(())
                            }
                            Message::Text(msg) => {
                                let msg: GenericJSONMessage = serde_json::from_str(msg.as_str()).expect("failed parsing msg");
                                match msg.name.as_str() {
                                    "player_name" => {
                                        let player_name: GameDataPlayer = serde_json::from_str(msg.data.as_str()).expect("failed parsing msg");
                                        if let Some(player) = welcome_msg.players.as_mut().unwrap().get_mut(&player_name.id) {
                                            player.player_name = player_name.player_name;
                                            player.custom = player_name.custom;
                                            player.hue = player_name.hue;
                                            player.friendly = player_name.friendly;
                                        } else {
                                            welcome_msg.players.as_mut().unwrap().insert(player_name.id, player_name);
                                        }
                                    }
                                    "shipgone" => {
                                        let id: u8 = msg.data.parse().unwrap();
                                        welcome_msg.players.as_mut().unwrap().remove(&id);
                                    }
                                    "cannot_join" => {
                                        socket_tx.close().await?;
                                        return Err(ListenerError::CannotJoin(game_id));
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
                                        let map_size = welcome_msg.mode.map_size;

                                        for i in (2..len).step_by(8) {
                                            let id = buf[i];
                                            let x: f32 = (buf[i+1] as f32) / 128f32 * (map_size as f32);
                                            let y: f32 = (buf[i+2] as f32) / 128f32 * (map_size as f32);
                                            let score: u32 = (buf[i+4] as u32) + ((buf[i+5] as u32) << 8) + ((buf[i+6] as u32) << 16);
                                            let ship: u16 = 100 * (1 + ((buf[i + 3] as u16) >> 5 & 7)) + 1 + (buf[i+4] as u16);
                                            let alive: bool = buf[i+3] & 1 != 0;
                                            encoded_byte_length += 15;

                                            let players = welcome_msg.players.as_mut().unwrap();
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
                                                }).to_string())).await?;
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
                                        let friendly_colors = welcome_msg.mode.friendly_colors as usize;
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

                                            let teams = welcome_msg.mode.teams.as_mut().unwrap();
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
            maybe_req = rx.recv() => {
                match maybe_req {
                    Some(req) => {
                        match req.0 {
                            ListenerRequest::GetState => {
                                req.1.send(ListenerResponse::Json(serde_json::to_string(&welcome_msg).expect("failed constructing json")));
                            }
                            ListenerRequest::Subscribe => {
                                req.1.send(ListenerResponse::Receiver(blob_tx.subscribe()));
                            }
                            ListenerRequest::GetName(id) => {
                                let maybe_player = welcome_msg.players.as_ref().unwrap().get(&id);
                                if let Some(player) = maybe_player {
                                    req.1.send(ListenerResponse::Json(json!({
                                        "name": "player_name",
                                        "data": player
                                    }).to_string()));
                                } else {
                                    req.1.send(ListenerResponse::Json(json!({
                                        "name": "no_player",
                                        "data": &id
                                    }).to_string()));
                                }
                            }
                        }
                    }
                    None => {
                        return Ok(());
                    }
                }
            }
        }
    }
}