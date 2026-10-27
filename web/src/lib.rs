//! Envoltura WASM del engine: el canvas solo muestra el framebuffer, sin WebGL.
//! El JavaScript solo abre el WebSocket y pasa bytes; el protocolo vive en Rust.
use engine::{Framebuffer, Game, Input, RemoteShip, Vec3};
use protocol::{encode_client, ClientMsg, PlayerState, ServerMsg, MAX_PLAYERS};
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

const INTERVALO_ENVIO: f32 = 0.05; // 20 estados por segundo

#[wasm_bindgen]
pub struct App {
    game: Game,
    fb: Framebuffer,
    rgba: Vec<u8>,
    input: Input,
    my_id: Option<u32>,
    outgoing: VecDeque<Vec<u8>>,
    send_acc: f32,
}

#[wasm_bindgen]
impl App {
    #[wasm_bindgen(constructor)]
    pub fn new(width: usize, height: usize) -> App {
        App {
            game: Game::new(1),
            fb: Framebuffer::new(width, height),
            rgba: vec![255; width * height * 4],
            input: Input::default(),
            my_id: None,
            outgoing: VecDeque::new(),
            send_acc: 0.0,
        }
    }

    /// La ventana cambió de tamaño (por ejemplo, pantalla completa).
    pub fn resize(&mut self, width: usize, height: usize) {
        if (width, height) != (self.fb.width, self.fb.height) {
            self.fb.resize(width, height);
            self.rgba = vec![255; width * height * 4];
        }
    }

    /// Teclas actuales como bits (ver `engine::input::bits`).
    pub fn set_input(&mut self, bits: u32) {
        self.input = Input::from_bits(bits);
    }

    /// Avanza `dt` segundos y dibuja un cuadro.
    pub fn frame(&mut self, dt: f32) {
        self.game.update(dt, &self.input);
        self.game.render(&mut self.fb);
        for (px, out) in self.fb.pixels.iter().zip(self.rgba.chunks_exact_mut(4)) {
            out[0] = (px >> 16) as u8;
            out[1] = (px >> 8) as u8;
            out[2] = *px as u8;
        }

        if self.my_id.is_some() {
            if self.game.take_start_request() {
                self.outgoing.push_back(encode_client(&ClientMsg::StartRace));
            }
            self.send_acc += dt;
            if self.send_acc >= INTERVALO_ENVIO {
                self.send_acc = 0.0;
                let s = self.game.local_state();
                self.outgoing.push_back(encode_client(&ClientMsg::State(PlayerState {
                    position: [s.position.x, s.position.y, s.position.z],
                    yaw: s.yaw,
                    pitch: s.pitch,
                    model: s.model,
                    progress: s.progress,
                })));
            }
        }
    }

    /// Siguiente mensaje que JS debe enviar por el WebSocket (vacío si no hay más).
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        self.outgoing.pop_front().unwrap_or_default()
    }

    /// Bytes que llegaron del servidor.
    pub fn net_message(&mut self, bytes: &[u8]) {
        match ServerMsg::decode(bytes) {
            Some(ServerMsg::Welcome { id }) => {
                self.my_id = Some(id);
                self.game.set_online(true);
            }
            Some(ServerMsg::Snapshot(snap)) => {
                let Some(me) = self.my_id else { return };
                // Mi casilla de salida: mi lugar entre los ids conectados.
                let mut ids: Vec<u32> = snap.players.iter().map(|p| p.id).filter(|&id| id != me).collect();
                ids.push(me);
                ids.sort_unstable();
                self.game.set_slot(ids.iter().position(|&id| id == me).unwrap_or(0));

                self.game.sync_clock(snap.clock_ms);
                self.game.set_race_server(snap.phase, snap.phase_ms);
                let rivals: Vec<RemoteShip> = snap
                    .players
                    .iter()
                    .filter(|p| p.id != me)
                    .take(MAX_PLAYERS)
                    .map(|p| RemoteShip {
                        id: p.id,
                        position: Vec3::new(p.state.position[0], p.state.position[1], p.state.position[2]),
                        yaw: p.state.yaw,
                        pitch: p.state.pitch,
                        model: p.state.model,
                        progress: p.state.progress,
                        finish_ms: p.finish_ms,
                    })
                    .collect();
                self.game.set_remote_players(&rivals);
            }
            None => {}
        }
    }

    /// La conexión se cayó: se sigue jugando solo.
    pub fn net_closed(&mut self) {
        self.my_id = None;
        self.outgoing.clear();
        self.game.set_online(false);
        self.game.place_on_grid();
    }

    pub fn status(&self) -> String {
        self.game.status()
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.rgba.as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.rgba.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::race::Phase;
    use protocol::*;

    fn snapshot(fase: u8, ids: &[u32]) -> Vec<u8> {
        let estado = PlayerState { position: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, model: 0, progress: 0 };
        ServerMsg::Snapshot(Snapshot {
            clock_ms: 90_000,
            phase: fase,
            phase_ms: 500,
            players: ids.iter().map(|&id| Player { id, state: estado, finish_ms: 0 }).collect(),
        })
        .encode()
    }

    #[test]
    fn el_motor_y_el_protocolo_usan_los_mismos_numeros() {
        assert_eq!(engine::scenario::GATE_COUNT, GATE_COUNT as usize);
        assert_eq!(Phase::Lobby as u8, PHASE_LOBBY);
        assert_eq!(Phase::Countdown as u8, PHASE_COUNTDOWN);
        assert_eq!(Phase::Racing as u8, PHASE_RACING);
        assert_eq!(Phase::Results as u8, PHASE_RESULTS);
    }

    #[test]
    fn sin_conexion_no_se_envia_nada() {
        let mut app = App::new(64, 36);
        for _ in 0..10 {
            app.frame(0.05);
        }
        assert!(app.take_outgoing().is_empty());
    }

    #[test]
    fn al_conectarse_se_envia_el_estado_20_veces_por_segundo() {
        let mut app = App::new(64, 36);
        app.net_message(&ServerMsg::Welcome { id: 3 }.encode());
        let mut enviados = 0;
        for _ in 0..20 {
            app.frame(0.05);
            while !app.take_outgoing().is_empty() {
                enviados += 1;
            }
        }
        assert!((18..=21).contains(&enviados), "{enviados}");
    }

    #[test]
    fn enter_estando_conectado_envia_la_orden_de_empezar() {
        let mut app = App::new(64, 36);
        app.net_message(&ServerMsg::Welcome { id: 1 }.encode());
        let enter = engine::input::bits::START;
        app.set_input(enter);
        app.frame(0.01); // el primer Enter solo sale del menú
        app.set_input(0);
        app.frame(0.01);
        app.set_input(enter);
        app.frame(0.01);
        let mensajes: Vec<_> = std::iter::from_fn(|| Some(app.take_outgoing()).filter(|m| !m.is_empty())).collect();
        assert!(mensajes.iter().any(|m| decode_client(m) == Some(ClientMsg::StartRace)));
    }

    #[test]
    fn la_fase_y_los_rivales_vienen_del_servidor() {
        let mut app = App::new(64, 36);
        app.net_message(&ServerMsg::Welcome { id: 5 }.encode());
        app.net_message(&snapshot(PHASE_COUNTDOWN, &[2, 5, 9]));
        app.frame(0.01);
        assert_eq!(app.game.phase(), Phase::Countdown);
        assert_eq!(app.game.remote_count(), 2, "yo no cuento como rival");
        assert!(app.status().contains("3 jugadores"));
    }

    #[test]
    fn al_caerse_la_conexion_se_vuelve_a_jugar_solo() {
        let mut app = App::new(64, 36);
        app.net_message(&ServerMsg::Welcome { id: 5 }.encode());
        app.net_message(&snapshot(PHASE_RACING, &[5, 6]));
        app.net_closed();
        assert_eq!(app.game.remote_count(), 0);
        assert_eq!(app.game.phase(), Phase::Lobby);
        assert!(app.status().contains("sin conexion"));
    }

    #[test]
    fn redimensionar_ajusta_el_cuadro() {
        let mut app = App::new(64, 36);
        app.resize(320, 180);
        app.frame(0.01);
        assert_eq!((app.fb.width, app.fb.height, app.pixels_len()), (320, 180, 320 * 180 * 4));
    }
}
