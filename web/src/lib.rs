//! Envoltura WASM del engine: el canvas solo muestra el framebuffer, sin WebGL.
//! El JavaScript solo abre el WebSocket y pasa bytes; el protocolo vive en Rust.
use engine::{Framebuffer, Game, Input, RemoteShip, Vec3};
use protocol::{encode_client, PlayerState, ServerMsg, MAX_PLAYERS};
use wasm_bindgen::prelude::*;

const INTERVALO_ENVIO: f32 = 0.05; // 20 estados por segundo

#[wasm_bindgen]
pub struct App {
    game: Game,
    fb: Framebuffer,
    rgba: Vec<u8>,
    input: Input,
    my_id: Option<u32>,
    outgoing: Vec<u8>,
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
            outgoing: Vec::new(),
            send_acc: 0.0,
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

        self.send_acc += dt;
        if self.my_id.is_some() && self.send_acc >= INTERVALO_ENVIO {
            self.send_acc = 0.0;
            let s = self.game.local_state();
            self.outgoing = encode_client(&PlayerState {
                position: [s.position.x, s.position.y, s.position.z],
                yaw: s.yaw,
                pitch: s.pitch,
                model: s.model,
            });
        }
    }

    /// Bytes que JS debe enviar por el WebSocket (vacío si no hay nada).
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outgoing)
    }

    /// Bytes que llegaron del servidor.
    pub fn net_message(&mut self, bytes: &[u8]) {
        match ServerMsg::decode(bytes) {
            Some(ServerMsg::Welcome { id }) => {
                self.my_id = Some(id);
                self.game.set_online(true);
            }
            Some(ServerMsg::Snapshot(players)) => {
                let rivals: Vec<RemoteShip> = players
                    .iter()
                    .filter(|p| Some(p.id) != self.my_id)
                    .take(MAX_PLAYERS)
                    .map(|p| RemoteShip {
                        id: p.id,
                        position: Vec3::new(p.state.position[0], p.state.position[1], p.state.position[2]),
                        yaw: p.state.yaw,
                        pitch: p.state.pitch,
                        model: p.state.model,
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
