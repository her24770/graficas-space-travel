// Chispas de las colisiones, se proyectan a la pantalla y se pintan como píxeles 
use crate::{rng::Rng, Framebuffer, Mat4, Vec3};

/// Máximo de chispas vivas a la vez
const MAX: usize = 160;
const VIDA: (f32, f32) = (0.35, 0.8);
const CAIDA: f32 = 8.0;
/// Largo de la estela en segundos de su movimiento.
const ESTELA_S: f32 = 0.05;
const TOLERANCIA: f32 = 0.6;
const COLORES: [u32; 3] = [0xFFF3B0, 0xFF7A00, 0x7A1200];

struct Spark {
    pos: Vec3,
    /// Velocidad respecto al cuerpo contra el que chocó.
    rel: Vec3,
    /// Velocidad del cuerpo en el choque para mover las chispas junto al cuerpo
    arrastre: Vec3,
    caida: Vec3,
    edad: f32,
    vida: f32,
}

pub struct Sparks {
    list: Vec<Spark>,
    rng: Rng,
}

impl Default for Sparks {
    fn default() -> Self {
        Self::new()
    }
}

impl Sparks {
    pub fn new() -> Self {
        Self { list: Vec::new(), rng: Rng::new(0x5BA2) }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Lanza un abanico de chispas abierto alrededor de la normal
    ///  Salen más y más rápido mientras más fuerte fue el golpe.
    pub fn burst(&mut self, pos: Vec3, normal: Vec3, body_velocity: Vec3, impact_speed: f32) {
        let n = (10.0 + impact_speed * 0.8).min(40.0) as usize;
        let eje = if normal.y.abs() < 0.9 { Vec3::Y } else { Vec3::new(1.0, 0.0, 0.0) };
        let a = normal.cross(eje).normalize();
        let b = normal.cross(a);
        for _ in 0..n {
            if self.list.len() >= MAX {
                self.list.remove(0);
            }
            let fi = self.rng.range(0.0, std::f32::consts::TAU);
            let elevacion = self.rng.range(0.25, 1.0);
            let lateral = (1.0 - elevacion * elevacion).sqrt();
            let dir = normal * elevacion + (a * fi.cos() + b * fi.sin()) * lateral;
            let rapidez = self.rng.range(4.0, 10.0) + impact_speed * 0.4;
            self.list.push(Spark {
                pos,
                rel: dir * rapidez,
                arrastre: body_velocity,
                caida: -normal * CAIDA,
                edad: 0.0,
                vida: self.rng.range(VIDA.0, VIDA.1),
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        for s in &mut self.list {
            s.edad += dt;
            s.rel += s.caida * dt;
            s.pos += (s.rel + s.arrastre) * dt;
        }
        self.list.retain(|s| s.edad < s.vida);
    }

    pub fn draw(&self, fb: &mut Framebuffer, view_proj: &Mat4) {
        let grosor = (fb.height / 270).max(1) as i32;
        for s in &self.list {
            let c = view_proj.transform_point(s.pos);
            if c.w <= 1e-4 || c.z / c.w > 1.0 {
                continue;
            }
            let Some((x1, y1, _)) = crate::raster::project(view_proj, s.pos, fb.width, fb.height) else { continue };
            // La estela sale de donde estaba la chispa un instante antes.
            let atras = s.pos - s.rel * ESTELA_S;
            let (x0, y0) = crate::raster::project(view_proj, atras, fb.width, fb.height).map_or((x1, y1), |p| (p.0, p.1));

            let color = color_en(s.edad / s.vida);
            let z = c.z / c.w;
            let tolerancia = TOLERANCIA / (c.w * c.w);
            let pasos = (x1 - x0).abs().max((y1 - y0).abs()).ceil().clamp(1.0, 24.0) as i32;
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
    }
}

fn color_en(t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0) * (COLORES.len() - 1) as f32;
    let i = (t as usize).min(COLORES.len() - 2);
    let k = t - i as f32;
    let canal = |c: u32, sh: u32| ((c >> sh) & 0xFF) as f32;
    let mezcla = |sh: u32| (canal(COLORES[i], sh) + (canal(COLORES[i + 1], sh) - canal(COLORES[i], sh)) * k) as u32;
    (mezcla(16) << 16) | (mezcla(8) << 8) | mezcla(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lanzar(impacto: f32) -> Sparks {
        let mut s = Sparks::new();
        s.burst(Vec3::ZERO, Vec3::Y, Vec3::ZERO, impacto);
        s
    }

    #[test]
    fn un_golpe_fuerte_saca_mas_chispas_que_uno_suave() {
        assert!(lanzar(30.0).len() > lanzar(2.0).len());
    }

    #[test]
    fn nunca_hay_mas_del_maximo() {
        let mut s = Sparks::new();
        for _ in 0..20 {
            s.burst(Vec3::ZERO, Vec3::Y, Vec3::ZERO, 100.0);
        }
        assert!(s.len() <= MAX);
    }

    #[test]
    fn salen_hacia_afuera_de_la_superficie() {
        let s = lanzar(10.0);
        assert!(s.list.iter().all(|c| c.rel.dot(Vec3::Y) > 0.0));
    }

    #[test]
    fn se_apagan_con_el_tiempo_sin_depender_de_los_cuadros() {
        let (mut a, mut b) = (lanzar(10.0), lanzar(10.0));
        for _ in 0..10 {
            a.update(0.1);
        }
        for _ in 0..100 {
            b.update(0.01);
        }
        assert!(a.is_empty() && b.is_empty(), "a 1 s ya no queda ninguna");
    }

    #[test]
    fn heredan_la_velocidad_del_cuerpo() {
        let mut s = Sparks::new();
        s.burst(Vec3::ZERO, Vec3::Y, Vec3::new(100.0, 0.0, 0.0), 5.0);
        s.update(0.1);
        assert!(s.list.iter().all(|c| c.pos.x > 5.0));
    }

    #[test]
    fn el_color_va_de_claro_a_oscuro() {
        assert_eq!(color_en(0.0), COLORES[0]);
        assert_eq!(color_en(1.0), COLORES[2]);
    }

    #[test]
    fn dibuja_pixeles_visibles_y_respeta_la_profundidad() {
        let vp = Mat4::perspective(1.0, 1.0, 0.5, 100.0) * Mat4::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let mut s = Sparks::new();
        s.burst(Vec3::ZERO, Vec3::Y, Vec3::ZERO, 5.0);

        let mut libre = Framebuffer::new(64, 64);
        libre.clear(0);
        s.draw(&mut libre, &vp);
        assert!(libre.pixels.iter().any(|&p| p != 0), "se dibujó alguna chispa");

        let mut tapado = Framebuffer::new(64, 64);
        tapado.clear(0);
        tapado.depth.fill(-1.0);
        s.draw(&mut tapado, &vp);
        assert!(tapado.pixels.iter().all(|&p| p == 0));
    }
}
