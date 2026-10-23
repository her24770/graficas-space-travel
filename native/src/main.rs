use engine::{Framebuffer, Game};
use minifb::{Key, Window, WindowOptions};
use std::time::Instant;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

fn main() {
    let mut fb = Framebuffer::new(WIDTH, HEIGHT);
    let mut game = Game::new();
    let mut window = Window::new("Space Travel", WIDTH, HEIGHT, WindowOptions::default())
        .expect("no se pudo crear la ventana");
    window.set_target_fps(60);

    let mut last = Instant::now();
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32();
        last = now;

        game.update(dt);
        game.render(&mut fb);
        window
            .update_with_buffer(&fb.pixels, WIDTH, HEIGHT)
            .expect("no se pudo mostrar el framebuffer");
    }
}
