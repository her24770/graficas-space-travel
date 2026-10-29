//! Naves de otros jugadores: llegan ~20 veces por segundo y se suavizan para que no den saltos.
use crate::Vec3;
use std::f32::consts::PI;
pub const NAME_LEN: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteShip {
    pub id: u32,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub model: u8,
    /// Puntos de control cruzados.
    pub progress: u8,
    /// Milisegundos de carrera al terminar; 0 = aún no termina.
    pub finish_ms: u32,
    pub name: [u8; NAME_LEN],
}

const SUAVIZADO: f32 = 12.0;

/// Lleva un ángulo a [-π, π) para girar siempre por el lado corto.
pub fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

pub(crate) struct RemotePlayer {
    pub shown: RemoteShip,
    pub target: RemoteShip,
}

impl RemotePlayer {
    pub fn new(s: RemoteShip) -> Self {
        Self { shown: s, target: s }
    }

    pub fn update(&mut self, dt: f32) {
        let k = 1.0 - (-SUAVIZADO * dt).exp();
        let (s, t) = (&mut self.shown, &self.target);
        s.position = s.position + (t.position - s.position) * k;
        s.yaw += wrap_angle(t.yaw - s.yaw) * k;
        s.pitch += (t.pitch - s.pitch) * k;
        s.model = t.model;
        s.progress = t.progress;
        s.finish_ms = t.finish_ms;
        s.name = t.name;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nave(x: f32, yaw: f32) -> RemoteShip {
        RemoteShip { id: 1, position: Vec3::new(x, 0.0, 0.0), yaw, pitch: 0.0, model: 0, progress: 0, finish_ms: 0, name: [0; NAME_LEN] }
    }

    #[test]
    fn se_acerca_al_objetivo_sin_pasarse() {
        let mut p = RemotePlayer::new(nave(0.0, 0.0));
        p.target = nave(10.0, 0.0);
        p.update(0.05);
        assert!(p.shown.position.x > 0.0 && p.shown.position.x < 10.0);
        for _ in 0..200 {
            p.update(0.05);
        }
        assert!((p.shown.position.x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn gira_por_el_lado_corto() {
        let mut p = RemotePlayer::new(nave(0.0, 3.0));
        p.target = nave(0.0, -3.0); // 6 rad hacia atrás, pero solo ~0.28 rad por el otro lado
        p.update(0.05);
        assert!(p.shown.yaw > 3.0, "debe seguir subiendo, no devolverse");
    }

    #[test]
    fn wrap_angle_queda_en_rango() {
        for a in [-10.0, -3.2, 0.0, 3.2, 10.0] {
            assert!((-PI..PI).contains(&wrap_angle(a)));
        }
    }
}
