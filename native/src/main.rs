mod sound;

use engine::{input::bits, Framebuffer, Game, Input};
use minifb::{Key, KeyRepeat, ScaleMode, Window, WindowOptions};
use std::time::Instant;

/// Se dibuja con una altura fija y el ancho sigue la proporción de la ventana:
/// al agrandarla o ponerla en pantalla completa se ve nítida sin volverse lenta.
const ALTO_INTERNO: usize = 540;

const TECLAS: [(Key, u32); 10] = [
    (Key::W, bits::FORWARD),
    (Key::S, bits::BACKWARD),
    (Key::A, bits::YAW_LEFT),
    (Key::D, bits::YAW_RIGHT),
    (Key::Left, bits::YAW_LEFT),
    (Key::Right, bits::YAW_RIGHT),
    (Key::Up, bits::PITCH_UP),
    (Key::Down, bits::PITCH_DOWN),
    (Key::Space, bits::BRAKE),
    (Key::F, bits::FIRE),
];

fn medidas(ventana: (usize, usize)) -> (usize, usize) {
    let ancho = (ALTO_INTERNO as f32 * ventana.0 as f32 / ventana.1.max(1) as f32).round() as usize / 2 * 2;
    (ancho.clamp(320, 2400), ALTO_INTERNO)
}

fn main() {
    let options = WindowOptions { resize: true, scale_mode: ScaleMode::Stretch, ..WindowOptions::default() };
    let mut window = Window::new("Space Travel", 960, 540, options).expect("no se pudo crear la ventana");
    window.set_target_fps(60);

    let (w, h) = medidas(window.get_size());
    let mut fb = Framebuffer::new(w, h);
    let mut game = Game::new(1);
    let mut sonidos = sound::Sonidos::new();

    let mut last = Instant::now();
    let mut frame = 0u32;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32();
        last = now;

        let (w, h) = medidas(window.get_size());
        if (w, h) != (fb.width, fb.height) {
            fb.resize(w, h);
        }

        let mut b = 0;
        for (tecla, bit) in TECLAS {
            if window.is_key_down(tecla) {
                b |= bit;
            }
        }
        if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            b |= bits::CYCLE_MODEL;
        }
        if window.is_key_pressed(Key::Enter, KeyRepeat::No) {
            b |= bits::START;
        }
        for (tecla, bit) in [
            (Key::M, bits::MENU),
            (Key::G, bits::GRAVITY),
            (Key::O, bits::ORBIT),
            (Key::V, bits::VIEW),
            (Key::R, bits::RESEED),
            (Key::P, bits::CHASE),
        ] {
            if window.is_key_pressed(tecla, KeyRepeat::No) {
                b |= bit;
            }
        }

        game.update(dt, &Input::from_bits(b));
        let choques = game.take_collisions();
        if let Some(s) = sonidos.as_mut() {
            let jugando = game.screen() == engine::Screen::Playing;
            let (cercania_agujero, cercania_ovni) = if jugando { (game.black_hole_proximity(), game.ufo_proximity()) } else { (0.0, 0.0) };
            s.actualizar(dt, jugando && b & bits::FORWARD != 0, &choques, game.take_gates_passed(), cercania_agujero, cercania_ovni);
        }
        game.render(&mut fb);
        window
            .update_with_buffer(&fb.pixels, fb.width, fb.height)
            .expect("no se pudo mostrar el framebuffer");

        frame += 1;
        if frame.is_multiple_of(15) {
            window.set_title(&game.status());
        }
    }
}
