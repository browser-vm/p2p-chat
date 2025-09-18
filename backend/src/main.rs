use axum::{
    extract::{Query, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
    Json,
};
use tokio::net::TcpListener;
use chrono::{Duration, Utc};
use futures::{sink::SinkExt, stream::StreamExt};
use jsonwebtoken::{decode, encode, DecodingKey, Header, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;
use validator::Validate;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    limit::RequestBodyLimitLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
struct LoginRequest {
    #[validate(length(min = 3, max = 20))]
    username: String,
    #[validate(length(min = 6))]
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
struct RegisterRequest {
    #[validate(length(min = 3, max = 20))]
    username: String,
    #[validate(length(min = 6, max = 100))]
    password: String,
}


#[derive(Clone, Debug, Deserialize)]
struct WsQuery {
    token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum SignalingMessage {
    JoinRoom { room: String },
    Offer { room: String, sdp: String },
    Answer { room: String, sdp: String },
    IceCandidate { room: String, candidate: String },
}

#[derive(Debug, Clone)]
struct AppState {
    users: Arc<Mutex<HashMap<String, String>>>,
    rooms: Arc<Mutex<HashMap<String, HashMap<Uuid, (String, tokio::sync::mpsc::Sender<Message>)>>>>,
}

const JWT_SECRET: &str = "secret";

async fn validate_token(token: &str) -> Result<String, StatusCode> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET.as_ref()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(token_data.claims.sub)
}

async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let token = if let Some(token) = query.token {
        token
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let username = match validate_token(&token).await {
        Ok(u) => u,
        Err(status) => return status.into_response(),
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, username, token))
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    username: String,
    _token: String,
) {
    let (sink, mut stream) = socket.split();
    let client_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel(32);

    // Writing task for outgoing messages
    let mut sink_for_writing = sink;
    let writing_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink_for_writing.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Reading loop for incoming messages
    while let Some(item) = stream.next().await {
        let msg = if let Ok(msg) = item {
            msg
        } else {
            let _ = tx.send(Message::Close(None)).await;
            break;
        };

        if let Message::Text(text) = msg {
            if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(&text) {
                match &sig_msg {
                    SignalingMessage::JoinRoom { room } => {
                        if join_room(&state, room.clone(), client_id, username.clone(), tx.clone()).await.is_ok() {
                            // Success
                        } else {
                            let _ = tx.send(Message::Text(r#"{"type":"error","message":"Room full"}"#.to_string())).await;
                            continue;
                        }
                    }
                    SignalingMessage::Offer { room, .. } | SignalingMessage::Answer { room, .. } | SignalingMessage::IceCandidate { room, .. } => {
                        let room_str = room.as_str();
                        if let Some((other_id, _)) = find_other_peer(&state, room_str, &client_id).await {
                            if let Some(other_tx) = get_sender(&state, room_str, &other_id).await {
                                let _ = other_tx.try_send(Message::Text(text.clone()));
                            }
                        } else {
                            let _ = tx.send(Message::Text(r#"{"type":"error","message":"No peer in room"}"#.to_string())).await;
                        }
                    }
                }
            }
        }
    }

    remove_from_rooms(&state, &client_id).await;
    drop(tx); // Close channel to stop writing task
    let _ = writing_task.await;
}

async fn join_room(
    state: &AppState,
    room: String,
    client_id: Uuid,
    username: String,
    tx: tokio::sync::mpsc::Sender<Message>,
) -> Result<(), ()> {
    let mut rooms = state.rooms.lock().await;
    let room_peers = rooms.entry(room.clone()).or_insert_with(HashMap::new);
    if room_peers.len() >= 2 {
        return Err(());
    }
    room_peers.insert(client_id, (username, tx));
    if room_peers.len() == 2 {
        // Notify both or send list
        let peers: Vec<_> = room_peers.values().map(|(u, _)| u.clone()).collect();
        for (_, tx) in room_peers.values() {
            let _ = tx.try_send(Message::Text(serde_json::to_string(&serde_json::json!({"type": "peers", "peers": peers})).unwrap()));
        }
    }
    Ok(())
}

async fn find_other_peer(
    state: &AppState,
    room: &str,
    client_id: &Uuid,
) -> Option<(Uuid, String)> {
    let rooms = state.rooms.lock().await;
    if let Some(room_peers) = rooms.get(room) {
        for (id, (u, _)) in room_peers {
            if id != client_id {
                return Some((*id, u.clone()));
            }
        }
    }
    None
}

async fn get_sender(
    state: &AppState,
    room: &str,
    client_id: &Uuid,
) -> Option<mpsc::Sender<Message>> {
    let rooms = state.rooms.lock().await;
    if let Some(room_peers) = rooms.get(room) {
        if let Some((_, tx)) = room_peers.get(client_id) {
            Some(tx.clone())
        } else {
            None
        }
    } else {
        None
    }
}

async fn remove_from_rooms(state: &AppState, client_id: &Uuid) {
    let mut rooms = state.rooms.lock().await;
    rooms.retain(|_, peers| {
        peers.retain(|id, _| id != client_id);
        !peers.is_empty()
    });
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    if let Err(errors) = payload.validate() {
        return (StatusCode::BAD_REQUEST, format!("Validation error: {:?}", errors)).into_response();
    }

    let mut users = state.users.lock().await;
    if users.contains_key(&payload.username) {
        return (
            StatusCode::BAD_REQUEST,
            "User already exists",
        ).into_response();
    }
    users.insert(payload.username.clone(), payload.password.clone()); // Hash password in production
    info!("User registered: {}", payload.username);
    (StatusCode::CREATED, "User registered").into_response()
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    if let Err(errors) = payload.validate() {
        return (StatusCode::BAD_REQUEST, format!("Validation error: {:?}", errors)).into_response();
    }

    let users = state.users.lock().await;
    if let Some(stored_pass) = users.get(&payload.username) {
        if stored_pass == &payload.password {
            let claims = Claims {
                sub: payload.username.clone(),
                exp: (Utc::now() + Duration::hours(24)).timestamp() as usize,
            };
            let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(JWT_SECRET.as_ref())).unwrap();
            info!("User logged in: {}", payload.username);
            return Json(serde_json::json!({ "token": token })).into_response();
        }
    }
    (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let users = Arc::new(Mutex::new(HashMap::new()));
    let rooms = Arc::new(Mutex::new(HashMap::new()));
    let state = AppState {
        users,
        rooms,
    };

    let app = Router::new()
        .route("/", get(|| async { "Hello, P2P Chat Signaling Server!" }))
        .route("/ws", get(ws_handler))
        .route("/register", post(register))
        .route("/login", post(login))
        .layer(CorsLayer::permissive()) // For development; restrict in production
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(1024 * 10)) // 10KB limit
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running on http://{}", addr);
    println!("Server running on http://{}", addr);
    println!("WebSocket available at ws://{}", addr);
    // For WSS, configure TLS in production
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}