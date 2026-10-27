//! Modo órbita: la nave da vueltas alrededor de un planeta (o del sol) y se controla de forma
//! local a ese cuerpo: altura, velocidad y plano de la órbita, en lugar de volar libre.
use crate::Vec3;
use std::f32::consts::TAU;

/// Metros por segundo que sube o baja la órbita.
pub const ALTURA_VEL: f32 = 15.0;
/// Aceleración al apurar o frenar la órbita (u/s²).
pub const CAMBIO_VEL: f32 = 15.0;
/// Radianes por segundo que gira el plano de la órbita.
pub const GIRO_PLANO: f32 = 0.8;
pub const VEL_MIN: f32 = 5.0;
pub const VEL_MAX: f32 = 90.0;

/// Velocidad de una órbita circular con la gravedad del juego (g·R²/d²): v = √(g·R²/r).
pub fn circular_speed(surface_gravity: f32, body_radius: f32, r: f32) -> f32 {
    (surface_gravity * body_radius * body_radius / r.max(1e-3)).sqrt()
}

#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    /// Índice del cuerpo: 0 = sol, 1.. = planetas.
    pub body: usize,
    pub radius: f32,
    /// Base del plano de la órbita: `u` apunta a la nave en el ángulo 0, `v` es la dirección de avance.
    u: Vec3,
    v: Vec3,
    angle: f32,
    pub speed: f32,
}

impl Orbit {
    /// Entra en órbita circular a la distancia actual, en el plano en que se movía la nave.
    pub fn enter(body: usize, offset: Vec3, relative_velocity: Vec3, forward: Vec3, speed: f32) -> Orbit {
        let u = offset.normalize();
        let tangente = |d: Vec3| (d - u * d.dot(u)).normalize();
        let mut v = tangente(if relative_velocity.length() > 1.0 { relative_velocity } else { forward });
        for alternativa in [Vec3::Y, Vec3::new(1.0, 0.0, 0.0)] {
            if v.length() > 0.5 {
                break;
            }
            v = u.cross(alternativa).normalize();
        }
        Orbit { body, radius: offset.length(), u, v, angle: 0.0, speed: speed.clamp(VEL_MIN, VEL_MAX) }
    }

    /// Dirección del centro del cuerpo hacia la nave.
    pub fn radial(&self) -> Vec3 {
        self.u * self.angle.cos() + self.v * self.angle.sin()
    }

    /// Dirección en la que avanza la nave.
    pub fn tangent(&self) -> Vec3 {
        self.u * -self.angle.sin() + self.v * self.angle.cos()
    }

    pub fn position(&self, center: Vec3) -> Vec3 {
        center + self.radial() * self.radius
    }

    /// `climb`: +1 sube, -1 baja. `throttle`: +1 acelera, -1 frena. `turn`: gira el plano.
    pub fn update(&mut self, dt: f32, climb: f32, throttle: f32, turn: f32, min_radius: f32, max_radius: f32) {
        self.radius = (self.radius + climb * ALTURA_VEL * dt).clamp(min_radius, max_radius);
        self.speed = (self.speed + throttle * CAMBIO_VEL * dt).clamp(VEL_MIN, VEL_MAX);
        if turn != 0.0 {
            // Girar el plano = girar la dirección de avance alrededor de la vertical local.
            let (r, t) = (self.radial(), self.tangent());
            let phi = turn * GIRO_PLANO * dt;
            self.u = r;
            self.v = (t * phi.cos() + r.cross(t) * phi.sin()).normalize();
            self.angle = 0.0;
        }
        self.angle = (self.angle + self.speed / self.radius * dt) % TAU;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orbita() -> Orbit {
        Orbit::enter(1, Vec3::new(30.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, -1.0), 10.0)
    }

    #[test]
    fn la_orbita_mantiene_la_distancia_y_avanza() {
        let mut o = orbita();
        let inicio = o.position(Vec3::ZERO);
        for _ in 0..60 {
            o.update(1.0 / 60.0, 0.0, 0.0, 0.0, 10.0, 100.0);
            assert!((o.position(Vec3::ZERO).length() - 30.0).abs() < 1e-3);
        }
        assert!(o.position(Vec3::ZERO).distance(inicio) > 5.0);
    }

    #[test]
    fn la_tangente_es_perpendicular_a_la_radial() {
        let mut o = orbita();
        for _ in 0..50 {
            o.update(0.05, 0.0, 0.0, 1.0, 10.0, 100.0);
            assert!(o.radial().dot(o.tangent()).abs() < 1e-4);
        }
    }

    #[test]
    fn subir_y_bajar_respeta_los_limites() {
        let mut o = orbita();
        for _ in 0..1000 {
            o.update(0.05, -1.0, 0.0, 0.0, 12.0, 100.0);
        }
        assert!((o.radius - 12.0).abs() < 1e-4, "no se puede bajar a la superficie");
        o.update(1.0, 1.0, 0.0, 0.0, 12.0, 100.0);
        assert!((o.radius - 12.0 - ALTURA_VEL).abs() < 1e-3);
    }

    #[test]
    fn acelerar_tiene_tope() {
        let mut o = orbita();
        for _ in 0..1000 {
            o.update(0.05, 0.0, 1.0, 0.0, 10.0, 100.0);
        }
        assert_eq!(o.speed, VEL_MAX);
    }

    #[test]
    fn girar_cambia_el_plano() {
        let mut o = orbita();
        let antes = o.tangent();
        for _ in 0..20 {
            o.update(0.05, 0.0, 0.0, 1.0, 10.0, 100.0);
        }
        assert!(o.tangent().dot(antes) < 0.95);
    }

    #[test]
    fn la_velocidad_circular_baja_con_la_distancia() {
        assert!(circular_speed(10.0, 10.0, 20.0) > circular_speed(10.0, 10.0, 40.0));
        assert!((circular_speed(10.0, 10.0, 10.0) - 10.0).abs() < 1e-4, "en la superficie: √(g·R)");
    }

    #[test]
    fn entrar_quieto_usa_el_frente_de_la_nave() {
        let o = Orbit::enter(0, Vec3::new(0.0, 0.0, 50.0), Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 10.0);
        assert!(o.tangent().dot(Vec3::new(1.0, 0.0, 0.0)) > 0.99);
    }

    #[test]
    fn entrar_mirando_al_planeta_igual_encuentra_un_plano() {
        let o = Orbit::enter(0, Vec3::new(0.0, 0.0, 50.0), Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 10.0);
        assert!(o.tangent().length() > 0.99 && o.radial().dot(o.tangent()).abs() < 1e-4);
    }
}
