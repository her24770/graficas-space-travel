//! Mapeo de normales por software.
//!
//! Un mapa de normales es una textura donde cada texel guarda una normal en el *espacio tangente*
//! (x = hacia la derecha de la textura, y = hacia arriba, z = hacia afuera de la superficie).
//! El fragment shader la lee y la gira al mundo con la base T, B, N de la superficie, así la luz
//! "ve" relieve fino (rocas, grietas) que la malla no tiene.
//!
//! Aquí el mapa no es una imagen cargada: se hornea una vez (perezosamente) a partir de un campo de
//! alturas con ruido sobre la esfera. Lo demás es el pipeline normal de un mapa de normales:
//!
//! 1. **Coordenadas de textura** esféricas (`sphere_uv`): `u` = longitud, `v` = latitud.
//! 2. **Tangente por vértice** (`sphere_tangent`): ∂P/∂u. El rasterizador la interpola.
//! 3. **Mipmaps**: cada nivel promedia 2×2 texels. El promedio de normales se acorta solo, de modo
//!    que de lejos el relieve se aplana en vez de centellear.
//! 4. **Muestreo trilineal** (`NormalMap::sample`) con el nivel que calcula el rasterizador por triángulo.
//! 5. **Base TBN** (`perturb`): lleva la normal del espacio tangente al del mundo.
use crate::shaders::{fbm, hash, smoothstep};
use crate::Vec3;
use std::f32::consts::PI;
use std::sync::OnceLock;

/// Tamaño del nivel más fino (la proporción 2:1 es la de una esfera con longitud/latitud).
pub const WIDTH: usize = 1024;
pub const HEIGHT: usize = 512;

/// Qué relieve tiene el mapa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Terreno rocoso: lomas y cordilleras.
    Rock,
    /// Hielo: fracturas finas sobre una superficie rugosa.
    Ice,
}

struct Level {
    w: usize,
    h: usize,
    /// (x, y) de la normal en espacio tangente; z se reconstruye (es siempre positiva).
    texels: Vec<[f32; 2]>,
}

pub struct NormalMap {
    levels: Vec<Level>,
}

/// Coordenadas de textura de la dirección unitaria `p`: `u` crece hacia el este (la dirección de
/// `sphere_tangent`) y `v` hacia el norte (+Y).
pub fn sphere_uv(p: Vec3) -> (f32, f32) {
    (0.5 - p.z.atan2(p.x) / (2.0 * PI), 0.5 + p.y.clamp(-1.0, 1.0).asin() / PI)
}

/// Dirección en que crece `u` sobre la esfera (∂P/∂u normalizado). Es cero justo en los polos,
/// donde la longitud no está definida.
pub fn sphere_tangent(p: Vec3) -> Vec3 {
    Vec3::new(p.z, 0.0, -p.x).normalize()
}

/// Dirección unitaria que corresponde a las coordenadas de textura (inversa de `sphere_uv`).
fn sphere_dir(u: f32, v: f32) -> Vec3 {
    let (theta, phi) = ((0.5 - u) * 2.0 * PI, (v - 0.5) * PI);
    Vec3::new(phi.cos() * theta.cos(), phi.sin(), phi.cos() * theta.sin())
}

/// Lleva `ts` (normal del espacio tangente) al espacio de `n`. `tangent` no tiene que ser
/// perpendicular a `n` (viene interpolada): se ortogonaliza. B = N × T completa una base derecha.
/// Si no hay tangente utilizable devuelve `n` sin tocar.
pub fn perturb(n: Vec3, tangent: Vec3, ts: Vec3) -> Vec3 {
    let t = (tangent - n * n.dot(tangent)).normalize();
    if t == Vec3::ZERO {
        return n;
    }
    let b = n.cross(t);
    (t * ts.x + b * ts.y + n * ts.z).normalize()
}

impl NormalMap {
    /// Hornea un mapa de `w`×`h` desde un campo de alturas sobre la esfera unidad.
    /// `amplitude` es la altura del relieve en radios de la esfera: más grande, normales más inclinadas.
    pub fn from_height(w: usize, h: usize, amplitude: f32, height: impl Fn(Vec3) -> f32) -> Self {
        let mut hs = vec![0.0f32; w * h];
        for j in 0..h {
            for i in 0..w {
                hs[j * w + i] = height(sphere_dir((i as f32 + 0.5) / w as f32, (j as f32 + 0.5) / h as f32));
            }
        }
        let mut texels = vec![[0.0f32; 2]; w * h];
        for j in 0..h {
            // Un texel mide menos de ancho cerca de los polos: la pendiente se mide por longitud de arco real.
            let phi = ((j as f32 + 0.5) / h as f32 - 0.5) * PI;
            let ds_u = (2.0 * PI * phi.cos().max(0.02)) / w as f32;
            let ds_v = PI / h as f32;
            for i in 0..w {
                let at = |ii: usize, jj: usize| hs[jj * w + ii];
                let (l, r) = ((i + w - 1) % w, (i + 1) % w);
                let (d, u) = (j.saturating_sub(1), (j + 1).min(h - 1));
                let du = (at(r, j) - at(l, j)) / (2.0 * ds_u);
                let dv = (at(i, u) - at(i, d)) / ((u - d) as f32 * ds_v);
                // La normal de una superficie z = f(x, y) es (-fx, -fy, 1) normalizada.
                let n = Vec3::new(-du * amplitude, -dv * amplitude, 1.0).normalize();
                texels[j * w + i] = [n.x, n.y];
            }
        }
        let mut levels = vec![Level { w, h, texels }];
        while levels.last().is_some_and(|l| l.w > 4 && l.h > 2) {
            let prev = levels.last().unwrap();
            let (nw, nh) = (prev.w / 2, prev.h / 2);
            let mut texels = Vec::with_capacity(nw * nh);
            for j in 0..nh {
                for i in 0..nw {
                    let mut acc = [0.0f32; 2];
                    for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let t = prev.texels[(2 * j + dj) * prev.w + 2 * i + di];
                        acc[0] += t[0] * 0.25;
                        acc[1] += t[1] * 0.25;
                    }
                    texels.push(acc);
                }
            }
            levels.push(Level { w: nw, h: nh, texels });
        }
        NormalMap { levels }
    }

    /// Cuántos niveles de mipmap tiene.
    pub fn levels(&self) -> usize {
        self.levels.len()
    }

    /// Lectura bilineal de un nivel: `u` envuelve (la textura rodea la esfera), `v` se sujeta.
    fn bilinear(&self, level: usize, u: f32, v: f32) -> [f32; 2] {
        let l = &self.levels[level];
        let (x, y) = (u * l.w as f32 - 0.5, (v * l.h as f32 - 0.5).clamp(0.0, (l.h - 1) as f32));
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = (x - fx, y - fy);
        let x0 = (fx as i64).rem_euclid(l.w as i64) as usize;
        let x1 = (x0 + 1) % l.w;
        let (y0, y1) = (fy as usize, (fy as usize + 1).min(l.h - 1));
        let t = |xx: usize, yy: usize| l.texels[yy * l.w + xx];
        let mut out = [0.0; 2];
        for (k, o) in out.iter_mut().enumerate() {
            let a = t(x0, y0)[k] + (t(x1, y0)[k] - t(x0, y0)[k]) * tx;
            let b = t(x0, y1)[k] + (t(x1, y1)[k] - t(x0, y1)[k]) * tx;
            *o = a + (b - a) * ty;
        }
        out
    }

    /// Normal en espacio tangente en (`u`, `v`), con el nivel de detalle `lod` (0 = el más fino;
    /// fraccionario = mezcla trilineal de dos niveles).
    pub fn sample(&self, u: f32, v: f32, lod: f32) -> Vec3 {
        let lod = lod.clamp(0.0, (self.levels.len() - 1) as f32);
        let l0 = lod.floor() as usize;
        let t = lod - l0 as f32;
        let mut s = self.bilinear(l0, u, v);
        if t > 1e-3 && l0 + 1 < self.levels.len() {
            let s1 = self.bilinear(l0 + 1, u, v);
            s = [s[0] + (s1[0] - s[0]) * t, s[1] + (s1[1] - s[1]) * t];
        }
        Vec3::new(s[0], s[1], (1.0 - s[0] * s[0] - s[1] * s[1]).max(0.0).sqrt())
    }

    /// Nivel de mipmap que corresponde a que un píxel cubra `texels_per_pixel` texels del nivel más fino.
    pub fn lod_for(texels_per_pixel: f32) -> f32 {
        texels_per_pixel.max(1.0).log2()
    }
}

/// Alturas de un terreno rocoso: ruido fino más cordilleras (el ruido plegado `1 - |2n - 1|` da crestas).
fn rock_height(p: Vec3) -> f32 {
    let bumps = fbm(p * 30.0, 4);
    let ridge = 1.0 - (2.0 * fbm(p * 9.0 + Vec3::new(5.2, 1.3, 8.1), 3) - 1.0).abs();
    0.55 * bumps + 0.45 * ridge * ridge
}

/// Alturas de hielo: surcos finos donde el ruido cruza 0.5 sobre una rugosidad suave.
fn ice_height(p: Vec3) -> f32 {
    let crack = smoothstep(0.0, 0.05, (fbm(p * 7.0 + Vec3::new(2.4, 9.1, 3.3), 4) - 0.5).abs());
    // Grumos aislados (nieve compactada) en celdas de ruido de valor.
    let lumps = hash((p.x * 40.0).floor() as i32, (p.y * 40.0).floor() as i32, (p.z * 40.0).floor() as i32);
    0.75 * crack + 0.15 * fbm(p * 30.0, 3) + 0.10 * lumps
}

/// El mapa horneado de cada tipo. Se hace la primera vez que se pide y se reutiliza.
pub fn map(kind: Kind) -> &'static NormalMap {
    static ROCK: OnceLock<NormalMap> = OnceLock::new();
    static ICE: OnceLock<NormalMap> = OnceLock::new();
    match kind {
        Kind::Rock => ROCK.get_or_init(|| NormalMap::from_height(WIDTH, HEIGHT, 0.045, rock_height)),
        Kind::Ice => ICE.get_or_init(|| NormalMap::from_height(WIDTH, HEIGHT, 0.06, ice_height)),
    }
}

/// Hornea todos los mapas ya, para que no haya un tirón la primera vez que un planeta se ve de cerca.
pub fn warm_up() {
    map(Kind::Rock);
    map(Kind::Ice);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una loma gaussiana centrada en +X.
    fn loma(p: Vec3) -> f32 {
        (-(p.distance(Vec3::new(1.0, 0.0, 0.0)).powi(2)) / 0.05).exp()
    }

    #[test]
    fn la_tangente_apunta_hacia_donde_crece_u() {
        let p = sphere_dir(0.3, 0.6);
        let q = sphere_dir(0.3 + 1e-3, 0.6);
        let t = sphere_tangent(p);
        assert!(t.dot((q - p).normalize()) > 0.999);
        assert!(t.dot(p).abs() < 1e-5, "es tangente a la esfera");
    }

    #[test]
    fn la_base_tbn_es_derecha_y_el_norte_es_b() {
        // T × B = N con B hacia +Y en el ecuador.
        let n = Vec3::new(1.0, 0.0, 0.0);
        let t = sphere_tangent(n);
        let b = n.cross(t);
        assert!(b.dot(Vec3::Y) > 0.999, "{b:?}");
        assert!(t.cross(b).dot(n) > 0.999);
    }

    #[test]
    fn la_normal_inclinada_en_x_se_inclina_hacia_la_tangente() {
        let n = Vec3::new(0.0, 0.0, 1.0);
        let t = Vec3::new(1.0, 0.0, 0.0);
        let ts = Vec3::new(0.6, 0.0, 0.8);
        let p = perturb(n, t, ts);
        assert!(p.distance(Vec3::new(0.6, 0.0, 0.8)) < 1e-5);
        // La tangente se ortogonaliza: una que no es perpendicular da el mismo resultado.
        let sesgada = perturb(n, Vec3::new(1.0, 0.0, 0.7), ts);
        assert!(sesgada.distance(p) < 1e-5);
    }

    #[test]
    fn el_mapa_de_una_loma_inclina_las_normales_alejandose_del_centro() {
        let m = NormalMap::from_height(256, 128, 0.2, loma);
        let (cu, cv) = sphere_uv(Vec3::new(1.0, 0.0, 0.0));
        // En la cima la normal es casi vertical; a los lados mira hacia afuera de la loma.
        let cima = m.sample(cu, cv, 0.0);
        assert!(cima.z > 0.99, "{cima:?}");
        let este = m.sample(cu + 0.03, cv, 0.0);
        let oeste = m.sample(cu - 0.03, cv, 0.0);
        assert!(este.x > 0.05 && oeste.x < -0.05, "este {este:?} oeste {oeste:?}");
        let norte = m.sample(cu, cv + 0.06, 0.0);
        let sur = m.sample(cu, cv - 0.06, 0.0);
        assert!(norte.y > 0.05 && sur.y < -0.05, "norte {norte:?} sur {sur:?}");
    }

    #[test]
    fn los_mipmaps_aplanan_el_relieve() {
        let m = NormalMap::from_height(128, 64, 0.2, rock_height);
        assert!(m.levels() >= 5);
        let inclinacion = |lod: f32| -> f32 {
            (0..200).map(|i| {
                let n = m.sample((i as f32 * 0.0371) % 1.0, 0.1 + (i as f32 * 0.0113) % 0.8, lod);
                1.0 - n.z
            }).sum::<f32>() / 200.0
        };
        let (fino, grueso) = (inclinacion(0.0), inclinacion(4.0));
        assert!(fino > 0.001, "el nivel fino tiene relieve ({fino})");
        assert!(grueso < fino * 0.5, "de lejos el relieve se promedia: {fino} -> {grueso}");
    }

    #[test]
    fn la_textura_envuelve_en_longitud() {
        let m = NormalMap::from_height(128, 64, 0.2, rock_height);
        let (a, b) = (m.sample(0.0005, 0.5, 0.0), m.sample(0.9995, 0.5, 0.0));
        assert!(a.distance(b) < 0.1, "sin costura en u: {a:?} {b:?}");
    }

}
