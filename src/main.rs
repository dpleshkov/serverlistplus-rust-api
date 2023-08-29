use std::io;
use warp::Filter;

mod listener;
mod http_utils;
mod listener_manager;

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

    listener::new("wss://134-122-125-113.starblast.io:3020/".parse().unwrap(), 8065, "ùov()".parse().unwrap());

    loop {

    }
    Ok(())
}
