/// Buffer de color 0x00RRGGBB con su z-buffer.
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    /// Profundidad NDC por píxel (-1 cerca, 1 lejos). Más pequeño = más cerca.
    pub depth: Vec<f32>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
            depth: vec![f32::INFINITY; width * height],
        }
    }

    /// Cambia el tamaño (pantalla completa o ventana redimensionada). Deja el contenido en negro.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.pixels = vec![0; width * height];
        self.depth = vec![f32::INFINITY; width * height];
    }

    /// Limpia el color y reinicia el z-buffer.
    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
        self.depth.fill(f32::INFINITY);
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pixel_fuera_de_rango_no_paniquea() {
        let mut fb = Framebuffer::new(4, 4);
        fb.set_pixel(10, 10, 0xFFFFFF);
        fb.set_pixel(1, 2, 0xFF0000);
        assert_eq!(fb.pixels[2 * 4 + 1], 0xFF0000);
    }

    #[test]
    fn resize_ajusta_color_y_zbuffer() {
        let mut fb = Framebuffer::new(2, 2);
        fb.resize(5, 3);
        assert_eq!((fb.width, fb.height, fb.pixels.len(), fb.depth.len()), (5, 3, 15, 15));
    }

    #[test]
    fn clear_reinicia_el_zbuffer() {
        let mut fb = Framebuffer::new(2, 2);
        fb.depth[0] = 0.5;
        fb.clear(0);
        assert!(fb.depth[0].is_infinite());
    }
}
