use futures_util::StreamExt;
use serde_json;
use reqwest;
use reqwest::{Client};
use serde::{Deserialize};
use tokio::io;

#[derive(Deserialize)]
pub struct System {
    pub(crate) name: String,
    pub(crate) id: u16,
    pub(crate) mode: String,
    pub(crate) players: u8,
    pub(crate) unlisted: bool,
    pub(crate) open: bool,
    pub(crate) survival: bool,
    pub(crate) time: u32,
    pub(crate) criminal_activity: u8
}

#[derive(Deserialize)]
pub struct Location {
    pub(crate) location: String,
    pub(crate) address: String,
    pub(crate) current_players: u16,
    pub(crate) systems: Vec<System>,
    pub(crate) modding: Option<bool>
}

pub async fn get_sim_status(optional_client: Option<&Client>) -> io::Result<Vec<Location>> {
    let res;
    if let Some(client) = optional_client {
        res = client.get("https://starblast.io/simstatus.json").send().await;
    } else {
        res = reqwest::get("https://starblast.io/simstatus.json").await;
    }
    let body = res.expect("Failure fetching simstatus.json").text()
        .await.expect("Failure parsing simstatus response");
    let sim_status: Vec<Location> = serde_json::from_str(&body)?;
    return Ok(sim_status);
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