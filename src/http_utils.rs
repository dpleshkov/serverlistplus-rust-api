use serde_json;
use reqwest;
use reqwest::{Client};
use serde::{Deserialize, Serialize};
use tokio::io;

// TODO: remove unnecessary Clone trait
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

pub async fn get_sim_status(optional_client: Option<&Client>) -> Vec<Location> {
    let res;
    if let Some(client) = optional_client {
        res = client.get("https://starblast.io/simstatus.json").send().await;
    } else {
        res = reqwest::get("https://starblast.io/simstatus.json").await;
    }
    let body = res.expect("Failure fetching simstatus.json").text()
        .await.expect("Failure parsing simstatus response");
    let sim_status: Vec<Location> = serde_json::from_str(&body).expect("Failed parsing simstatus.json");
    return sim_status;
}

pub async fn get_join_packet_name(optional_client: Option<&Client>) -> io::Result<String> {
    let res;
    if let Some(client) = optional_client {
        res = client.get("https://starblast.io/").send().await;
    } else {
        res = reqwest::get("https://starblast.io").await;
    }
    let body = res.expect("Failure fetching site HTML").text()
        .await.expect("Failure parsing site HTML response");
    let obf_name_start = body.find("t.socket.send(JSON.stringify({name:")
        .expect("Could not find join packet obfuscated name");
    let obf_name = &body[obf_name_start+41..obf_name_start+46];
    let packet_name_start = body.find(obf_name).expect("Could not find join packet name");
    let packet_name_untrimmed = &body[packet_name_start+7..packet_name_start+15];
    let packet_name = packet_name_untrimmed.split("\"").next().unwrap();
    return Ok(packet_name.parse().unwrap());
}

pub fn to_wss_address(ip: &String) -> String {
    let s: Vec<&str> = ip.split(':').collect();
    let addr: Vec<&str> = s[0].split('.').collect();
    let port = s[1];

    return format!("wss://{}-{}-{}-{}.starblast.io:{}/", addr[0], addr[1], addr[2], addr[3], port);
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