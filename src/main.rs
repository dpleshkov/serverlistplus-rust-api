use std::io;
use crate::listener::Listener;

mod listener;
mod http_utils;
mod listener_manager;
mod proxy;
mod websocket_manager;

#[tokio::main]
async fn main() -> io::Result<()> {
    let simstatus = http_utils::get_sim_status(None).await?;
    for location in simstatus {
        for system in location.systems {
            println!("{}", system.name);
        }
    }
    let join_name = http_utils::get_join_packet_name(None).await?;
    println!("{}", join_name);

    Listener::new("wss://134-122-125-113.starblast.io:3020/".parse().unwrap(), 8065, "ùov()".parse().unwrap(), None);

    loop {

    }
    Ok(())
}
