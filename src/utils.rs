use serde_json;
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;
use std::time::{SystemTime, UNIX_EPOCH};
use hyper::{Client};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;

#[derive(Deserialize, Serialize, Clone)]
pub struct System {
    pub(crate) name: String,
    pub(crate) id: u16,
    pub(crate) mode: String,
    pub(crate) players: u8,
    pub(crate) unlisted: bool,
    pub(crate) open: bool,
    pub(crate) survival: bool,
    pub(crate) time: u32,
    pub(crate) criminal_activity: u8,
    pub(crate) mod_id: Option<String>
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Location {
    pub(crate) location: String,
    pub(crate) address: String,
    pub(crate) current_players: u16,
    pub(crate) systems: Vec<System>,
    pub(crate) modding: Option<bool>
}

#[derive(Debug)]
pub enum SimStatusError {
    HyperError(hyper::Error),
    SerdeError(serde_json::Error)
}

impl From<hyper::Error> for SimStatusError {
    fn from(err: hyper::Error) -> Self {
        SimStatusError::HyperError(err)
    }
}

impl From<serde_json::Error> for SimStatusError {
    fn from(err: serde_json::Error) -> Self {
        SimStatusError::SerdeError(err)
    }
}

pub async fn get_sim_status(optional_client: Option<Client<HttpsConnector<HttpConnector>>>) -> Result<Vec<Location>, SimStatusError> {
    let res;
    if let Some(client) = optional_client {
        res = client.get("https://starblast.io/simstatus.json".parse().unwrap()).await;
    } else {
        let client = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());
        res = client.get("https://starblast.io/simstatus.json".parse().unwrap()).await;
    }
    let body = res?.into_body();
    let bytes = hyper::body::to_bytes(body).await?;
    let text = String::from_utf8(bytes.to_vec()).expect("Failed parsing body");
    let sim_status: Vec<Location> = serde_json::from_str(&text)?;
    return Ok(sim_status);
}

pub async fn get_join_packet_name(optional_client: Option<Client<HttpsConnector<HttpConnector>>>) -> Result<String, SimStatusError> {
    let res;
    if let Some(client) = optional_client {
        res = client.get("https://starblast.io/".parse().unwrap()).await;
    } else {
        let client = Client::builder().build::<_, hyper::Body>(HttpsConnector::new());
        res = client.get("https://starblast.io/".parse().unwrap()).await;
    }
    let body = res?.into_body();
    let bytes = hyper::body::to_bytes(body).await?;
    let text = String::from_utf8(bytes.to_vec()).expect("Failed parsing body");
    let obf_name_start = text.find("t.socket.send(JSON.stringify({name:")
        .expect("Could not find join packet obfuscated name");
    let obf_name = &text[obf_name_start+41..obf_name_start+46];
    let packet_name_start = text.find(obf_name).expect("Could not find join packet name");
    let packet_name_untrimmed = &text[packet_name_start+7..packet_name_start+15];
    let packet_name = packet_name_untrimmed.split("\"").next().unwrap();
    return Ok(packet_name.parse().unwrap());
}

pub fn get_ms_since_epoch() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    t.as_secs() * 1000 + t.subsec_nanos() as u64 / 1_000_000
}

pub fn to_wss_address(ip: &String) -> Option<String> {
    let s: Vec<&str> = ip.split(':').collect();
    if s.len() == 2 {
        let addr: Vec<&str> = s[0].split('.').collect();
        let port = s[1];
        if addr.len() == 4 {
            return Some(format!("wss://{}-{}-{}-{}.starblast.io:{}/", addr[0], addr[1], addr[2], addr[3], port));
        }
    }
    None
}

pub fn translate_color(hue: u16) -> String {
    if hue < 20 {
        return "Red".parse().unwrap();
    } else if hue >= 20 && hue < 40 {
        return "Orange".parse().unwrap();
    } else if hue >= 40 && hue < 70 {
        return "Yellow".parse().unwrap();
    } else if hue >= 70 && hue < 140 {
        return "Green".parse().unwrap();
    } else if hue >= 140 && hue < 170 {
        return "Teal".parse().unwrap();
    } else if hue >= 170 && hue < 270 {
        return "Blue".parse().unwrap();
    } else if hue >= 270 && hue < 300 {
        return "Purple".parse().unwrap();
    } else if hue >= 300 && hue < 330 {
        return "Pink".parse().unwrap();
    }
    return "Red".parse().unwrap();
}

pub fn read_proxies_file(file_path: String) -> Vec<String> {
    let mut output: Vec<String> = vec![];
    for line in read_to_string(file_path).expect("Failed reading proxy file").lines() {
        if line.starts_with('#') {
            continue;
        } else {
            output.push(String::from(line));
        }
    }
    output
}