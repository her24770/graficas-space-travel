/// Buffer de color 0x00RRGGBB.
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![0; width * height] }
    }

    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
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
}
