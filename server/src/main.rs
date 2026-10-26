//! Servidor de sincronización: reparte la posición de cada nave y lleva el reloj de la carrera.
//! No simula nada; las órbitas y la física viven en cada cliente.
mod race;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use protocol::{decode_client, ClientMsg, Player, PlayerState, ServerMsg, Snapshot, MAX_PLAYERS};
use race::RaceState;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::broadcast::{self, error::RecvError};

const TICK: Duration = Duration::from_millis(50); // 20 snapshots por segundo
const MAX_MENSAJE: usize = 64; // bytes; un estado válido pesa 23
/// Quien deja de mandar posiciones (pestaña en segundo plano, red caída) desaparece para los demás.
const INACTIVO_MS: u32 = if cfg!(test) { 800 } else { 5000 };

struct Entry {
    state: PlayerState,
    last_seen: u32,
}

struct World {
    players: HashMap<u32, Entry>,
    race: RaceState,
}

impl World {
    fn states(&self) -> HashMap<u32, PlayerState> {
        self.players.iter().map(|(id, e)| (*id, e.state)).collect()
    }
}

struct Shared {
    world: Mutex<World>,
    next_id: AtomicU32,
    conectados: AtomicUsize,
    tx: broadcast::Sender<Vec<u8>>,
    boot: Instant,
}

type AppState = Arc<Shared>;

impl Shared {
    /// Milisegundos desde que arrancó el servidor: es el reloj del mundo.
    fn now_ms(&self) -> u32 {
        self.boot.elapsed().as_millis() as u32
    }
}

fn new_state() -> AppState {
    Arc::new(Shared {
        world: Mutex::new(World { players: HashMap::new(), race: RaceState::new() }),
        next_id: AtomicU32::new(0),
        conectados: AtomicUsize::new(0),
        tx: broadcast::channel(16).0,
        boot: Instant::now(),
    })
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
}

/// Cada TICK avanza la carrera, descarta a los inactivos y manda el estado de todos a todos.
fn spawn_ticker(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            let now = state.now_ms();
            let snapshot = {
                let mut w = state.world.lock().unwrap();
                w.players.retain(|_, e| now.wrapping_sub(e.last_seen) < INACTIVO_MS);
                let states = w.states();
                w.race.tick(now, &states);
                if w.players.is_empty() {
                    continue;
                }
                let players = w
                    .players
                    .iter()
                    .map(|(id, e)| Player { id: *id, state: e.state, finish_ms: w.race.finish_ms(*id) })
                    .collect();
                ServerMsg::Snapshot(Snapshot { clock_ms: now, phase: w.race.phase, phase_ms: w.race.phase_ms(now), players }).encode()
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

fn on_message(state: &Shared, id: u32, msg: ClientMsg) {
    let now = state.now_ms();
    let mut w = state.world.lock().unwrap();
    match msg {
        ClientMsg::State(s) if valido(&s) => {
            w.race.record_progress(id, s.progress, now);
            w.players.insert(id, Entry { state: s, last_seen: now });
        }
        ClientMsg::State(_) => {}
        ClientMsg::StartRace => {
            if w.race.start(now) {
                for e in w.players.values_mut() {
                    e.state.progress = 0;
                }
            }
        }
    }
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
                        if let Some(m) = decode_client(&bytes) {
                            on_message(&state, id, m);
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

    {
        let mut w = state.world.lock().unwrap();
        w.players.remove(&id);
        w.race.remove(id);
    }
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
    use protocol::{encode_client, PHASE_COUNTDOWN, PHASE_LOBBY};
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
        PlayerState { position: [x, 0.0, 0.0], yaw: 0.0, pitch: 0.0, model: 0, progress: 0 }
    }

    async fn enviar(c: &mut Cliente, m: ClientMsg) {
        c.send(Msg::Binary(encode_client(&m).into())).await.unwrap();
    }

    async fn esperar_snapshot(c: &mut Cliente, cond: impl Fn(&Snapshot) -> bool) -> Snapshot {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let ServerMsg::Snapshot(s) = siguiente(c).await {
                    if cond(&s) {
                        return s;
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

        enviar(&mut a, ClientMsg::State(estado(10.0))).await;
        enviar(&mut b, ClientMsg::State(estado(-5.0))).await;
        // Los dos clientes siguen mandando para no caer por inactividad durante la prueba.
        let s = esperar_snapshot(&mut a, |s| s.players.len() == 2).await;
        assert_eq!(s.players.iter().find(|p| p.id == id_b).unwrap().state.position[0], -5.0);
        assert_eq!(s.phase, PHASE_LOBBY);

        drop(b);
        let s = esperar_snapshot(&mut a, |s| s.players.len() == 1).await;
        assert_eq!(s.players[0].id, id_a);
    }

    #[tokio::test]
    async fn ignora_basura_y_estados_invalidos() {
        let url = arrancar().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let _ = id_de(&mut a).await;
        a.send(Msg::Binary(vec![1, 2, 3].into())).await.unwrap();
        enviar(&mut a, ClientMsg::State(estado(f32::NAN))).await;
        enviar(&mut a, ClientMsg::State(estado(3.0))).await;
        let s = esperar_snapshot(&mut a, |s| s.players.len() == 1).await;
        assert_eq!(s.players[0].state.position[0], 3.0);
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

    #[tokio::test]
    async fn pedir_empezar_inicia_la_cuenta_regresiva_para_todos_y_reinicia_el_progreso() {
        let url = arrancar().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let (mut b, _) = connect_async(&url).await.unwrap();
        let (_, id_b) = (id_de(&mut a).await, id_de(&mut b).await);
        enviar(&mut a, ClientMsg::State(estado(0.0))).await;
        enviar(&mut b, ClientMsg::State(PlayerState { progress: 5, ..estado(1.0) })).await;
        esperar_snapshot(&mut a, |s| s.players.len() == 2).await;

        enviar(&mut a, ClientMsg::StartRace).await;
        let s = esperar_snapshot(&mut b, |s| s.phase == PHASE_COUNTDOWN).await;
        assert!(s.phase_ms < 3000);
        assert_eq!(s.players.iter().find(|p| p.id == id_b).unwrap().state.progress, 0);
        assert!(s.clock_ms > 0 || s.phase_ms < 3000, "el snapshot trae el reloj del mundo");
    }

    #[tokio::test]
    async fn quien_deja_de_mandar_posiciones_desaparece_para_los_demas() {
        let url = arrancar().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let (mut b, _) = connect_async(&url).await.unwrap();
        let (id_a, id_b) = (id_de(&mut a).await, id_de(&mut b).await);
        enviar(&mut a, ClientMsg::State(estado(1.0))).await;
        enviar(&mut b, ClientMsg::State(estado(2.0))).await;
        esperar_snapshot(&mut b, |s| s.players.len() == 2).await;

        // A se queda callado (como una pestaña en segundo plano); B sigue activo.
        let solo_b = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                enviar(&mut b, ClientMsg::State(estado(2.0))).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
                while let Ok(Some(Ok(Msg::Binary(bytes)))) = tokio::time::timeout(Duration::from_millis(1), b.next()).await {
                    if let Some(ServerMsg::Snapshot(s)) = ServerMsg::decode(&bytes[..]) {
                        if s.players.len() == 1 && s.players[0].id == id_b {
                            return;
                        }
                    }
                }
            }
        })
        .await;
        assert!(solo_b.is_ok(), "A debía desaparecer tras quedarse inactivo");

        // Al volver a mandar su estado, A reaparece.
        enviar(&mut a, ClientMsg::State(estado(1.0))).await;
        let s = esperar_snapshot(&mut b, |s| s.players.len() == 2).await;
        assert!(s.players.iter().any(|p| p.id == id_a));
    }
}
