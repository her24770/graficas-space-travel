//! Fragment shaders propios: se ejecutan por píxel con la posición, la normal y la
//! posición local (del objeto) interpoladas. Los planetas usan ruido procedural, así
//! cada uno tiene identidad visible y su textura gira junto con él.
use crate::Vec3;

/// Datos de la escena que ven todos los shaders.
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    pub time: f32,
    /// Posición de la luz (el sol).
    pub light_pos: Vec3,
    pub eye: Vec3,
}

/// Lo que recibe un shader por cada píxel.
#[derive(Clone, Copy, Debug)]
pub struct Fragment {
    pub world: Vec3,
    pub normal: Vec3,
    /// Posición en el espacio del objeto: sirve de coordenada de textura 3D.
    pub local: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Material {
    /// Color liso con luz del sol.
    Flat { color: u32 },
    /// Color que no depende de la luz (puntos de control).
    Emissive { color: u32 },
    /// Metal pulido con mezcla de tinte para las naves
    Metal { color: u32, tint: Option<u32> },
    /// Superficie ardiente que se anima con el tiempo.
    Sun,
    /// Mares, continentes, picos y casquetes polares.
    Rocky { sea: u32, land: u32, peak: u32, seed: f32 },
    /// Bandas de nubes distorsionadas.
    Gas { a: u32, b: u32, seed: f32 },
    /// Hielo con grietas.
    Ice { a: u32, b: u32, seed: f32 },
    /// Anillos con bandas, huecos y ruido fino. Donde son poco densos el píxel se descarta
    Ring { a: u32, b: u32 },
    /// Roca oscura con cráteres y grietas de lava que brillan
    Meteor,
}

impl Material {
    /// Color representativo (para el minimapa).
    pub fn tint(&self) -> u32 {
        match *self {
            Material::Flat { color } | Material::Emissive { color } | Material::Metal { color, .. } => color,
            Material::Sun => 0xFFC107,
            Material::Rocky { land, .. } => land,
            Material::Gas { a, .. } | Material::Ice { a, .. } | Material::Ring { a, .. } => a,
            Material::Meteor => 0xD02A05,
        }
    }
}

fn rgb(c: u32) -> Vec3 {
    Vec3::new(((c >> 16) & 0xFF) as f32, ((c >> 8) & 0xFF) as f32, (c & 0xFF) as f32) * (1.0 / 255.0)
}

fn pack(c: Vec3) -> u32 {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (ch(c.x) << 16) | (ch(c.y) << 8) | ch(c.z)
}

/// Mezcla dos colores 0xRRGGBB
pub fn blend(a: u32, b: u32, t: f32) -> u32 {
    pack(rgb(a).lerp(rgb(b), t))
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263)
        ^ (z as u32).wrapping_mul(2_147_483_647);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}

/// Ruido de valor 3D en [0, 1]: interpola con suavizado los valores en las esquinas de la celda.
pub fn value_noise(p: Vec3) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, ty, tz) = (s(p.x - fx), s(p.y - fy), s(p.z - fz));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx, dy, dz| hash(ix + dx, iy + dy, iz + dz);
    let x00 = l(c(0, 0, 0), c(1, 0, 0), tx);
    let x10 = l(c(0, 1, 0), c(1, 1, 0), tx);
    let x01 = l(c(0, 0, 1), c(1, 0, 1), tx);
    let x11 = l(c(0, 1, 1), c(1, 1, 1), tx);
    l(l(x00, x10, ty), l(x01, x11, ty), tz)
}

/// Suma de varias octavas de ruido: más detalle en cada una. Resultado en [0, 1].
pub fn fbm(p: Vec3, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut q) = (0.0, 0.5, 0.0, p);
    for _ in 0..octaves {
        sum += amp * value_noise(q);
        norm += amp;
        amp *= 0.5;
        q = q * 2.03 + Vec3::new(17.1, 3.7, 9.2);
    }
    sum / norm
}

/// Nivel del mar de los planetas rocosos: por debajo de este valor de ruido es agua.
pub const SEA_LEVEL: f32 = 0.48;

/// Ruido de altura de un planeta rocoso en la dirección `dir` (unitaria, espacio local).
pub fn rocky_height(dir: Vec3, seed: f32) -> f32 {
    fbm(dir * 3.0 + Vec3::new(seed, seed * 0.7, seed * 1.3), 5)
}

/// Ruido de altura de un planeta de hielo en la dirección `dir` (unitaria, espacio local).
pub fn ice_height(dir: Vec3, seed: f32) -> f32 {
    fbm(dir * 5.0 + Vec3::new(seed, 0.0, seed), 4)
}

/// Brillo de la atmósfera en el borde del planeta.
fn rim(normal: Vec3, view: Vec3, lit: f32) -> f32 {
    (1.0 - normal.dot(view).max(0.0)).powi(3) * (0.25 + lit)
}

/// Radio interior de los anillos (el exterior es 1).
const RING_INNER: f32 = 0.565;

/// Ruido fino y densidad de los anillos en la posición local `l` (el disco está en el plano XZ).
fn ring_profile(l: Vec3) -> (f32, f32) {
    let r = (l.x * l.x + l.z * l.z).sqrt();
    let t = (r - RING_INNER) / (1.0 - RING_INNER);
    let gap = smoothstep(0.52, 0.56, t) * (1.0 - smoothstep(0.62, 0.66, t));
    let edge = smoothstep(0.0, 0.04, t) * (1.0 - smoothstep(0.96, 1.0, t));
    let fine = fbm(Vec3::new(t * 60.0, 0.0, 0.0), 3);
    (fine, edge * (1.0 - gap * 0.95) * (0.45 + 0.55 * fine))
}

pub fn discards(m: &Material, f: &Fragment) -> bool {
    matches!(m, Material::Ring { .. }) && ring_profile(f.local).1 < 0.18
}

pub fn shade(m: &Material, f: &Fragment, s: &Scene) -> u32 {
    let n = f.normal.normalize();
    let lambert = n.dot((s.light_pos - f.world).normalize()).max(0.0);
    let view = (s.eye - f.world).normalize();
    let p = f.local.normalize();

    match *m {
        Material::Flat { color } => pack(rgb(color) * (0.25 + 0.75 * lambert)),
        Material::Emissive { color } => pack(rgb(color)),
        Material::Metal { color, .. } => {
            let base = rgb(color);
            let l = (s.light_pos - f.world).normalize();
            let h = (l + view).normalize();
            let spec = n.dot(h).max(0.0).powi(48) * if lambert > 0.0 { 1.0 } else { 0.0 };
            // Reflejo falso: el espacio oscuro arriba y un horizonte claro, según hacia dónde mira la normal.
            let r = n * (2.0 * n.dot(view)) - view;
            let env = Vec3::new(0.05, 0.07, 0.12).lerp(Vec3::new(0.75, 0.82, 0.95), smoothstep(-0.4, 0.4, r.y));
            let fresnel = (1.0 - n.dot(view).max(0.0)).powi(4);
            let c = base * (0.10 + 0.35 * lambert)
                + base.lerp(env, 0.5) * (0.35 * (0.3 + 0.7 * lambert) + 0.4 * fresnel)
                + base.lerp(Vec3::new(1.0, 1.0, 1.0), 0.45) * spec * 1.1;
            pack(c)
        }
        Material::Sun => {
            let t = s.time;
            let a = fbm(p * 2.5 + Vec3::new(t * 0.12, 0.0, t * 0.07), 4);
            let b = fbm(p * 6.0 - Vec3::new(0.0, t * 0.2, 0.0), 3);
            let heat = (a * 0.7 + b * 0.5).clamp(0.0, 1.0);
            pack(rgb(0xFF4500).lerp(rgb(0xFFF2A8), heat) * (0.9 + 0.3 * heat))
        }
        Material::Rocky { sea, land, peak, seed } => {
            let h = rocky_height(p, seed);
            let ground = if h < SEA_LEVEL {
                rgb(sea) * 0.6 + rgb(sea) * (0.4 * h / SEA_LEVEL)
            } else {
                let t = ((h - SEA_LEVEL) / (1.0 - SEA_LEVEL)).clamp(0.0, 1.0);
                rgb(land).lerp(rgb(peak), t * t)
            };
            let polar = smoothstep(0.78, 0.9, p.y.abs() + (h - 0.5) * 0.15);
            let c = ground.lerp(Vec3::new(0.95, 0.97, 1.0), polar);
            pack(c * (0.10 + 0.90 * lambert) + Vec3::new(0.3, 0.5, 1.0) * (rim(n, view, lambert) * 0.6))
        }
        Material::Gas { a, b, seed } => {
            let warp = fbm(Vec3::new(p.x * 2.0 + seed, p.y * 5.0, p.z * 2.0 + seed), 3);
            let band = 0.5 + 0.5 * (p.y * 9.0 + warp * 3.0).sin();
            let c = rgb(a).lerp(rgb(b), band);
            pack(c * (0.12 + 0.88 * lambert) + rgb(a) * (rim(n, view, lambert) * 0.5))
        }
        Material::Ice { a, b, seed } => {
            let h = ice_height(p, seed);
            let crack = 1.0 - smoothstep(0.0, 0.03, (h - 0.5).abs());
            let c = rgb(a).lerp(rgb(b), h) * (1.0 - 0.5 * crack);
            pack(c * (0.12 + 0.88 * lambert) + Vec3::new(0.6, 0.8, 1.0) * (rim(n, view, lambert) * 0.5))
        }
        Material::Ring { a, b } => {
            let (fine, density) = ring_profile(f.local);
            let color = rgb(a).lerp(rgb(b), smoothstep(0.3, 0.7, fine)).lerp(rgb(0x3d3226), (1.0 - density) * 0.6);
            // Se ven las dos caras: la luz cuenta igual venga de arriba o de abajo.
            let luz = n.dot((s.light_pos - f.world).normalize()).abs();
            pack(color * (0.25 + 0.75 * luz))
        }
        Material::Meteor => {
            let q = f.local;
            let t = s.time;
            // Roca quemada con cráteres 
            let rock = fbm(q * 4.0, 5);
            let mut crust = rgb(0x1a1210).lerp(rgb(0x3b2a24), rock);
            let cr = fbm(q * 3.5 + Vec3::new(1.7, 8.3, 2.9), 2);
            crust = crust.lerp(rgb(0x0b0706), smoothstep(0.30, 0.22, cr) * 0.8);
            crust = crust.lerp(rgb(0x5a4638), smoothstep(0.30, 0.34, cr) * smoothstep(0.40, 0.34, cr) * 0.6);
            // Grietas de lava: donde el ruido cruza 0.5.
            let h = fbm(q * 3.0 + Vec3::new(0.0, t * 0.03, 0.0), 5);
            let vein = 1.0 - smoothstep(0.0, 0.07, (h - 0.5).abs());
            let magma = rgb(0xd02a05).lerp(rgb(0xffc21f), smoothstep(0.4, 1.0, vein));
            let pulse = 0.75 + 0.25 * (t * 1.5 + rock * 6.0).sin();
            let lit = crust * (0.08 + 0.92 * lambert) * (1.0 - vein) + (crust.lerp(magma, vein) + magma * (vein * 0.35 * pulse)) * vein;
            pack(lit + rgb(0xff4a12) * (rim(n, view, lambert) * 0.5))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escena(t: f32) -> Scene {
        Scene { time: t, light_pos: Vec3::new(0.0, 0.0, 100.0), eye: Vec3::new(0.0, 0.0, 10.0) }
    }

    fn frag(local: Vec3) -> Fragment {
        Fragment { world: local, normal: local.normalize(), local }
    }

    #[test]
    fn el_ruido_es_determinista_y_queda_en_rango() {
        for i in 0..500 {
            let p = Vec3::new(i as f32 * 0.37, i as f32 * 0.11, i as f32 * 0.73);
            let (a, b) = (fbm(p, 4), fbm(p, 4));
            assert_eq!(a, b);
            assert!((0.0..=1.0).contains(&a), "{a}");
        }
    }

    #[test]
    fn el_ruido_es_continuo() {
        let p = Vec3::new(1.3, 2.7, 0.4);
        let q = p + Vec3::new(0.001, 0.0, 0.0);
        assert!((value_noise(p) - value_noise(q)).abs() < 0.01);
    }

    #[test]
    fn los_anillos_dejan_huecos_y_solo_ellos_descartan_pixeles() {
        let anillo = Material::Ring { a: 0xD8C49A, b: 0x8A7556 };
        assert!(discards(&anillo, &frag(Vec3::new(0.3, 0.0, 0.0))));
        assert!(discards(&anillo, &frag(Vec3::new(1.2, 0.0, 0.0))));
        let dibujados = (0..100).filter(|&i| !discards(&anillo, &frag(Vec3::new(0.57 + i as f32 * 0.004, 0.0, 0.0)))).count();
        assert!(dibujados > 20, "{dibujados}");
        assert!(!discards(&Material::Meteor, &frag(Vec3::new(0.3, 0.0, 0.0))));
    }

    #[test]
    fn el_sol_se_anima_con_el_tiempo() {
        let f = frag(Vec3::new(0.3, 0.5, 0.8));
        assert_ne!(shade(&Material::Sun, &f, &escena(0.0)), shade(&Material::Sun, &f, &escena(5.0)));
    }

    #[test]
    fn los_planetas_cambian_de_color_segun_la_posicion() {
        let m = Material::Rocky { sea: 0x1E4FA8, land: 0x3E8E41, peak: 0x8D6E63, seed: 3.0 };
        let colores: std::collections::HashSet<u32> =
            (0..40).map(|i| shade(&m, &frag(Vec3::new((i as f32 * 0.7).sin(), (i as f32 * 0.3).cos(), 0.5)), &escena(0.0))).collect();
        assert!(colores.len() > 10, "debería haber variedad de colores");
    }

    #[test]
    fn el_lado_oscuro_es_mas_oscuro_que_el_iluminado() {
        let m = Material::Flat { color: 0xFFFFFF };
        let luz = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, 1.0), local: Vec3::ZERO }, &escena(0.0));
        let sombra = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, -1.0), local: Vec3::ZERO }, &escena(0.0));
        assert!((luz & 0xFF) > (sombra & 0xFF));
    }

    #[test]
    fn el_metal_brilla_mas_de_cara_a_la_luz_que_de_espaldas() {
        let m = Material::Metal { color: 0x8899AA, tint: None };
        let escena = Scene { time: 0.0, light_pos: Vec3::new(0.0, 0.0, 10.0), eye: Vec3::new(0.0, 0.0, 10.0) };
        let luz = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, 1.0), local: Vec3::ZERO }, &escena);
        let sombra = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, -1.0), local: Vec3::ZERO }, &escena);
        assert!(luz > sombra);
    }

    #[test]
    fn el_emisivo_ignora_la_luz() {
        let m = Material::Emissive { color: 0x123456 };
        let a = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, 1.0), local: Vec3::ZERO }, &escena(0.0));
        let b = shade(&m, &Fragment { world: Vec3::ZERO, normal: Vec3::new(0.0, 0.0, -1.0), local: Vec3::ZERO }, &escena(0.0));
        assert_eq!((a, b), (0x123456, 0x123456));
    }
}
