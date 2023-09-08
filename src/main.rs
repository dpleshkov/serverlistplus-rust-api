use std::env;
use crate::utils::read_proxies_file;

mod listener;
mod utils;
mod listener_manager;
mod proxy;
mod websocket_manager;
mod server;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    // ARG 1: PORT
    // ARG 2: proxies file
    let port: u16 = if args.len() >= 2 {
        args[1].parse().unwrap()
    } else {
        3000
    };

    let proxies = if args.len() >= 3 {
        Some(read_proxies_file(args[2].parse().unwrap()))
    } else {
        None
    };

    server::listen(port, proxies).await;
}
