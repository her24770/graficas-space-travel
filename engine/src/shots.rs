//! Disparos de la nave: proyectiles en línea recta que viven poco tiempo y se dibujan como rayos.
use crate::{Framebuffer, Mat4, Vec3};

/// Rapidez del disparo respecto a la nave (u/s).
pub const VELOCIDAD: f32 = 260.0;
/// Segundos entre un disparo y el siguiente mientras se mantiene la tecla.
pub const CADENCIA_S: f32 = 0.18;
const VIDA_S: f32 = 1.4;
/// Largo del rayo que se dibuja (u).
const LARGO: f32 = 14.0;
const COLOR: u32 = 0x80F0FF;
const TOLERANCIA: f32 = 0.6;

#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub pos: Vec3,
    pub vel: Vec3,
    edad: f32,
}

impl Shot {
    /// Tramo que recorrió en el último `dt`: los choques se prueban contra todo el tramo para que un
    /// disparo rápido no atraviese un objeto entre un cuadro y otro.
    pub fn tramo(&self, dt: f32) -> (Vec3, Vec3) {
        (self.pos - self.vel * dt, self.pos)
    }
}

#[derive(Default)]
pub struct Shots {
    list: Vec<Shot>,
    recarga: f32,
}

impl Shots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.recarga = 0.0;
    }

    /// Dispara desde `from` hacia `dir` si ya pasó la cadencia. La velocidad de la nave se suma
    /// para que el rayo no se quede atrás cuando la nave va rápido.
    pub fn fire(&mut self, from: Vec3, dir: Vec3, ship_vel: Vec3) -> bool {
        if self.recarga > 0.0 {
            return false;
        }
        self.list.push(Shot { pos: from, vel: dir * VELOCIDAD + ship_vel, edad: 0.0 });
        self.recarga = CADENCIA_S;
        true
    }

    pub fn update(&mut self, dt: f32) {
        self.recarga = (self.recarga - dt).max(0.0);
        for s in &mut self.list {
            s.edad += dt;
            s.pos += s.vel * dt;
        }
        self.list.retain(|s| s.edad < VIDA_S);
    }

    /// Quita los disparos para los que `sigue` devuelve false (por ejemplo, los que pegaron en algo).
    pub fn retain(&mut self, sigue: impl FnMut(&Shot) -> bool) {
        self.list.retain(sigue);
    }

    pub fn draw(&self, fb: &mut Framebuffer, view_proj: &Mat4) {
        let grosor = 2 * (fb.height / 270).max(1) as i32;
        for s in &self.list {
            let cola = s.pos - s.vel.normalize() * LARGO;
            draw_segment(fb, view_proj, cola, s.pos, COLOR, grosor);
        }
    }
}

/// ¿El segmento `a`-`b` pasa a menos de `r` del punto `c`?
pub fn segment_hits_sphere(a: Vec3, b: Vec3, c: Vec3, r: f32) -> bool {
    let ab = b - a;
    let t = if ab.dot(ab) > 1e-9 { ((c - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance(c) <= r
}

/// Línea 3D con prueba de profundidad (se tapa con lo que esté delante).
fn draw_segment(fb: &mut Framebuffer, view_proj: &Mat4, a: Vec3, b: Vec3, color: u32, grosor: i32) {
    let c = view_proj.transform_point(b);
    if c.w <= 1e-4 || c.z / c.w > 1.0 {
        return;
    }
    let Some((x1, y1, _)) = crate::raster::project(view_proj, b, fb.width, fb.height) else { return };
    let (x0, y0) = crate::raster::project(view_proj, a, fb.width, fb.height).map_or((x1, y1), |p| (p.0, p.1));
    let z = c.z / c.w;
    let tolerancia = TOLERANCIA / (c.w * c.w);
    let pasos = (x1 - x0).abs().max((y1 - y0).abs()).ceil().clamp(1.0, 64.0) as i32;
    for i in 0..=pasos {
        let k = i as f32 / pasos as f32;
        let (x, y) = ((x0 + (x1 - x0) * k) as i32, (y0 + (y1 - y0) * k) as i32);
        for dy in 0..grosor {
            for dx in 0..grosor {
                let (px, py) = (x + dx, y + dy);
                if px < 0 || py < 0 || px >= fb.width as i32 || py >= fb.height as i32 {
                    continue;
                }
                let idx = py as usize * fb.width + px as usize;
                if z <= fb.depth[idx] + tolerancia {
                    fb.pixels[idx] = color;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respeta_la_cadencia_y_los_disparos_se_apagan() {
        let mut s = Shots::new();
        assert!(s.fire(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO));
        assert!(!s.fire(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO), "aún recargando");
        s.update(CADENCIA_S);
        assert!(s.fire(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO));
        assert_eq!(s.len(), 2);
        for _ in 0..40 {
            s.update(0.05);
        }
        assert!(s.is_empty());
    }

    #[test]
    fn el_tramo_detecta_un_objeto_que_quedo_entre_dos_cuadros() {
        let mut s = Shots::new();
        s.fire(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO);
        s.update(0.05); // avanza 13 u de golpe
        let (a, b) = s.list[0].tramo(0.05);
        assert!(segment_hits_sphere(a, b, Vec3::new(6.0, 0.5, 0.0), 1.0));
        assert!(!segment_hits_sphere(a, b, Vec3::new(6.0, 5.0, 0.0), 1.0));
    }
}
