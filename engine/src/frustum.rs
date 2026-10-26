//! Frustum culling: descarta los objetos que quedan fuera de la vista antes de rasterizarlos.
use crate::{Mat4, Vec3};

pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    /// Extrae los 6 planos (izq, der, abajo, arriba, cerca, lejos) de la matriz vista-proyección.
    pub fn from_view_proj(vp: &Mat4) -> Self {
        let m = &vp.m;
        let combo = |sign: f32, row: usize| {
            let mut p = [0.0; 4];
            for (c, v) in p.iter_mut().enumerate() {
                *v = m[3][c] + sign * m[row][c];
            }
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            p.map(|v| v / len)
        };
        Self { planes: [combo(1.0, 0), combo(-1.0, 0), combo(1.0, 1), combo(-1.0, 1), combo(1.0, 2), combo(-1.0, 2)] }
    }

    /// ¿Se ve (aunque sea en parte) una esfera de centro `c` y radio `r`?
    pub fn sphere_visible(&self, c: Vec3, r: f32) -> bool {
        self.planes.iter().all(|p| p[0] * c.x + p[1] * c.y + p[2] * c.z + p[3] >= -r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frustum() -> Frustum {
        let vp = Mat4::perspective(1.0, 1.0, 0.1, 100.0) * Mat4::look_at(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        Frustum::from_view_proj(&vp)
    }

    #[test]
    fn ve_lo_que_esta_al_frente() {
        assert!(frustum().sphere_visible(Vec3::new(0.0, 0.0, -20.0), 1.0));
    }

    #[test]
    fn no_ve_lo_que_esta_detras() {
        assert!(!frustum().sphere_visible(Vec3::new(0.0, 0.0, 20.0), 1.0));
    }

    #[test]
    fn no_ve_lo_que_esta_muy_a_un_lado_ni_muy_lejos() {
        assert!(!frustum().sphere_visible(Vec3::new(80.0, 0.0, -20.0), 1.0));
        assert!(!frustum().sphere_visible(Vec3::new(0.0, 0.0, -500.0), 1.0));
    }

    #[test]
    fn una_esfera_grande_que_toca_el_borde_si_se_ve() {
        assert!(frustum().sphere_visible(Vec3::new(30.0, 0.0, -20.0), 20.0));
    }
}
