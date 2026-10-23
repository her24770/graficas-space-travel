use crate::Framebuffer;

/// Estado del juego. Native y web llaman a la misma lógica: `update(dt)` y `render(fb)`.
pub struct Game {
    /// Segundos acumulados; todo movimiento depende de esto, no de los cuadros.
    pub time: f32,
}

impl Game {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Placeholder: un círculo que pulsa y orbita, para comprobar que native y web corren lo mismo.
    pub fn render(&self, fb: &mut Framebuffer) {
        fb.clear(0x000008);
        let (w, h) = (fb.width as f32, fb.height as f32);
        let cx = w / 2.0 + (self.time * 1.5).cos() * w * 0.25;
        let cy = h / 2.0 + (self.time * 1.5).sin() * h * 0.25;
        let r = h * 0.08 * (1.0 + 0.2 * (self.time * 4.0).sin());
        let (x0, x1) = ((cx - r).max(0.0) as usize, ((cx + r) as usize).min(fb.width - 1));
        let (y0, y1) = ((cy - r).max(0.0) as usize, ((cy + r) as usize).min(fb.height - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                if dx * dx + dy * dy <= r * r {
                    fb.set_pixel(x, y, 0xFFB300);
                }
            }
        }
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_tiempo_avanza_con_dt() {
        let mut g = Game::new();
        g.update(0.5);
        g.update(0.25);
        assert!((g.time - 0.75).abs() < 1e-6);
    }
}
