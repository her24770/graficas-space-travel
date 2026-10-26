//! El escenario: el sol, los planetas y el circuito de la carrera.
//! Es solo datos: para agregar o cambiar un planeta o un punto de control se edita aquí.
use crate::shaders::Material;
use crate::Vec3;
use std::f32::consts::PI;

pub const SUN_RADIUS: f32 = 40.0;
pub const GATE_COUNT: usize = 10;
pub const GATE_RADIUS: f32 = 16.0;

pub struct PlanetDef {
    pub name: &'static str,
    pub radius: f32,
    pub orbit_radius: f32,
    /// Segundos que tarda en dar una vuelta al sol.
    pub orbit_period: f32,
    /// Segundos que tarda en dar una vuelta sobre sí mismo.
    pub spin_period: f32,
    /// Posición inicial en la órbita (radianes).
    pub phase: f32,
    /// Inclinación de la órbita (radianes).
    pub inclination: f32,
    /// Inclinación del eje de giro (radianes).
    pub tilt: f32,
    pub material: Material,
}

pub static PLANETS: [PlanetDef; 5] = [
    PlanetDef {
        name: "FERRO", radius: 7.0, orbit_radius: 110.0, orbit_period: 45.0, spin_period: 20.0,
        phase: 0.5, inclination: 0.05, tilt: 0.1,
        material: Material::Rocky { sea: 0x5D4037, land: 0xA1522B, peak: 0xE0A070, seed: 11.0 },
    },
    PlanetDef {
        name: "AQUA", radius: 10.0, orbit_radius: 190.0, orbit_period: 80.0, spin_period: 30.0,
        phase: 2.2, inclination: -0.08, tilt: 0.41,
        material: Material::Rocky { sea: 0x1E5FB8, land: 0x3E8E41, peak: 0x8D7B68, seed: 4.0 },
    },
    PlanetDef {
        name: "TITAN", radius: 24.0, orbit_radius: 320.0, orbit_period: 130.0, spin_period: 25.0,
        phase: 4.0, inclination: 0.1, tilt: 0.05,
        material: Material::Gas { a: 0xD9A066, b: 0x8C5A3C, seed: 2.0 },
    },
    PlanetDef {
        name: "GLACIA", radius: 11.0, orbit_radius: 430.0, orbit_period: 170.0, spin_period: 40.0,
        phase: 5.4, inclination: -0.12, tilt: 0.3,
        material: Material::Ice { a: 0xBFE3F5, b: 0x6FA8D6, seed: 7.0 },
    },
    PlanetDef {
        name: "NEBULA", radius: 17.0, orbit_radius: 520.0, orbit_period: 220.0, spin_period: 18.0,
        phase: 1.2, inclination: 0.15, tilt: 0.2,
        material: Material::Gas { a: 0x9C6FD6, b: 0x4FC3C8, seed: 9.0 },
    },
];

/// Posición de un planeta en el instante `t` (segundos del reloj del mundo).
/// Es determinista: el mismo reloj da la misma posición en todos los clientes.
pub fn planet_position(p: &PlanetDef, t: f64) -> Vec3 {
    let a = p.phase + (2.0 * std::f64::consts::PI * t / p.orbit_period as f64) as f32;
    Vec3::new(a.cos() * p.orbit_radius, a.sin() * p.orbit_radius * p.inclination.sin(), a.sin() * p.orbit_radius * p.inclination.cos())
}

/// Ángulo de giro de un planeta sobre su eje en el instante `t`.
pub fn planet_spin(p: &PlanetDef, t: f64) -> f32 {
    ((2.0 * std::f64::consts::PI * t / p.spin_period as f64) % (2.0 * std::f64::consts::PI)) as f32
}

#[derive(Clone, Copy, Debug)]
pub struct Gate {
    pub pos: Vec3,
    /// Hacia dónde hay que cruzarlo (apunta al siguiente punto).
    pub dir: Vec3,
    pub radius: f32,
}

/// Circuito cerrado que serpentea entre las órbitas de los planetas.
pub fn course() -> Vec<Gate> {
    let pos: Vec<Vec3> = (0..GATE_COUNT)
        .map(|i| {
            let a = 2.0 * PI * i as f32 / GATE_COUNT as f32;
            let r = 330.0 + 140.0 * (2.0 * a + 0.7).sin();
            Vec3::new(r * a.cos(), 90.0 * (3.0 * a).sin(), r * a.sin())
        })
        .collect();
    (0..GATE_COUNT)
        .map(|i| Gate { pos: pos[i], dir: (pos[(i + 1) % GATE_COUNT] - pos[i]).normalize(), radius: GATE_RADIUS })
        .collect()
}

/// (yaw, pitch) para que una nave mire hacia `dir` (el frente de la nave es -Z).
pub fn orientation(dir: Vec3) -> (f32, f32) {
    ((-dir.x).atan2(-dir.z), dir.y.clamp(-1.0, 1.0).asin())
}

/// Casilla de salida número `slot`: en fila antes del primer punto de control, mirándolo.
pub fn grid_slot(slot: usize) -> (Vec3, f32, f32) {
    let c = course();
    let dir = (c[0].pos - c[GATE_COUNT - 1].pos).normalize();
    let right = dir.cross(Vec3::Y).normalize();
    let (col, row) = ((slot % 4) as f32 - 1.5, (slot / 4) as f32);
    let pos = c[0].pos - dir * (90.0 + row * 25.0) + right * (col * 16.0);
    let (yaw, pitch) = orientation((c[0].pos - pos).normalize());
    (pos, yaw, pitch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ship;

    #[test]
    fn el_circuito_tiene_los_puntos_declarados() {
        assert_eq!(course().len(), GATE_COUNT);
    }

    #[test]
    fn las_orbitas_son_deterministas_y_periodicas() {
        let p = &PLANETS[1];
        assert_eq!(planet_position(p, 12.5), planet_position(p, 12.5));
        let (a, b) = (planet_position(p, 3.0), planet_position(p, 3.0 + p.orbit_period as f64));
        assert!(a.distance(b) < 0.01, "tras un período vuelve al mismo lugar");
    }

    #[test]
    fn el_planeta_se_mantiene_a_su_distancia_del_sol() {
        let p = &PLANETS[2];
        for t in [0.0, 17.0, 90.0] {
            assert!((planet_position(p, t).length() - p.orbit_radius).abs() < 0.01);
        }
    }

    #[test]
    fn las_orbitas_no_se_cruzan_con_el_sol() {
        for p in &PLANETS {
            assert!(p.orbit_radius - p.radius > SUN_RADIUS + 20.0, "{} está pegado al sol", p.name);
        }
    }

    #[test]
    fn los_puntos_de_control_no_chocan_con_el_sol() {
        assert!(course().iter().all(|g| g.pos.length() > SUN_RADIUS + GATE_RADIUS + 20.0));
    }

    #[test]
    fn la_salida_mira_hacia_el_primer_punto() {
        let (pos, yaw, pitch) = grid_slot(0);
        let mut s = Ship::new();
        s.reset(pos, yaw, pitch);
        let hacia = (course()[0].pos - pos).normalize();
        assert!(s.forward().dot(hacia) > 0.95, "la nave debe apuntar al primer punto");
    }

    #[test]
    fn las_casillas_de_salida_no_se_superponen() {
        for a in 0..8 {
            for b in (a + 1)..8 {
                assert!(grid_slot(a).0.distance(grid_slot(b).0) > 10.0, "casillas {a} y {b}");
            }
        }
    }

    #[test]
    fn orientation_es_la_inversa_de_forward() {
        for d in [Vec3::new(1.0, 0.2, -0.5), Vec3::new(-0.3, -0.6, 0.7)] {
            let d = d.normalize();
            let (yaw, pitch) = orientation(d);
            let mut s = Ship::new();
            s.reset(Vec3::ZERO, yaw, pitch);
            assert!(s.forward().distance(d) < 1e-4);
        }
    }
}
