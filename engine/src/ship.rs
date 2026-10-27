//! La nave: aceleración, frenado y giro progresivos, todo en función de `dt`.
use crate::{input::Input, Mat4, Vec3};

/// Aceleración del motor (u/s²). Ninguna gravedad del escenario puede superarla.
pub const EMPUJE: f32 = 40.0;
const EMPUJE_REVERSA: f32 = 20.0;
const ARRASTRE: f32 = 0.5; // 1/s, frenado natural
const ARRASTRE_FRENO: f32 = 3.0;
const VEL_MAX: f32 = 90.0;
const GIRO_YAW: f32 = 1.6; // rad/s máximos
const GIRO_PITCH: f32 = 1.2;
const SUAVIZADO_GIRO: f32 = 6.0; // más alto = responde más rápido
const PITCH_MAX: f32 = 1.4;

pub struct Ship {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    yaw_rate: f32,
    pitch_rate: f32,
}

impl Ship {
    pub fn new() -> Self {
        Self { position: Vec3::ZERO, velocity: Vec3::ZERO, yaw: 0.0, pitch: 0.0, yaw_rate: 0.0, pitch_rate: 0.0 }
    }

    /// Coloca la nave en reposo (salida de la carrera).
    pub fn reset(&mut self, position: Vec3, yaw: f32, pitch: f32) {
        *self = Ship { position, yaw, pitch, ..Ship::new() };
    }

    pub fn rotation(&self) -> Mat4 {
        Mat4::rotation_y(self.yaw) * Mat4::rotation_x(self.pitch)
    }

    /// El frente de la nave es -Z en su espacio local.
    pub fn forward(&self) -> Vec3 {
        self.rotation().transform_dir(Vec3::new(0.0, 0.0, -1.0))
    }

    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }

    pub fn update(&mut self, input: &Input, dt: f32) {
        let eje = |mas: bool, menos: bool| mas as i32 as f32 - menos as i32 as f32;
        let suavizado = 1.0 - (-SUAVIZADO_GIRO * dt).exp();

        let yaw_obj = eje(input.yaw_left, input.yaw_right) * GIRO_YAW;
        let pitch_obj = eje(input.pitch_up, input.pitch_down) * GIRO_PITCH;
        self.yaw_rate += (yaw_obj - self.yaw_rate) * suavizado;
        self.pitch_rate += (pitch_obj - self.pitch_rate) * suavizado;
        self.yaw += self.yaw_rate * dt;
        self.pitch = (self.pitch + self.pitch_rate * dt).clamp(-PITCH_MAX, PITCH_MAX);

        let empuje = input.forward as i32 as f32 * EMPUJE - input.backward as i32 as f32 * EMPUJE_REVERSA;
        self.velocity += self.forward() * (empuje * dt);
        let arrastre = if input.brake { ARRASTRE_FRENO } else { ARRASTRE };
        self.velocity = self.velocity * (-arrastre * dt).exp();
        if self.speed() > VEL_MAX {
            self.velocity = self.velocity.normalize() * VEL_MAX;
        }
        self.position += self.velocity * dt;
    }
}

impl Default for Ship {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simular(segundos: f32, dt: f32, input: Input) -> Ship {
        let mut s = Ship::new();
        for _ in 0..(segundos / dt).round() as usize {
            s.update(&input, dt);
        }
        s
    }

    #[test]
    fn avanzar_acelera_hacia_el_frente() {
        let s = simular(1.0, 1.0 / 60.0, Input { forward: true, ..Default::default() });
        assert!(s.speed() > 5.0);
        assert!(s.position.z < -1.0, "el frente es -Z");
    }

    #[test]
    fn el_resultado_no_depende_de_los_cuadros_por_segundo() {
        let i = Input { forward: true, yaw_left: true, ..Default::default() };
        let a = simular(2.0, 1.0 / 30.0, i);
        let b = simular(2.0, 1.0 / 144.0, i);
        assert!((a.speed() - b.speed()).abs() / b.speed() < 0.05);
        assert!((a.yaw - b.yaw).abs() < 0.05);
    }

    #[test]
    fn el_giro_es_progresivo() {
        let mut s = Ship::new();
        s.update(&Input { yaw_left: true, ..Default::default() }, 1.0 / 60.0);
        assert!(s.yaw > 0.0 && s.yaw < 0.01);
    }

    #[test]
    fn la_velocidad_tiene_tope() {
        let s = simular(60.0, 1.0 / 60.0, Input { forward: true, ..Default::default() });
        assert!(s.speed() <= VEL_MAX + 1e-3);
    }

    #[test]
    fn el_freno_detiene_mas_rapido_que_soltar() {
        let mut libre = simular(3.0, 1.0 / 60.0, Input { forward: true, ..Default::default() });
        let mut freno = simular(3.0, 1.0 / 60.0, Input { forward: true, ..Default::default() });
        for _ in 0..60 {
            libre.update(&Input::default(), 1.0 / 60.0);
            freno.update(&Input { brake: true, ..Default::default() }, 1.0 / 60.0);
        }
        assert!(freno.speed() < libre.speed());
    }
}
