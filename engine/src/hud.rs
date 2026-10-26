//! Texto en pantalla con una tipografía de mapa de bits 5x7 propia (solo mayúsculas, sin tildes).
use crate::Framebuffer;

#[rustfmt::skip]
static GLYPHS: &[(char, [u8; 7])] = &[
    (' ', [0, 0, 0, 0, 0, 0, 0]),
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
    ('3', [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
    (':', [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000]),
    ('.', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100]),
    ('-', [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
    ('/', [0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000]),
    ('!', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100]),
    ('=', [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000]),
    ('(', [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010]),
    (')', [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000]),
    ('%', [0b11000, 0b11001, 0b00010, 0b00100, 0b01000, 0b10011, 0b00011]),
    ('+', [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000]),
    (',', [0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b00100, 0b01000]),
    ('<', [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010]),
    ('>', [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000]),
    ('[', [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110]),
    (']', [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110]),
    ('*', [0b00000, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0b00000]),
    ('_', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111]),
];

fn glyph(c: char) -> &'static [u8; 7] {
    let c = c.to_ascii_uppercase();
    GLYPHS.iter().find(|(g, _)| *g == c).map(|(_, rows)| rows).unwrap_or(&GLYPHS[0].1)
}

pub fn rect(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for yy in y.max(0)..(y + h).min(fb.height as i32) {
        for xx in x.max(0)..(x + w).min(fb.width as i32) {
            fb.pixels[yy as usize * fb.width + xx as usize] = color;
        }
    }
}

/// Círculo relleno.
pub fn dot(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, color: u32) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                rect(fb, cx + x, cy + y, 1, 1, color);
            }
        }
    }
}

/// Ancho en píxeles de `s` dibujado con `scale`.
pub fn width(s: &str, scale: usize) -> i32 {
    (s.chars().count() * 6 * scale) as i32 - scale as i32
}

pub fn text(fb: &mut Framebuffer, x: i32, y: i32, scale: usize, color: u32, s: &str) {
    let k = scale as i32;
    for (i, c) in s.chars().enumerate() {
        let gx = x + i as i32 * 6 * k;
        for (row, bits) in glyph(c).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    rect(fb, gx + col * k, y + row as i32 * k, k, k, color);
                }
            }
        }
    }
}

/// Texto con sombra, legible sobre cualquier fondo.
pub fn label(fb: &mut Framebuffer, x: i32, y: i32, scale: usize, color: u32, s: &str) {
    text(fb, x + scale as i32, y + scale as i32, scale, 0x000000, s);
    text(fb, x, y, scale, color, s);
}

pub fn centered(fb: &mut Framebuffer, cx: i32, y: i32, scale: usize, color: u32, s: &str) {
    label(fb, cx - width(s, scale) / 2, y, scale, color, s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dibuja_pixeles_y_respeta_los_bordes() {
        let mut fb = Framebuffer::new(40, 20);
        text(&mut fb, 2, 2, 1, 0xFFFFFF, "A1");
        assert!(fb.pixels.contains(&0xFFFFFF));
        text(&mut fb, -50, -50, 3, 0xFFFFFF, "FUERA"); // no debe paniquear
        text(&mut fb, 35, 15, 3, 0xFFFFFF, "BORDE");
    }

    #[test]
    fn el_circulo_queda_dentro_de_su_radio() {
        let mut fb = Framebuffer::new(20, 20);
        dot(&mut fb, 10, 10, 3, 0xFFFFFF);
        assert_eq!(fb.pixels[10 * 20 + 10], 0xFFFFFF);
        assert_eq!(fb.pixels[10 * 20 + 14], 0);
        dot(&mut fb, -5, -5, 3, 0xFFFFFF); // fuera de pantalla: no debe paniquear
    }

    #[test]
    fn los_caracteres_desconocidos_y_minusculas_no_rompen() {
        let mut fb = Framebuffer::new(40, 20);
        text(&mut fb, 0, 0, 1, 0xFFFFFF, "abc \u{00F1}\u{1F600}");
        assert!(width("ABC", 2) > 0);
    }

    #[test]
    fn el_texto_centrado_queda_en_el_medio() {
        let mut fb = Framebuffer::new(100, 20);
        centered(&mut fb, 50, 5, 1, 0xFFFFFF, "II");
        let xs: Vec<usize> = (0..100).filter(|&x| (0..20).any(|y| fb.pixels[y * 100 + x] == 0xFFFFFF)).collect();
        let medio = (xs[0] + xs[xs.len() - 1]) as f32 / 2.0;
        assert!((medio - 50.0).abs() < 3.0, "{medio}");
    }
}
