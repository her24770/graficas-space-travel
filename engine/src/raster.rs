//! Rasterizador de triángulos propio: recorte contra el plano cercano,
//! división de perspectiva, descarte de caras traseras, z-buffer y
//! sombreado plano con una luz direccional.
use crate::{Framebuffer, Mat4, Mesh, Vec3, Vec4};

const AMBIENTE: f32 = 0.25;

/// Dibuja `mesh` y devuelve cuántos triángulos llegaron a rasterizarse.
/// `light_dir` es la dirección hacia la que viaja la luz.
pub fn draw_mesh(
    fb: &mut Framebuffer,
    mesh: &Mesh,
    model: &Mat4,
    view_proj: &Mat4,
    color: u32,
    light_dir: Vec3,
) -> usize {
    let to_light = -light_dir.normalize();
    let world: Vec<Vec3> = mesh.vertices.iter().map(|v| model.transform_point3(*v)).collect();
    let clip: Vec<Vec4> = world.iter().map(|v| view_proj.transform_point(*v)).collect();
    let mut drawn = 0;

    for t in &mesh.triangles {
        let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let normal = (world[i1] - world[i0]).cross(world[i2] - world[i0]).normalize();
        let k = AMBIENTE + (1.0 - AMBIENTE) * normal.dot(to_light).max(0.0);
        let shaded = scale_color(color, k);

        let (poly, n) = clip_near([clip[i0], clip[i1], clip[i2]]);
        for j in 1..n.saturating_sub(1) {
            if fill_triangle(fb, [poly[0], poly[j], poly[j + 1]], shaded) {
                drawn += 1;
            }
        }
    }
    drawn
}

/// Dibuja puntos lejanos (estrellas) que solo dependen de la rotación de la cámara.
pub fn draw_point(fb: &mut Framebuffer, view_proj: &Mat4, dir: Vec3, color: u32) {
    let c = view_proj.transform_point(dir);
    if c.w <= 1e-4 {
        return;
    }
    let (x, y) = (c.x / c.w, c.y / c.w);
    if x.abs() > 1.0 || y.abs() > 1.0 {
        return;
    }
    let sx = ((x * 0.5 + 0.5) * fb.width as f32) as usize;
    let sy = ((1.0 - (y * 0.5 + 0.5)) * fb.height as f32) as usize;
    fb.set_pixel(sx, sy, color);
}

fn scale_color(c: u32, k: f32) -> u32 {
    let ch = |shift: u32| (((c >> shift) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Recorta el triángulo contra el plano cercano (z >= -w). Devuelve hasta 4 vértices.
fn clip_near(tri: [Vec4; 3]) -> ([Vec4; 4], usize) {
    let mut out = [Vec4::ZERO; 4];
    let mut n = 0;
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = (a.z + a.w, b.z + b.w);
        if da >= 0.0 {
            out[n] = a;
            n += 1;
        }
        if (da >= 0.0) != (db >= 0.0) {
            out[n] = Vec4::lerp(a, b, da / (da - db));
            n += 1;
        }
    }
    (out, n)
}

/// Rasteriza un triángulo en coordenadas de recorte. Devuelve false si se descartó.
fn fill_triangle(fb: &mut Framebuffer, tri: [Vec4; 3], color: u32) -> bool {
    let (w, h) = (fb.width as f32, fb.height as f32);
    let mut p = [(0.0f32, 0.0f32, 0.0f32); 3];
    let mut ndc = [(0.0f32, 0.0f32); 3];
    for (i, v) in tri.iter().enumerate() {
        let inv_w = 1.0 / v.w;
        let (nx, ny, nz) = (v.x * inv_w, v.y * inv_w, v.z * inv_w);
        ndc[i] = (nx, ny);
        p[i] = ((nx * 0.5 + 0.5) * w, (1.0 - (ny * 0.5 + 0.5)) * h, nz);
    }

    // Cara trasera: en NDC (Y arriba) las caras frontales son antihorarias.
    let area_ndc = (ndc[1].0 - ndc[0].0) * (ndc[2].1 - ndc[0].1)
        - (ndc[1].1 - ndc[0].1) * (ndc[2].0 - ndc[0].0);
    if area_ndc <= 0.0 {
        return false;
    }

    let edge = |a: (f32, f32, f32), b: (f32, f32, f32), x: f32, y: f32| {
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)
    };
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
            if z <= 1.0 && z < fb.depth[idx] {
                fb.depth[idx] = z;
                fb.pixels[idx] = color;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const LUZ: Vec3 = Vec3::new(0.0, 0.0, -1.0); // ilumina caras que miran a +Z

    fn triangulo(z: f32) -> Mesh {
        Mesh {
            vertices: vec![Vec3::new(-1.0, -1.0, z), Vec3::new(1.0, -1.0, z), Vec3::new(0.0, 1.0, z)],
            triangles: vec![[0, 1, 2]],
        }
    }

    fn camara() -> Mat4 {
        Mat4::perspective(1.0, 1.0, 0.1, 100.0) * Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y)
    }

    fn centro(fb: &Framebuffer) -> u32 {
        fb.pixels[fb.height / 2 * fb.width + fb.width / 2]
    }

    #[test]
    fn dibuja_un_triangulo_de_frente() {
        let mut fb = Framebuffer::new(64, 64);
        let n = draw_mesh(&mut fb, &triangulo(0.0), &Mat4::IDENTITY, &camara(), 0xFF0000, LUZ);
        assert_eq!(n, 1);
        assert_eq!(centro(&fb), 0xFF0000);
    }

    #[test]
    fn descarta_la_cara_trasera() {
        let mut fb = Framebuffer::new(64, 64);
        let mut m = triangulo(0.0);
        m.triangles = vec![[0, 2, 1]];
        assert_eq!(draw_mesh(&mut fb, &m, &Mat4::IDENTITY, &camara(), 0xFF0000, LUZ), 0);
        assert_eq!(centro(&fb), 0);
    }

    #[test]
    fn el_zbuffer_deja_ganar_al_mas_cercano_en_cualquier_orden() {
        for cerca_primero in [true, false] {
            let mut fb = Framebuffer::new(64, 64);
            let orden = if cerca_primero { [(1.0, 0x0000FF), (-1.0, 0xFF0000)] } else { [(-1.0, 0xFF0000), (1.0, 0x0000FF)] };
            for (z, color) in orden {
                draw_mesh(&mut fb, &triangulo(z), &Mat4::IDENTITY, &camara(), color, LUZ);
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
        };
        draw_mesh(&mut fb, &m, &Mat4::IDENTITY, &camara(), 0xFFFFFF, LUZ);
    }
}
