use std::io;
use std::sync::{Arc};
use crate::listener::Listener;

use futures::{sink::SinkExt, stream::StreamExt};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper_tungstenite::{tungstenite, HyperWebsocket};
use std::convert::Infallible;
use hyper_tungstenite::hyper::service::{make_service_fn, service_fn};
use tungstenite::Message;
use crate::listener_manager::ListenerManager;
use crate::websocket_manager::manage_ws;
use std::net::SocketAddr;

mod listener;
mod http_utils;
mod listener_manager;
mod proxy;
mod websocket_manager;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

async fn handle_request(mut request: Request<Body>, listeners: Arc<ListenerManager>) -> Result<Response<Body>, Error> {
    // Check if the request is a websocket upgrade request.
    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, hyper_websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        // Spawn a task to handle the websocket connection.
        tokio::spawn(async move {
            match hyper_websocket.await {
                Err(e) => {
                    eprintln!("Error in websocket: {}", e);
                },
                Ok(websocket) => {
                    match manage_ws(websocket, Arc::clone(&listeners)).await {
                        Ok(_) => {
                            return;
                        }
                        Err(e) => {
                            eprintln!("Error in manage_ws: {}", e);
                            return;
                        }
                    }
                }
            }
        });

        // Return the response so the spawned future can continue.
        Ok(response)
    } else {
        // Handle regular HTTP requests here.
        Ok(Response::new(Body::from("Hello HTTP!")))
    }
}

#[tokio::main]
async fn main() {
    let address = SocketAddr::from(([127, 0, 0, 1], 3000));

    let manager = Arc::new(ListenerManager::new());

    // Ownership puzzle involving passing listener manager into handling function
    let make_svc = make_service_fn(move |_| {
        let listeners = manager.clone();
        async move {
            Ok::<_, Error>(service_fn(move |req| {
                let listeners = listeners.clone();
                async move {
                    handle_request(req, listeners).await
                }
            }))
        }
    });

    let server = Server::bind(&address).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
