use std::net::SocketAddr;
use std::sync::Arc;

use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper_tungstenite::hyper::service::{make_service_fn, service_fn};
use serde::Deserialize;

use crate::listener_manager::{ListenerAdditionResponse, ListenerManager};
use crate::websocket_manager::manage_ws;

#[derive(Deserialize)]
struct CustomGamePost {
    url: String,
}

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

async fn handle_request(
    mut request: Request<Body>,
    listeners: Arc<ListenerManager>,
) -> Result<Response<Body>, Error> {
    // Check if the request is a websocket upgrade request.
    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, hyper_websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        // Spawn a task to handle the websocket connection.
        tokio::spawn(async move {
            match hyper_websocket.await {
                Err(e) => {
                    eprintln!("Error in websocket: {}", e);
                }
                Ok(websocket) => match manage_ws(websocket, Arc::clone(&listeners)).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error in manage_ws: {}", e);
                    }
                },
            }
        });

        // Return the response so the spawned future can continue.
        Ok(response)
    } else {
        // Handle regular HTTP requests here.
        if request.uri().to_string().starts_with("/simstatus.json") {
            let sim_status = listeners.get_sim_status().await;
            return if let Some(status) = sim_status {
                Ok(Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .status(StatusCode::OK)
                    .body(Body::from(serde_json::to_string(&status).unwrap()))
                    .expect("Failure building body"))
            } else {
                Ok(Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .status(StatusCode::OK)
                    .body(Body::from("[]"))
                    .expect("failure building body"))
            };
        } else if request.uri().to_string().starts_with("/status/") {
            let uri = request.uri().to_string();
            let segments: Vec<&str> = uri.split('/').collect();
            return if segments.len() < 3 {
                Ok(Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(
                        "400 Bad Request. Please specify a game ID like /status/1234",
                    ))
                    .expect("failure building body"))
            } else {
                let game_id: String = segments[2].parse().unwrap_or(String::from(""));
                let maybe_status = listeners.get_state(game_id).await;
                if let Some(status) = maybe_status {
                    Ok(Response::builder()
                        .header("Access-Control-Allow-Origin", "*")
                        .status(StatusCode::OK)
                        .body(Body::from(serde_json::to_string(&status).unwrap()))
                        .expect("failure building body"))
                } else {
                    Ok(Response::builder()
                        .header("Access-Control-Allow-Origin", "*")
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("{}"))
                        .expect("failure building body"))
                }
            };
        } else if request.uri().to_string().starts_with("/post") && request.method() == Method::POST
        {
            if let Ok(post) = serde_json::from_str::<CustomGamePost>(
                String::from_utf8(hyper::body::to_bytes(request.into_body()).await?.to_vec())?
                    .as_str(),
            ) {
                // https://starblast.io/#1234@123.123.123.123:1234
                let split: Vec<&str> = post.url.split('/').collect();
                dbg!(split.clone());
                if split.len() == 4 {
                    let split: Vec<&str> = split[3][1..].split('@').collect();
                    dbg!(split.clone());
                    if split.len() == 2 {
                        let id: u16 = split[0].parse().unwrap_or(0);
                        let ip = split[1].parse().unwrap();
                        if let Some(address) = crate::utils::to_wss_address(split[1]) {
                            dbg!(address.clone());
                            let res = listeners.add_custom_game(id, ip).await;
                            return Ok(match res {
                                None => Response::builder()
                                    .header("Access-Control-Allow-Origin", "*")
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::from("{\"status\":\"error\",\"error\":\"error\"}"))
                                    .expect("failure building body"),
                                Some(result) => match result {
                                    ListenerAdditionResponse::CannotJoin => Response::builder()
                                        .header("Access-Control-Allow-Origin", "*")
                                        .status(StatusCode::OK)
                                        .body(Body::from("{\"error\":\"can't connect\"}"))
                                        .expect("failure building body"),
                                    ListenerAdditionResponse::Success => Response::builder()
                                        .header("Access-Control-Allow-Origin", "*")
                                        .status(StatusCode::OK)
                                        .body(Body::from("{\"status\":\"success\"}"))
                                        .expect("failure building body"),
                                    ListenerAdditionResponse::AlreadyExists => Response::builder()
                                        .header("Access-Control-Allow-Origin", "*")
                                        .status(StatusCode::OK)
                                        .body(Body::from("{\"status\":\"success\"}"))
                                        .expect("failure building body"),
                                    ListenerAdditionResponse::BadFormat => Response::builder()
                                        .header("Access-Control-Allow-Origin", "*")
                                        .status(StatusCode::BAD_REQUEST)
                                        .body(Body::from(
                                            "{\"status\":\"error\",\"error\":\"invalid format\"}",
                                        ))
                                        .expect("failure building body"),
                                    ListenerAdditionResponse::IsPublic => Response::builder()
                                        .header("Access-Control-Allow-Origin", "*")
                                        .status(StatusCode::OK)
                                        .body(Body::from("{\"status\":\"success\"}"))
                                        .expect("failure building body"),
                                },
                            });
                        }
                    }
                }
            }
            return Ok(Response::builder()
                .header("Access-Control-Allow-Origin", "*")
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(
                    "{\"status\":\"error\",\"error\":\"invalid format\"}",
                ))
                .expect("failure building body"));
        } else if request.method() == Method::OPTIONS {
            return Ok(Response::builder()
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
                .header("Access-Control-Allow-Headers", "*")
                .header("Access-Control-Max-Age", "84600")
                .status(StatusCode::NO_CONTENT)
                .body(Body::from(""))
                .expect("failure building body"));
        }
        Ok(Response::builder()
            .header("Access-Control-Allow-Origin", "*")
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .expect("failure building body"))
    }
}

pub async fn listen(port: u16, proxies: Option<Vec<String>>) {
    let address = SocketAddr::from(([127, 0, 0, 1], port));

    let manager = Arc::new(ListenerManager::new(proxies));

    // Ownership puzzle involving passing listener manager into handling function
    let make_svc = make_service_fn(move |_| {
        let listeners = manager.clone();
        async move {
            Ok::<_, Error>(service_fn(move |req| {
                let listeners = listeners.clone();
                async move { handle_request(req, listeners).await }
            }))
        }
    });

    let server = Server::bind(&address).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
