//! Envoltura WASM del engine: el canvas solo muestra el framebuffer, sin WebGL.
use engine::{Framebuffer, Game, Input};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct App {
    game: Game,
    fb: Framebuffer,
    rgba: Vec<u8>,
    input: Input,
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
