//! Lector de archivos Wavefront OBJ: geometría (`v` y `f`) si se da el .mtl un color por triángulo 
use crate::math::Vec3;
use std::{collections::HashMap, fmt};

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Una normal por vértice para sombreado suave. Vacío = sombreado plano por cara.
    pub normals: Vec<Vec3>,
    /// Color 0xRRGGBB de cada triángulo. Vacío = un solo material.
    pub colors: Vec<u32>,
}

#[derive(Debug, PartialEq)]
pub enum ObjError {
    NoEsUtf8,
    NumeroInvalido(usize),
    IndiceInvalido(usize),
    SinTriangulos,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObjError::NoEsUtf8 => write!(f, "el archivo no es texto UTF-8"),
            ObjError::NumeroInvalido(l) => write!(f, "número inválido en la línea {l}"),
            ObjError::IndiceInvalido(l) => write!(f, "índice de vértice inválido en la línea {l}"),
            ObjError::SinTriangulos => write!(f, "el modelo no tiene caras"),
        }
    }
}

const COLOR_SIN_MATERIAL: u32 = 0xB0B0B0;

/// Colores `Kd` de cada `newmtl`. Blender los guarda en lineal; se pasan a sRGB
pub fn parse_mtl(text: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    let mut actual: Option<String> = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("newmtl") => actual = parts.next().map(str::to_string),
            Some("Kd") => {
                let c: Vec<f32> = parts.filter_map(|s| s.parse().ok()).collect();
                if let (Some(name), [r, g, b, ..]) = (&actual, c.as_slice()) {
                    let ch = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u32;
                    out.insert(name.clone(), (ch(*r) << 16) | (ch(*g) << 8) | ch(*b));
                }
            }
            _ => {}
        }
    }
    out
}

pub fn parse(bytes: &[u8]) -> Result<Mesh, ObjError> {
    parse_with_mtl(bytes, None)
}

/// Como `parse`, pero con el texto del .mtl asigna a cada triángulo el color de su `usemtl`.
pub fn parse_with_mtl(bytes: &[u8], mtl: Option<&str>) -> Result<Mesh, ObjError> {
    let materiales = mtl.map(parse_mtl);
    let mut material: Option<String> = None;
    let text = std::str::from_utf8(bytes).map_err(|_| ObjError::NoEsUtf8)?;
    let mut mesh = Mesh::default();
    // (índice crudo, número de línea) de cada cara; se resuelven al final
    // porque los índices negativos dependen de cuántos `v` ya se leyeron.
    for (n, line) in text.lines().enumerate() {
        let n = n + 1;
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let mut c = [0.0f32; 3];
                for slot in c.iter_mut() {
                    *slot = parts
                        .next()
                        .and_then(|s| s.parse().ok())
                        .ok_or(ObjError::NumeroInvalido(n))?;
                }
                mesh.vertices.push(Vec3::new(c[0], c[1], c[2]));
            }
            Some("usemtl") => material = parts.next().map(str::to_string),
            Some("f") => {
                let mut idx = Vec::new();
                for tok in parts {
                    let raw: i64 = tok
                        .split('/')
                        .next()
                        .and_then(|s| s.parse().ok())
                        .ok_or(ObjError::IndiceInvalido(n))?;
                    let i = if raw > 0 {
                        raw - 1
                    } else if raw < 0 {
                        mesh.vertices.len() as i64 + raw
                    } else {
                        return Err(ObjError::IndiceInvalido(n));
                    };
                    if i < 0 || i as usize >= mesh.vertices.len() {
                        return Err(ObjError::IndiceInvalido(n));
                    }
                    idx.push(i as u32);
                }
                if idx.len() < 3 {
                    return Err(ObjError::IndiceInvalido(n));
                }
                // Los polígonos se parten en abanico: (0,1,2), (0,2,3), ...
                for k in 1..idx.len() - 1 {
                    mesh.triangles.push([idx[0], idx[k], idx[k + 1]]);
                    if let Some(m) = &materiales {
                        let color = material.as_ref().and_then(|n| m.get(n)).copied().unwrap_or(COLOR_SIN_MATERIAL);
                        mesh.colors.push(color);
                    }
                }
            }
            _ => {}
        }
    }
    if mesh.triangles.is_empty() {
        return Err(ObjError::SinTriangulos);
    }
    Ok(mesh)
}

impl Mesh {
    /// Cubo de lado 2 centrado en el origen (caras antihorarias vistas desde fuera).
    pub fn cube() -> Mesh {
        let v = [
            (-1.0, -1.0, -1.0), (1.0, -1.0, -1.0), (1.0, 1.0, -1.0), (-1.0, 1.0, -1.0),
            (-1.0, -1.0, 1.0), (1.0, -1.0, 1.0), (1.0, 1.0, 1.0), (-1.0, 1.0, 1.0),
        ];
        Mesh {
            vertices: v.iter().map(|&(x, y, z)| Vec3::new(x, y, z)).collect(),
            triangles: vec![
                [0, 3, 2], [0, 2, 1], [4, 5, 6], [4, 6, 7],
                [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
                [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
            ],
            normals: Vec::new(),
            colors: Vec::new(),
        }
    }

    /// Recalcula una normal suave por vértice promediando las caras que lo tocan.
    pub fn recompute_normals(&mut self) {
        let mut n = vec![Vec3::ZERO; self.vertices.len()];
        for t in &self.triangles {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let face = (self.vertices[b] - self.vertices[a]).cross(self.vertices[c] - self.vertices[a]);
            for i in [a, b, c] {
                n[i] += face;
            }
        }
        self.normals = n.into_iter().map(Vec3::normalize).collect();
    }

    /// Copia "parada en el suelo": centrada en X y Z, con la base en y = 0 y altura 1.
    /// Así un objeto de superficie se apoya sobre el planeta sin flotar ni hundirse.
    pub fn grounded(&self) -> Mesh {
        let mut min = self.vertices[0];
        let mut max = self.vertices[0];
        for v in &self.vertices {
            min = Vec3::new(min.x.min(v.x), min.y.min(v.y), min.z.min(v.z));
            max = Vec3::new(max.x.max(v.x), max.y.max(v.y), max.z.max(v.z));
        }
        let alto = (max.y - min.y).max(1e-6);
        let base = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
        Mesh {
            vertices: self.vertices.iter().map(|v| (*v - base) * (1.0 / alto)).collect(),
            triangles: self.triangles.clone(),
            normals: self.normals.clone(),
            colors: self.colors.clone(),
        }
    }

    pub fn rotated_y_180(&self) -> Mesh {
        let giro = |v: &Vec3| Vec3::new(-v.x, v.y, -v.z);
        Mesh {
            vertices: self.vertices.iter().map(giro).collect(),
            triangles: self.triangles.clone(),
            normals: self.normals.iter().map(giro).collect(),
            colors: self.colors.clone(),
        }
    }

    /// Copia centrada en su caja envolvente y escalada a radio 1.
    /// Así cualquier modelo se ve bien sin importar cómo se exportó.
    pub fn normalized(&self) -> Mesh {
        let mut min = self.vertices[0];
        let mut max = self.vertices[0];
        for v in &self.vertices {
            min = Vec3::new(min.x.min(v.x), min.y.min(v.y), min.z.min(v.z));
            max = Vec3::new(max.x.max(v.x), max.y.max(v.y), max.z.max(v.z));
        }
        let center = (min + max) * 0.5;
        let radius = self.vertices.iter().map(|v| (*v - center).length()).fold(0.0, f32::max);
        let k = if radius > 1e-9 { 1.0 / radius } else { 1.0 };
        Mesh {
            vertices: self.vertices.iter().map(|v| (*v - center) * k).collect(),
            triangles: self.triangles.clone(),
            normals: self.normals.clone(),
            colors: self.colors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_vertices_y_caras() {
        let m = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        assert_eq!(m.vertices.len(), 3);
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn parte_cuadrilateros_en_dos_triangulos() {
        let m = parse(b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
        assert_eq!(m.triangles, vec![[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn acepta_formato_con_uv_y_normales() {
        let m = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\nf 1/1/1 2/1/1 3/1/1\n").unwrap();
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn asigna_a_cada_triangulo_el_color_de_su_material() {
        let obj = b"v 0 0 0
v 1 0 0
v 0 1 0
usemtl rojo
f 1 2 3
usemtl azul
f 1 2 3
usemtl raro
f 1 2 3
";
        let mtl = "newmtl rojo
Kd 1 0 0
newmtl azul
Kd 0 0 1
";
        let m = parse_with_mtl(obj, Some(mtl)).unwrap();
        assert_eq!(m.colors, vec![0xFF0000, 0x0000FF, COLOR_SIN_MATERIAL]);
        assert!(parse(obj).unwrap().colors.is_empty());
    }

    #[test]
    fn acepta_indices_negativos() {
        let m = parse(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n").unwrap();
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn rechaza_indice_fuera_de_rango() {
        assert_eq!(parse(b"v 0 0 0\nf 1 2 3\n").unwrap_err(), ObjError::IndiceInvalido(2));
    }

    #[test]
    fn rechaza_modelo_sin_caras() {
        assert_eq!(parse(b"v 0 0 0\n").unwrap_err(), ObjError::SinTriangulos);
    }

    #[test]
    fn grounded_apoya_la_base_en_cero_con_altura_uno() {
        let m = parse(b"v 5 2 5\nv 7 2 5\nv 6 6 5\nf 1 2 3\n").unwrap().grounded();
        let (min_y, max_y) = m.vertices.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v.y), b.max(v.y)));
        assert!(min_y.abs() < 1e-6 && (max_y - 1.0).abs() < 1e-6);
        assert!(m.vertices.iter().map(|v| v.x).sum::<f32>().abs() < 1e-5, "centrado en X");
    }

    #[test]
    fn recompute_normals_da_normales_unitarias_hacia_afuera() {
        let mut m = Mesh::cube();
        m.recompute_normals();
        assert!(m.normals.iter().zip(&m.vertices).all(|(n, v)| (n.length() - 1.0).abs() < 1e-5 && n.dot(*v) > 0.0));
    }
    #[test]
    fn rotated_y_180_invierte_x_y_z_y_conserva_las_caras() {
        let m = parse(b"v 1 2 3
        v 0 0 0
        v 0 1 0
        f 1 2 3
        ").unwrap().rotated_y_180();
        assert_eq!(m.vertices[0], Vec3::new(-1.0, 2.0, -3.0));
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn normalized_centra_y_escala_a_radio_uno() {
        let m = parse(b"v 10 10 10\nv 14 10 10\nv 10 14 10\nf 1 2 3\n").unwrap().normalized();
        let max = m.vertices.iter().map(|v| v.length()).fold(0.0, f32::max);
        assert!((max - 1.0).abs() < 1e-5);
    }
}
