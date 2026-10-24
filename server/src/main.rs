//! Servidor de sincronización: recibe el estado de cada nave y lo reparte a todos.
//! No simula nada; las órbitas y la física viven en cada cliente.
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use protocol::{decode_client, Player, PlayerState, ServerMsg, MAX_PLAYERS};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::broadcast::{self, error::RecvError};

const TICK: Duration = Duration::from_millis(50); // 20 snapshots por segundo
const MAX_MENSAJE: usize = 64; // bytes; un estado válido pesa 22

struct Shared {
    players: Mutex<HashMap<u32, PlayerState>>,
    next_id: AtomicU32,
    conectados: AtomicUsize,
    tx: broadcast::Sender<Vec<u8>>,
}

type AppState = Arc<Shared>;

fn new_state() -> AppState {
    Arc::new(Shared {
        players: Mutex::new(HashMap::new()),
        next_id: AtomicU32::new(0),
        conectados: AtomicUsize::new(0),
        tx: broadcast::channel(16).0,
    })
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
}

/// Cada TICK manda el estado de todos a todos los conectados.
fn spawn_ticker(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            let snapshot = {
                let players = state.players.lock().unwrap();
                if players.is_empty() {
                    continue;
                }
                let list = players.iter().map(|(id, s)| Player { id: *id, state: *s }).collect();
                ServerMsg::Snapshot(list).encode()
            };
            let _ = state.tx.send(snapshot); // sin receptores no pasa nada
        }
    });
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.max_message_size(MAX_MENSAJE).on_upgrade(move |socket| handle(socket, state))
}

fn valido(s: &PlayerState) -> bool {
    s.position.iter().all(|v| v.is_finite() && v.abs() < 1.0e6) && s.yaw.is_finite() && s.pitch.is_finite()
}

async fn handle(mut socket: WebSocket, state: AppState) {
    if state.conectados.fetch_add(1, Ordering::SeqCst) >= MAX_PLAYERS {
        state.conectados.fetch_sub(1, Ordering::SeqCst);
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let id = state.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    eprintln!("jugador {id} conectado");
    let mut rx = state.tx.subscribe();

    if socket.send(Message::Binary(ServerMsg::Welcome { id }.encode().into())).await.is_ok() {
        loop {
            tokio::select! {
                msg = socket.recv() => match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        if let Some(s) = decode_client(&bytes).filter(valido) {
                            state.players.lock().unwrap().insert(id, s);
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                },
                snap = rx.recv() => match snap {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {} // cliente lento: se salta cuadros viejos
                    Err(RecvError::Closed) => break,
                },
            }
        }
    }

    state.players.lock().unwrap().remove(&id);
    state.conectados.fetch_sub(1, Ordering::SeqCst);
    eprintln!("jugador {id} desconectado");
}

#[tokio::main]
async fn main() {
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".into());
    let state = new_state();
    spawn_ticker(state.clone());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("no se pudo abrir el puerto");
    eprintln!("servidor escuchando en {addr}");
    axum::serve(listener, app(state)).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use protocol::encode_client;
    use tokio::net::TcpStream;
    use tokio_tungstenite::{connect_async, tungstenite::Message as Msg, MaybeTlsStream, WebSocketStream};

    type Cliente = WebSocketStream<MaybeTlsStream<TcpStream>>;

    async fn arrancar() -> String {
        let state = new_state();
        spawn_ticker(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
        format!("ws://{addr}/ws")
    }

    async fn siguiente(c: &mut Cliente) -> ServerMsg {
        loop {
            if let Msg::Binary(b) = c.next().await.unwrap().unwrap() {
                return ServerMsg::decode(&b[..]).unwrap();
            }
        }
    }

    async fn id_de(c: &mut Cliente) -> u32 {
        match siguiente(c).await {
            ServerMsg::Welcome { id } => id,
            otro => panic!("se esperaba Welcome, llegó {otro:?}"),
        }
    }

    fn estado(x: f32) -> PlayerState {
        PlayerState { position: [x, 0.0, 0.0], yaw: 0.0, pitch: 0.0, model: 0 }
    }

    async fn esperar_jugadores(c: &mut Cliente, n: usize) -> Vec<Player> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let ServerMsg::Snapshot(p) = siguiente(c).await {
                    if p.len() == n {
                        return p;
                    }
                }
            }
        })
        .await
        .expect("no llegó el snapshot esperado")
    }

    #[tokio::test]
    async fn dos_clientes_se_ven_entre_si_y_al_salir_desaparecen() {
        let url = arrancar().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let (mut b, _) = connect_async(&url).await.unwrap();
        let (id_a, id_b) = (id_de(&mut a).await, id_de(&mut b).await);
        assert_ne!(id_a, id_b);

        a.send(Msg::Binary(encode_client(&estado(10.0)).into())).await.unwrap();
        b.send(Msg::Binary(encode_client(&estado(-5.0)).into())).await.unwrap();

        let vistos = esperar_jugadores(&mut a, 2).await;
        let de_b = vistos.iter().find(|p| p.id == id_b).unwrap();
        assert_eq!(de_b.state.position[0], -5.0);

        drop(b);
        let vistos = esperar_jugadores(&mut a, 1).await;
        assert_eq!(vistos[0].id, id_a);
    }

    #[tokio::test]
    async fn ignora_basura_y_estados_invalidos() {
        let url = arrancar().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let _ = id_de(&mut a).await;
        a.send(Msg::Binary(vec![1, 2, 3].into())).await.unwrap();
        a.send(Msg::Binary(encode_client(&estado(f32::NAN)).into())).await.unwrap();
        a.send(Msg::Binary(encode_client(&estado(3.0)).into())).await.unwrap();
        let p = esperar_jugadores(&mut a, 1).await;
        assert_eq!(p[0].state.position[0], 3.0);
    }

    #[tokio::test]
    async fn rechaza_al_noveno_jugador() {
        let url = arrancar().await;
        let mut dentro = Vec::new();
        for _ in 0..MAX_PLAYERS {
            let (mut c, _) = connect_async(&url).await.unwrap();
            id_de(&mut c).await;
            dentro.push(c);
        }
        let (mut extra, _) = connect_async(&url).await.unwrap();
        let r = tokio::time::timeout(Duration::from_secs(2), extra.next()).await.unwrap();
        assert!(matches!(r, Some(Ok(Msg::Close(_))) | Some(Err(_)) | None), "el noveno no debe recibir Welcome");
    }
}
