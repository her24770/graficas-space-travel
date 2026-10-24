use engine::{input::bits, Framebuffer, Game, Input};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::time::Instant;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

const TECLAS: [(Key, u32); 7] = [
    (Key::W, bits::FORWARD),
    (Key::S, bits::BACKWARD),
    (Key::A, bits::YAW_LEFT),
    (Key::D, bits::YAW_RIGHT),
    (Key::Up, bits::PITCH_UP),
    (Key::Down, bits::PITCH_DOWN),
    (Key::Space, bits::BRAKE),
];

fn main() {
    let mut fb = Framebuffer::new(WIDTH, HEIGHT);
    let mut game = Game::new(1);
    let mut window = Window::new("Space Travel", WIDTH, HEIGHT, WindowOptions::default())
        .expect("no se pudo crear la ventana");
    window.set_target_fps(60);

    let mut last = Instant::now();
    let mut frame = 0u32;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32();
        last = now;

        let mut b = 0;
        for (tecla, bit) in TECLAS {
            if window.is_key_down(tecla) {
                b |= bit;
            }
        }
        if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            b |= bits::CYCLE_MODEL;
        }

        game.update(dt, &Input::from_bits(b));
        game.render(&mut fb);
        window
            .update_with_buffer(&fb.pixels, WIDTH, HEIGHT)
            .expect("no se pudo mostrar el framebuffer");

        frame += 1;
        if frame.is_multiple_of(15) {
            window.set_title(&game.status());
        }
    }
}
