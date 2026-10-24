//! Cámara en tercera persona que sigue a la nave con suavizado.
use crate::{Mat4, Ship, Vec3};

const OFFSET: Vec3 = Vec3::new(0.0, 1.8, 6.0); // detrás y arriba, en espacio de la nave
const MIRA: Vec3 = Vec3::new(0.0, 0.5, -8.0); // punto al frente que mira la cámara
const SEGUIMIENTO: f32 = 8.0;

pub struct ChaseCamera {
    pub eye: Vec3,
    pub target: Vec3,
}

impl ChaseCamera {
    pub fn new(ship: &Ship) -> Self {
        let (eye, target) = Self::deseado(ship);
        Self { eye, target }
    }

    fn deseado(ship: &Ship) -> (Vec3, Vec3) {
        let r = ship.rotation();
        (ship.position + r.transform_dir(OFFSET), ship.position + r.transform_dir(MIRA))
    }

    pub fn update(&mut self, ship: &Ship, dt: f32) {
        let (eye, target) = Self::deseado(ship);
        let k = 1.0 - (-SEGUIMIENTO * dt).exp();
        self.eye = self.eye + (eye - self.eye) * k;
        self.target = self.target + (target - self.target) * k;
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.eye, self.target, Vec3::Y)
    }
}
