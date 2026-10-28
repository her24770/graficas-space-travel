//! Rasterizador de triángulos propio: recorte contra el plano cercano, división de
//! perspectiva, descarte de caras traseras, z-buffer e interpolación de atributos
//! corregida por perspectiva para el fragment shader.
use crate::shaders::{shade, Fragment, Material, Scene};
use crate::{Framebuffer, Mat4, Mesh, Vec3, Vec4};

/// Atributos que se interpolan por píxel.
#[derive(Clone, Copy)]
struct Varyings {
    world: Vec3,
    normal: Vec3,
    local: Vec3,
}

impl Varyings {
    fn lerp(a: Varyings, b: Varyings, t: f32) -> Varyings {
        Varyings { world: a.world.lerp(b.world, t), normal: a.normal.lerp(b.normal, t), local: a.local.lerp(b.local, t) }
    }
}

#[derive(Clone, Copy)]
struct ClipVertex {
    pos: Vec4,
    var: Varyings,
}

/// Dibuja `mesh` y devuelve cuántos triángulos llegaron a rasterizarse.
pub fn draw_mesh(
    fb: &mut Framebuffer,
    mesh: &Mesh,
    model: &Mat4,
    view_proj: &Mat4,
    material: &Material,
    scene: &Scene,
) -> usize {
    let world: Vec<Vec3> = mesh.vertices.iter().map(|v| model.transform_point3(*v)).collect();
    let clip: Vec<Vec4> = world.iter().map(|v| view_proj.transform_point(*v)).collect();
    // Con normales por vértice el sombreado es suave; sin ellas, plano por cara.
    let normals: Vec<Vec3> = mesh.normals.iter().map(|n| model.transform_dir(*n).normalize()).collect();
    let smooth = !normals.is_empty();
    let mut drawn = 0;

    for (ti, t) in mesh.triangles.iter().enumerate() {
        let material = &match (mesh.colors.get(ti), *material) {
            (Some(&c), Material::Metal { .. }) => Material::Metal { color: c },
            (Some(&c), Material::Flat { .. }) => Material::Flat { color: c },
            (_, m) => m,
        };
        let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
        let face = (world[idx[1]] - world[idx[0]]).cross(world[idx[2]] - world[idx[0]]).normalize();
        let tri = idx.map(|i| ClipVertex {
            pos: clip[i],
            var: Varyings { world: world[i], normal: if smooth { normals[i] } else { face }, local: mesh.vertices[i] },
        });

        let (poly, n) = clip_near(tri);
        for j in 1..n.saturating_sub(1) {
            if fill_triangle(fb, [poly[0], poly[j], poly[j + 1]], material, scene) {
                drawn += 1;
            }
        }
    }
    drawn
}

/// Dibuja puntos lejanos (estrellas) que solo dependen de la rotación de la cámara.
pub fn draw_point(fb: &mut Framebuffer, view_proj: &Mat4, dir: Vec3, color: u32) {
    if let Some((x, y, _)) = project(view_proj, dir, fb.width, fb.height) {
        if x >= 0.0 && y >= 0.0 {
            fb.set_pixel(x as usize, y as usize, color);
        }
    }
}

/// Proyecta un punto a píxeles. Devuelve (x, y, w); None si está detrás de la cámara.
pub fn project(view_proj: &Mat4, p: Vec3, width: usize, height: usize) -> Option<(f32, f32, f32)> {
    let c = view_proj.transform_point(p);
    if c.w <= 1e-4 {
        return None;
    }
    let (x, y) = (c.x / c.w, c.y / c.w);
    Some(((x * 0.5 + 0.5) * width as f32, (1.0 - (y * 0.5 + 0.5)) * height as f32, c.w))
}

/// Recorta el triángulo contra el plano cercano (z >= -w). Devuelve hasta 4 vértices.
fn clip_near(tri: [ClipVertex; 3]) -> ([ClipVertex; 4], usize) {
    let mut out = [tri[0]; 4];
    let mut n = 0;
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = (a.pos.z + a.pos.w, b.pos.z + b.pos.w);
        if da >= 0.0 {
            out[n] = a;
            n += 1;
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out[n] = ClipVertex { pos: Vec4::lerp(a.pos, b.pos, t), var: Varyings::lerp(a.var, b.var, t) };
            n += 1;
        }
    }
    (out, n)
}

/// Rasteriza un triángulo en coordenadas de recorte. Devuelve false si se descartó.
fn fill_triangle(fb: &mut Framebuffer, tri: [ClipVertex; 3], material: &Material, scene: &Scene) -> bool {
    let (w, h) = (fb.width as f32, fb.height as f32);
    let mut p = [(0.0f32, 0.0f32, 0.0f32); 3];
    let mut ndc = [(0.0f32, 0.0f32); 3];
    let mut inv_w = [0.0f32; 3];
    for (i, v) in tri.iter().enumerate() {
        inv_w[i] = 1.0 / v.pos.w;
        let (nx, ny, nz) = (v.pos.x * inv_w[i], v.pos.y * inv_w[i], v.pos.z * inv_w[i]);
        ndc[i] = (nx, ny);
        p[i] = ((nx * 0.5 + 0.5) * w, (1.0 - (ny * 0.5 + 0.5)) * h, nz);
    }

    // Cara trasera: en NDC (Y arriba) las caras frontales son antihorarias.
    let area_ndc = (ndc[1].0 - ndc[0].0) * (ndc[2].1 - ndc[0].1) - (ndc[1].1 - ndc[0].1) * (ndc[2].0 - ndc[0].0);
    if area_ndc <= 0.0 {
        return false;
    }

    let edge = |a: (f32, f32, f32), b: (f32, f32, f32), x: f32, y: f32| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
    let area = edge(p[0], p[1], p[2].0, p[2].1);
    if area.abs() < 1e-9 {
        return false;
    }

    let min_x = p.iter().map(|v| v.0).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let max_x = p.iter().map(|v| v.0).fold(f32::NEG_INFINITY, f32::max).ceil().min(w - 1.0).max(0.0) as usize;
    let min_y = p.iter().map(|v| v.1).fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let max_y = p.iter().map(|v| v.1).fold(f32::NEG_INFINITY, f32::max).ceil().min(h - 1.0).max(0.0) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let b0 = edge(p[1], p[2], px, py) / area;
            let b1 = edge(p[2], p[0], px, py) / area;
            let b2 = edge(p[0], p[1], px, py) / area;
            if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                continue;
            }
            // z en NDC es lineal en pantalla, se interpola directo.
            let z = b0 * p[0].2 + b1 * p[1].2 + b2 * p[2].2;
            let idx = y * fb.width + x;
            if z > 1.0 || z >= fb.depth[idx] {
                continue;
            }
            fb.depth[idx] = z;

            // Atributos: se interpola attr/w y se divide por 1/w (corrección de perspectiva).
            let (q0, q1, q2) = (b0 * inv_w[0], b1 * inv_w[1], b2 * inv_w[2]);
            let k = 1.0 / (q0 + q1 + q2);
            let mix = |f: fn(&Varyings) -> Vec3| (f(&tri[0].var) * q0 + f(&tri[1].var) * q1 + f(&tri[2].var) * q2) * k;
            let frag = Fragment { world: mix(|v| v.world), normal: mix(|v| v.normal), local: mix(|v| v.local) };
            fb.pixels[idx] = shade(material, &frag, scene);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escena() -> Scene {
        Scene { time: 0.0, light_pos: Vec3::new(0.0, 0.0, 100.0), eye: Vec3::new(0.0, 0.0, 5.0) }
    }

    fn triangulo(z: f32) -> Mesh {
        Mesh {
            vertices: vec![Vec3::new(-1.0, -1.0, z), Vec3::new(1.0, -1.0, z), Vec3::new(0.0, 1.0, z)],
            triangles: vec![[0, 1, 2]],
            normals: Vec::new(),
            ..Default::default()
        }
    }

    fn camara() -> Mat4 {
        Mat4::perspective(1.0, 1.0, 0.1, 100.0) * Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y)
    }

    fn centro(fb: &Framebuffer) -> u32 {
        fb.pixels[fb.height / 2 * fb.width + fb.width / 2]
    }

    fn rojo() -> Material {
        Material::Emissive { color: 0xFF0000 }
    }

    #[test]
    fn dibuja_un_triangulo_de_frente() {
        let mut fb = Framebuffer::new(64, 64);
        let n = draw_mesh(&mut fb, &triangulo(0.0), &Mat4::IDENTITY, &camara(), &rojo(), &escena());
        assert_eq!(n, 1);
        assert_eq!(centro(&fb), 0xFF0000);
    }

    #[test]
    fn descarta_la_cara_trasera() {
        let mut fb = Framebuffer::new(64, 64);
        let mut m = triangulo(0.0);
        m.triangles = vec![[0, 2, 1]];
        assert_eq!(draw_mesh(&mut fb, &m, &Mat4::IDENTITY, &camara(), &rojo(), &escena()), 0);
        assert_eq!(centro(&fb), 0);
    }

    #[test]
    fn el_zbuffer_deja_ganar_al_mas_cercano_en_cualquier_orden() {
        for cerca_primero in [true, false] {
            let mut fb = Framebuffer::new(64, 64);
            let orden = if cerca_primero { [(1.0, 0x0000FF), (-1.0, 0xFF0000)] } else { [(-1.0, 0xFF0000), (1.0, 0x0000FF)] };
            for (z, color) in orden {
                draw_mesh(&mut fb, &triangulo(z), &Mat4::IDENTITY, &camara(), &Material::Emissive { color }, &escena());
            }
            assert_eq!(centro(&fb), 0x0000FF);
        }
    }

    #[test]
    fn no_revienta_con_un_triangulo_que_cruza_la_camara() {
        let mut fb = Framebuffer::new(64, 64);
        let m = Mesh {
            vertices: vec![Vec3::new(-1.0, -1.0, 10.0), Vec3::new(1.0, -1.0, 10.0), Vec3::new(0.0, 1.0, -10.0)],
            triangles: vec![[0, 1, 2]],
            normals: Vec::new(),
            ..Default::default()
        };
        draw_mesh(&mut fb, &m, &Mat4::IDENTITY, &camara(), &rojo(), &escena());
    }

    #[test]
    fn el_sombreado_suave_varia_a_lo_largo_de_una_esfera() {
        let mut fb = Framebuffer::new(96, 96);
        let esfera = crate::procedural::icosphere(3);
        let model = Mat4::scale(Vec3::new(2.0, 2.0, 2.0));
        draw_mesh(&mut fb, &esfera, &model, &camara(), &Material::Flat { color: 0xFFFFFF }, &escena());
        let tonos: std::collections::HashSet<u32> = fb.pixels.iter().copied().filter(|&p| p != 0).collect();
        assert!(tonos.len() > 20, "una esfera suave debe tener un degradado, no pocos tonos planos ({})", tonos.len());
    }

    #[test]
    fn project_descarta_lo_que_esta_detras() {
        assert!(project(&camara(), Vec3::new(0.0, 0.0, 50.0), 64, 64).is_none());
        assert!(project(&camara(), Vec3::ZERO, 64, 64).is_some());
    }
}
