//! Objetos sobre la superficie de los planetas (árboles, rocas, cristales...), colocados de forma
//! procedural: la semilla decide dónde se intenta poner cada uno y el ruido decide la densidad
//! (se agrupan en bosques o campos) y la zona (tierra, montaña). Misma semilla = mismo resultado.
use crate::rng::Rng;
use crate::scenario::{surface_radius, PlanetDef, Zone};
use crate::shaders::fbm;
use crate::{terrain, Mat4, Vec3};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct PropInstance {
    /// Índice dentro de `PlanetDef::props`.
    pub kind: usize,
    /// Transformación relativa al centro del planeta en su espacio local: gira junto con él.
    pub local: Mat4,
    /// Pie del objeto, en el espacio local del planeta.
    pub base: Vec3,
    pub size: f32,
}

/// Rotación que lleva el "arriba" del modelo (Y) a la normal de la superficie `up`,
/// con un giro `yaw` alrededor de ella para que no todos miren igual.
pub fn surface_basis(up: Vec3, yaw: f32) -> Mat4 {
    let auxiliar = if up.y.abs() < 0.9 { Vec3::Y } else { Vec3::new(1.0, 0.0, 0.0) };
    let x = auxiliar.cross(up).normalize();
    let z = x.cross(up);
    Mat4::from_basis(x, up, z) * Mat4::rotation_y(yaw)
}

fn zona_valida(planet: &PlanetDef, zone: Zone, dir: Vec3) -> bool {
    let m = &planet.material;
    match zone {
        Zone::Anywhere => !terrain::is_sea(m, dir),
        Zone::Land => !terrain::is_sea(m, dir) && !terrain::is_high(m, dir),
        Zone::High => terrain::is_high(m, dir),
    }
}

/// Coloca los objetos de un planeta. `planet_index` separa las semillas de cada planeta.
pub fn place(planet_index: usize, planet: &PlanetDef, seed: u64) -> Vec<PropInstance> {
    let mut out = Vec::new();
    for (kind, def) in planet.props.iter().enumerate() {
        let mut rng = Rng::new(seed ^ ((planet_index as u64 + 1) << 32) ^ ((kind as u64 + 1) << 16));
        // El ruido de densidad también cambia con la semilla.
        let desfase = Vec3::new(rng.range(0.0, 100.0), rng.range(0.0, 100.0), rng.range(0.0, 100.0));
        let (mut puestos, mut intentos) = (0, 0);
        while puestos < def.count && intentos < def.count * 60 {
            intentos += 1;
            let (z, a) = (rng.range(-1.0, 1.0), rng.range(0.0, 2.0 * PI));
            let r = (1.0 - z * z).sqrt();
            let dir = Vec3::new(r * a.cos(), z, r * a.sin());
            let densidad = fbm(dir * 4.0 + desfase, 3);
            if !zona_valida(planet, def.zone, dir) || rng.f32() > densidad * 1.6 - 0.3 {
                continue;
            }
            let size = def.size * rng.range(0.8, 1.25);
            let base = dir * surface_radius(planet, dir);
            let local = Mat4::translation(base) * surface_basis(dir, rng.range(0.0, 2.0 * PI)) * Mat4::scale(Vec3::new(size, size, size));
            out.push(PropInstance { kind, local, base, size });
            puestos += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{planet_rotation, PLANETS};

    fn con_objetos() -> (usize, &'static PlanetDef) {
        PLANETS.iter().enumerate().find(|(_, p)| !p.props.is_empty()).expect("algún planeta debe tener objetos")
    }

    #[test]
    fn misma_semilla_mismos_objetos_y_otra_semilla_otros() {
        let (i, p) = con_objetos();
        let (a, b, c) = (place(i, p, 7), place(i, p, 7), place(i, p, 8));
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| x.base == y.base));
        assert!(a.iter().zip(&c).any(|(x, y)| x.base != y.base));
    }

    #[test]
    fn se_colocan_casi_todos_los_pedidos() {
        for (i, p) in PLANETS.iter().enumerate() {
            let puestos = place(i, p, 1);
            for (k, def) in p.props.iter().enumerate() {
                let n = puestos.iter().filter(|x| x.kind == k).count() as u32;
                assert!(n <= def.count && n * 2 >= def.count, "{} / {}: {n} de {}", p.name, def.model, def.count);
            }
        }
    }

    #[test]
    fn cada_objeto_esta_apoyado_sobre_la_superficie_con_relieve() {
        let (i, p) = con_objetos();
        for x in place(i, p, 3) {
            assert!((x.base.length() - surface_radius(p, x.base)).abs() < 1e-3);
            assert!(x.local.transform_point3(Vec3::ZERO).distance(x.base) < 1e-3, "el pie del modelo va en la base");
        }
    }

    #[test]
    fn cada_objeto_queda_parado_perpendicular_al_suelo() {
        let (i, p) = con_objetos();
        for x in place(i, p, 3) {
            let arriba = x.local.transform_dir(Vec3::Y).normalize();
            assert!(arriba.dot(x.base.normalize()) > 0.999);
        }
    }

    #[test]
    fn los_de_tierra_no_quedan_en_el_mar() {
        for (i, p) in PLANETS.iter().enumerate() {
            for x in place(i, p, 5) {
                if p.props[x.kind].zone != Zone::High {
                    assert!(!terrain::is_sea(&p.material, x.base), "{} en el mar", p.props[x.kind].model);
                }
            }
        }
    }

    #[test]
    fn siguen_parados_mientras_el_planeta_gira() {
        let (i, p) = con_objetos();
        let x = place(i, p, 3)[0];
        for t in [0.0, 13.0, 77.0] {
            let rot = planet_rotation(p, t);
            let mundo = rot * x.local;
            let pie = mundo.transform_point3(Vec3::ZERO);
            let arriba = mundo.transform_dir(Vec3::Y).normalize();
            assert!((pie.length() - x.base.length()).abs() < 1e-3);
            assert!(arriba.dot(pie.normalize()) > 0.999);
        }
    }

    #[test]
    fn surface_basis_es_una_rotacion_valida_incluso_en_los_polos() {
        for up in [Vec3::Y, Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.6, 0.0, 0.8)] {
            let m = surface_basis(up, 1.0);
            assert!((m.transform_dir(Vec3::Y) - up).length() < 1e-4);
            let (x, z) = (m.transform_dir(Vec3::new(1.0, 0.0, 0.0)), m.transform_dir(Vec3::new(0.0, 0.0, 1.0)));
            assert!((x.cross(up) - z).length() < 1e-4, "base de mano derecha");
        }
    }
}
