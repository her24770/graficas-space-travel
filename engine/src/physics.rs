//! Gravedad y colisiones de la nave contra el sol y los planetas.
//! Solo es el movimiento: los efectos visuales o de sonido de un choque se conectan aparte
//! leyendo los eventos de colisión del juego.
use crate::Vec3;

/// Qué tanto rebota la nave al chocar (0 = se queda pegada, 1 = rebota igual de rápido).
pub const REBOTE: f32 = 0.35;
/// Roce con la superficie (1/s): mientras toca, la velocidad paralela al suelo (relativa al
/// cuerpo) se frena así cada segundo. Depende de `dt`, no de los cuadros por segundo.
pub const ROCE: f32 = 3.0;
/// Margen de contacto: apoyada a menos de esta distancia de la superficie se considera tocando.
pub const CONTACTO: f32 = 0.05;

/// Un cuerpo que atrae y contra el que se choca.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub center: Vec3,
    pub radius: f32,
    /// Aceleración de la gravedad en su superficie (u/s²).
    pub surface_gravity: f32,
    pub velocity: Vec3,
}

/// Aceleración de gravedad en `p`: cada cuerpo atrae hacia su centro con g·R²/d²,
/// es decir, `surface_gravity` en su superficie y cuatro veces menos al doble de distancia.
pub fn gravity_at(p: Vec3, bodies: &[Body]) -> Vec3 {
    bodies.iter().fold(Vec3::ZERO, |acc, b| {
        let d = b.center - p;
        let dist = d.length().max(b.radius); // dentro del cuerpo no crece sin límite
        acc + d.normalize() * (b.surface_gravity * b.radius * b.radius / (dist * dist))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Dirección hacia afuera de la superficie en el punto del choque.
    pub normal: Vec3,
    /// Velocidad con la que se acercaba a la superficie (0 si ya se alejaba).
    pub impact_speed: f32,
}

/// Choque de una esfera (la nave) contra la superficie de un cuerpo.
/// `surface` es el radio de la superficie en la dirección de la nave (con relieve).
/// Si se tocan, saca la nave a la superficie, rebota la velocidad que iba hacia el suelo y frena
/// el deslizamiento. Todo es relativo al cuerpo, así que un planeta en movimiento la arrastra.
#[allow(clippy::too_many_arguments)]
pub fn collide(
    pos: &mut Vec3,
    vel: &mut Vec3,
    ship_radius: f32,
    body_center: Vec3,
    body_velocity: Vec3,
    surface: f32,
    dt: f32,
) -> Option<Contact> {
    let d = *pos - body_center;
    let dist = d.length();
    let limite = surface + ship_radius;
    if dist >= limite + CONTACTO {
        return None;
    }
    let normal = if dist > 1e-6 { d * (1.0 / dist) } else { Vec3::Y };
    if dist < limite {
        *pos = body_center + normal * limite;
    }

    let relativa = *vel - body_velocity;
    let vn = relativa.dot(normal);
    let tangencial = (relativa - normal * vn) * (-ROCE * dt).exp();
    let normal_nueva = if vn < 0.0 { -vn * REBOTE } else { vn };
    *vel = body_velocity + normal * normal_nueva + tangencial;
    Some(Contact { normal, impact_speed: (-vn).max(0.0) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sol() -> Body {
        Body { center: Vec3::ZERO, radius: 10.0, surface_gravity: 20.0, velocity: Vec3::ZERO }
    }

    #[test]
    fn la_gravedad_apunta_al_centro_y_vale_g_en_la_superficie() {
        let a = gravity_at(Vec3::new(10.0, 0.0, 0.0), &[sol()]);
        assert!((a - Vec3::new(-20.0, 0.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn al_doble_de_distancia_la_gravedad_es_la_cuarta_parte() {
        let cerca = gravity_at(Vec3::new(20.0, 0.0, 0.0), &[sol()]).length();
        let lejos = gravity_at(Vec3::new(40.0, 0.0, 0.0), &[sol()]).length();
        assert!((cerca / lejos - 4.0).abs() < 1e-3);
    }

    #[test]
    fn varios_cuerpos_se_suman_y_en_el_medio_se_cancelan() {
        let b = Body { center: Vec3::new(100.0, 0.0, 0.0), ..sol() };
        assert!(gravity_at(Vec3::new(50.0, 0.0, 0.0), &[sol(), b]).length() < 1e-4);
    }

    #[test]
    fn en_el_centro_no_hay_division_por_cero() {
        assert!(gravity_at(Vec3::ZERO, &[sol()]).length().is_finite());
    }

    #[test]
    fn la_caida_no_depende_de_los_cuadros_por_segundo() {
        let caer = |dt: f32| {
            let (mut p, mut v) = (Vec3::new(30.0, 0.0, 0.0), Vec3::ZERO);
            for _ in 0..(1.0 / dt).round() as usize {
                v += gravity_at(p, &[sol()]) * dt;
                p += v * dt;
            }
            p.x
        };
        assert!((caer(1.0 / 30.0) - caer(1.0 / 240.0)).abs() < 0.3);
    }

    #[test]
    fn sin_tocar_no_hay_choque() {
        let (mut p, mut v) = (Vec3::new(20.0, 0.0, 0.0), Vec3::new(-5.0, 0.0, 0.0));
        assert!(collide(&mut p, &mut v, 1.0, Vec3::ZERO, Vec3::ZERO, 10.0, 0.016).is_none());
        assert_eq!(p, Vec3::new(20.0, 0.0, 0.0));
    }

    #[test]
    fn al_chocar_sale_a_la_superficie_y_rebota() {
        let (mut p, mut v) = (Vec3::new(10.5, 0.0, 0.0), Vec3::new(-20.0, 3.0, 0.0));
        let c = collide(&mut p, &mut v, 1.0, Vec3::ZERO, Vec3::ZERO, 10.0, 0.1).unwrap();
        assert!((p.length() - 11.0).abs() < 1e-4, "queda apoyada sobre la superficie");
        assert!((v.x - 20.0 * REBOTE).abs() < 1e-4, "rebota más lento de lo que llegó");
        assert!((c.impact_speed - 20.0).abs() < 1e-4);
        assert!(v.y > 0.0 && v.y < 3.0, "el roce frena el deslizamiento");
    }

    #[test]
    fn un_planeta_en_movimiento_arrastra_a_la_nave_apoyada() {
        let (mut p, mut v) = (Vec3::new(10.5, 0.0, 0.0), Vec3::ZERO);
        for _ in 0..60 {
            assert!(collide(&mut p, &mut v, 1.0, Vec3::ZERO, Vec3::new(0.0, 0.0, 15.0), 10.0, 1.0 / 60.0).is_some());
        }
        assert!(v.z > 10.0, "tras un segundo apoyada se mueve casi como el planeta");
    }

    #[test]
    fn el_roce_no_depende_de_los_cuadros_por_segundo() {
        let deslizar = |dt: f32| {
            let (mut p, mut v) = (Vec3::new(11.0, 0.0, 0.0), Vec3::new(0.0, 10.0, 0.0));
            for _ in 0..(1.0 / dt).round() as usize {
                collide(&mut p, &mut v, 1.0, Vec3::ZERO, Vec3::ZERO, 10.0, dt);
            }
            v.y
        };
        assert!((deslizar(1.0 / 30.0) - deslizar(1.0 / 144.0)).abs() < 0.01);
    }

    #[test]
    fn apoyada_sin_moverse_no_cuenta_como_impacto() {
        let (mut p, mut v) = (Vec3::new(11.0, 0.0, 0.0), Vec3::ZERO);
        let c = collide(&mut p, &mut v, 1.0, Vec3::ZERO, Vec3::ZERO, 10.0, 0.016).unwrap();
        assert_eq!(c.impact_speed, 0.0);
    }
}
