//! Geometría generada por código: esferas por subdivisión y anillos (toroides).
use crate::{Mesh, Vec3};
use std::collections::HashMap;
use std::f32::consts::PI;

fn midpoint(cache: &mut HashMap<(u32, u32), u32>, vertices: &mut Vec<Vec3>, a: u32, b: u32) -> u32 {
    let key = (a.min(b), a.max(b));
    *cache.entry(key).or_insert_with(|| {
        let m = ((vertices[a as usize] + vertices[b as usize]) * 0.5).normalize();
        vertices.push(m);
        vertices.len() as u32 - 1
    })
}

/// Esfera de radio 1: un icosaedro al que se le divide cada triángulo en 4, `subdivisions` veces.
/// Triángulos = 20 * 4^subdivisions. Las normales son las posiciones (sombreado suave).
pub fn icosphere(subdivisions: u32) -> Mesh {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut vertices: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..subdivisions {
        let mut cache = HashMap::new();
        let mut next = Vec::with_capacity(triangles.len() * 4);
        for &[a, b, c] in &triangles {
            let ab = midpoint(&mut cache, &mut vertices, a, b);
            let bc = midpoint(&mut cache, &mut vertices, b, c);
            let ca = midpoint(&mut cache, &mut vertices, c, a);
            next.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        triangles = next;
    }
    Mesh { normals: vertices.clone(), vertices, triangles }
}

/// Esfera de radio variable: cada vértice se mueve hacia afuera según `radius(dir)`.
/// Así el relieve es geometría real (la silueta del planeta deja de ser un círculo perfecto).
pub fn displaced_sphere(subdivisions: u32, radius: impl Fn(Vec3) -> f32) -> Mesh {
    let mut m = icosphere(subdivisions);
    for v in &mut m.vertices {
        *v = *v * radius(*v);
    }
    m.recompute_normals();
    m
}

/// Cono con la base en y = 0 y la punta en y = 1. Es la forma de prueba de los objetos de superficie
/// mientras no exista su modelo en `assets/props/`.
pub fn cone(segments: u32) -> Mesh {
    let mut vertices = vec![Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO];
    for i in 0..segments {
        let a = 2.0 * PI * i as f32 / segments as f32;
        vertices.push(Vec3::new(0.35 * a.cos(), 0.0, 0.35 * a.sin()));
    }
    let mut triangles = Vec::new();
    for i in 0..segments {
        let (a, b) = (2 + i, 2 + (i + 1) % segments);
        triangles.push([0, b, a]); // costado
        triangles.push([1, a, b]); // base
    }
    Mesh { vertices, triangles, normals: Vec::new() }
}

/// Anillo en el plano XY (se cruza volando a lo largo de Z). `major` = radio del anillo, `minor` = grosor.
pub fn torus(major: f32, minor: f32, segments: u32, sides: u32) -> Mesh {
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    for i in 0..segments {
        let u = 2.0 * PI * i as f32 / segments as f32;
        for j in 0..sides {
            let v = 2.0 * PI * j as f32 / sides as f32;
            let n = Vec3::new(v.cos() * u.cos(), v.cos() * u.sin(), v.sin());
            vertices.push(Vec3::new(major * u.cos(), major * u.sin(), 0.0) + n * minor);
            normals.push(n);
        }
    }
    let idx = |i: u32, j: u32| (i % segments) * sides + j % sides;
    let mut triangles = Vec::new();
    for i in 0..segments {
        for j in 0..sides {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    Mesh { vertices, triangles, normals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centroide_y_normal(m: &Mesh, t: &[u32; 3]) -> (Vec3, Vec3) {
        let [a, b, c] = [m.vertices[t[0] as usize], m.vertices[t[1] as usize], m.vertices[t[2] as usize]];
        ((a + b + c) * (1.0 / 3.0), (b - a).cross(c - a).normalize())
    }

    #[test]
    fn la_subdivision_multiplica_los_triangulos_por_cuatro() {
        for n in 0..4 {
            assert_eq!(icosphere(n).triangles.len(), 20 * 4usize.pow(n));
        }
    }

    #[test]
    fn todos_los_vertices_de_la_esfera_son_unitarios() {
        assert!(icosphere(3).vertices.iter().all(|v| (v.length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn las_caras_de_la_esfera_miran_hacia_afuera() {
        let m = icosphere(2);
        assert!(m.triangles.iter().all(|t| {
            let (c, n) = centroide_y_normal(&m, t);
            n.dot(c) > 0.0
        }));
    }

    #[test]
    fn las_caras_del_anillo_miran_hacia_afuera() {
        let m = torus(10.0, 1.0, 24, 8);
        assert!(m.triangles.iter().all(|t| {
            let (c, n) = centroide_y_normal(&m, t);
            let en_el_anillo = Vec3::new(c.x, c.y, 0.0).normalize() * 10.0;
            n.dot(c - en_el_anillo) > 0.0
        }));
    }

    #[test]
    fn la_esfera_deformada_respeta_el_radio_pedido() {
        let m = displaced_sphere(2, |d| 1.0 + 0.1 * d.y);
        assert!(m.vertices.iter().all(|v| (v.length() - (1.0 + 0.1 * v.normalize().y)).abs() < 1e-4));
        assert!(m.triangles.iter().all(|t| {
            let (c, n) = centroide_y_normal(&m, t);
            n.dot(c) > 0.0
        }));
    }

    #[test]
    fn las_caras_del_cono_miran_hacia_afuera() {
        let m = cone(8);
        let centro = Vec3::new(0.0, 0.3, 0.0);
        assert!(m.triangles.iter().all(|t| {
            let (c, n) = centroide_y_normal(&m, t);
            n.dot(c - centro) > 0.0
        }));
    }

    #[test]
    fn el_anillo_es_hueco_en_el_centro() {
        let m = torus(10.0, 1.0, 24, 8);
        assert!(m.vertices.iter().all(|v| Vec3::new(v.x, v.y, 0.0).length() > 8.9));
    }
}
