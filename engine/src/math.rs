//! Álgebra propia: vectores y matrices 4x4. Sin librerías.
//! Convención: mano derecha, el modelo mira hacia -Z, Y arriba.
//! `Mat4` es fila-mayor y los vectores son columnas (v' = M * v).
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn distance(self, o: Vec3) -> f32 {
        (self - o).length()
    }

    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }

    /// Devuelve el vector unitario; si es nulo, devuelve cero.
    pub fn normalize(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { Vec3::ZERO }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const ZERO: Vec4 = Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 };

    pub fn lerp(a: Vec4, b: Vec4, t: f32) -> Vec4 {
        Vec4 {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            z: a.z + (b.z - a.z) * t,
            w: a.w + (b.w - a.w) * t,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translation(t: Vec3) -> Mat4 {
        let mut r = Mat4::IDENTITY;
        r.m[0][3] = t.x;
        r.m[1][3] = t.y;
        r.m[2][3] = t.z;
        r
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut r = Mat4::IDENTITY;
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }

    pub fn rotation_x(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::IDENTITY;
        r.m[1][1] = c;
        r.m[1][2] = -s;
        r.m[2][1] = s;
        r.m[2][2] = c;
        r
    }

    pub fn rotation_y(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::IDENTITY;
        r.m[0][0] = c;
        r.m[0][2] = s;
        r.m[2][0] = -s;
        r.m[2][2] = c;
        r
    }

    pub fn rotation_z(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut r = Mat4::IDENTITY;
        r.m[0][0] = c;
        r.m[0][1] = -s;
        r.m[1][0] = s;
        r.m[1][1] = c;
        r
    }

    /// Proyección en perspectiva (profundidad NDC de -1 a 1).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let mut r = Mat4 { m: [[0.0; 4]; 4] };
        r.m[0][0] = f / aspect;
        r.m[1][1] = f;
        r.m[2][2] = (far + near) / (near - far);
        r.m[2][3] = 2.0 * far * near / (near - far);
        r.m[3][2] = -1.0;
        r
    }

    /// Matriz de vista: coloca la cámara en `eye` mirando a `target`.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Mat4 {
            m: [
                [s.x, s.y, s.z, -s.dot(eye)],
                [u.x, u.y, u.z, -u.dot(eye)],
                [-f.x, -f.y, -f.z, f.dot(eye)],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn without_translation(mut self) -> Mat4 {
        self.m[0][3] = 0.0;
        self.m[1][3] = 0.0;
        self.m[2][3] = 0.0;
        self
    }

    /// Transforma un punto (w = 1) y devuelve coordenadas homogéneas.
    pub fn transform_point(&self, p: Vec3) -> Vec4 {
        let m = &self.m;
        Vec4 {
            x: m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3],
            y: m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3],
            z: m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3],
            w: m[3][0] * p.x + m[3][1] * p.y + m[3][2] * p.z + m[3][3],
        }
    }

    /// Transforma un punto con una matriz afín (sin división por w).
    pub fn transform_point3(&self, p: Vec3) -> Vec3 {
        let v = self.transform_point(p);
        Vec3::new(v.x, v.y, v.z)
    }

    /// Transforma una dirección (w = 0): ignora la traslación.
    pub fn transform_dir(&self, d: Vec3) -> Vec3 {
        let m = &self.m;
        Vec3::new(
            m[0][0] * d.x + m[0][1] * d.y + m[0][2] * d.z,
            m[1][0] * d.x + m[1][1] * d.y + m[1][2] * d.z,
            m[2][0] * d.x + m[2][1] * d.y + m[2][2] * d.z,
        )
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut m = [[0.0; 4]; 4];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| self.m[r][k] * o.m[k][c]).sum();
            }
        }
        Mat4 { m }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cerca(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn identidad_no_cambia_el_punto() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert!(cerca(Mat4::IDENTITY.transform_point3(p), p));
    }

    #[test]
    fn rotacion_y_gira_el_frente() {
        // Yaw de 90°: el frente (-Z) pasa a apuntar a -X.
        let f = Mat4::rotation_y(std::f32::consts::FRAC_PI_2).transform_dir(Vec3::new(0.0, 0.0, -1.0));
        assert!(cerca(f, Vec3::new(-1.0, 0.0, 0.0)));
    }

    #[test]
    fn rotacion_x_positiva_sube_la_nariz() {
        let f = Mat4::rotation_x(std::f32::consts::FRAC_PI_2).transform_dir(Vec3::new(0.0, 0.0, -1.0));
        assert!(cerca(f, Vec3::new(0.0, 1.0, 0.0)));
    }

    #[test]
    fn composicion_aplica_de_derecha_a_izquierda() {
        let m = Mat4::translation(Vec3::new(10.0, 0.0, 0.0)) * Mat4::scale(Vec3::new(2.0, 2.0, 2.0));
        assert!(cerca(m.transform_point3(Vec3::new(1.0, 0.0, 0.0)), Vec3::new(12.0, 0.0, 0.0)));
    }

    #[test]
    fn look_at_deja_al_objetivo_frente_a_la_camara() {
        let v = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        assert!(cerca(v.transform_point3(Vec3::ZERO), Vec3::new(0.0, 0.0, -5.0)));
    }

    #[test]
    fn perspectiva_mapea_near_y_far() {
        let p = Mat4::perspective(1.0, 1.0, 1.0, 100.0);
        let n = p.transform_point(Vec3::new(0.0, 0.0, -1.0));
        let f = p.transform_point(Vec3::new(0.0, 0.0, -100.0));
        assert!((n.z / n.w + 1.0).abs() < 1e-4);
        assert!((f.z / f.w - 1.0).abs() < 1e-4);
    }
}
