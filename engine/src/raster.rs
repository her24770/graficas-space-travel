//! Rasterizador de triángulos propio: recorte contra el plano cercano, división de
//! perspectiva, descarte de caras traseras, z-buffer e interpolación de atributos
//! corregida por perspectiva para el fragment shader.
use crate::normalmap::{self, NormalMap};
use crate::shaders::{blend, discards, shade, Fragment, Material, Scene};

const FUERZA_TINTE: f32 = 0.45;
use crate::{Framebuffer, Mat4, Mesh, Vec3, Vec4};

/// Atributos que se interpolan por píxel.
#[derive(Clone, Copy)]
struct Varyings {
    world: Vec3,
    normal: Vec3,
    local: Vec3,
    /// Tangente del mapa de normales (cero si el material no usa uno).
    tangent: Vec3,
}

impl Varyings {
    fn lerp(a: Varyings, b: Varyings, t: f32) -> Varyings {
        Varyings {
            world: a.world.lerp(b.world, t),
            normal: a.normal.lerp(b.normal, t),
            local: a.local.lerp(b.local, t),
            tangent: a.tangent.lerp(b.tangent, t),
        }
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
    // Mapa de normales: la tangente de cada vértice (dirección en que crece `u` de la textura) pasa al mundo
    // con el modelo, igual que las normales. Solo se calcula para los materiales que lo usan.
    let mapped = scene.normal_maps && material.normal_map().is_some();
    let tangents: Vec<Vec3> = if mapped {
        mesh.vertices.iter().map(|v| model.transform_dir(normalmap::sphere_tangent(v.normalize())).normalize()).collect()
    } else {
        Vec::new()
    };
    let mut drawn = 0;

    for (ti, t) in mesh.triangles.iter().enumerate() {
        // El color de cada parte viene del modelo y se mezcla el tinte del jugador
        let propio = mesh.colors.get(ti).copied();
        let material = &match *material {
            Material::Metal { color, tint } => {
                let base = propio.unwrap_or(color);
                Material::Metal { color: tint.map_or(base, |t| blend(base, t, FUERZA_TINTE)), tint: None }
            }
            Material::Flat { .. } if mesh.glow.get(ti) == Some(&true) => Material::Magma,
            Material::Flat { color } => Material::Flat { color: propio.unwrap_or(color) },
            m => m,
        };
        let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
        let face = (world[idx[1]] - world[idx[0]]).cross(world[idx[2]] - world[idx[0]]).normalize();
        let tri = idx.map(|i| ClipVertex {
            pos: clip[i],
            var: Varyings {
                world: world[i],
                normal: if smooth { normals[i] } else { face },
                local: mesh.vertices[i],
                tangent: if mapped { tangents[i] } else { Vec3::ZERO },
            },
        });

        let (poly, n) = clip_near(tri);
        for j in 1..n.saturating_sub(1) {
            if fill_triangle(fb, [poly[0], poly[j], poly[j + 1]], material, scene, mapped) {
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

/// Nivel de mipmap del mapa de normales para un triángulo: cuántos texels cubre cada píxel.
/// `pixel_area2` es el doble del área del triángulo en píxeles (la que ya calcula el rasterizador).
/// Se compara el área que ocupa en la textura con la que ocupa en pantalla.
fn normal_map_lod(tri: &[ClipVertex; 3], pixel_area2: f32) -> f32 {
    let dirs = tri.map(|v| v.var.local.normalize());
    // En el polo la longitud no está definida: ahí el relieve se aplana en vez de centellear.
    if dirs.iter().any(|d| d.y.abs() > 0.9995) {
        return f32::MAX;
    }
    let uv = dirs.map(normalmap::sphere_uv);
    // Un triángulo que cruza la costura (u = 0 / 1) se desenvuelve para medirlo.
    let unwrap = |du: f32| if du > 0.5 { du - 1.0 } else if du < -0.5 { du + 1.0 } else { du };
    let (du1, du2) = (unwrap(uv[1].0 - uv[0].0) * normalmap::WIDTH as f32, unwrap(uv[2].0 - uv[0].0) * normalmap::WIDTH as f32);
    let (dv1, dv2) = ((uv[1].1 - uv[0].1) * normalmap::HEIGHT as f32, (uv[2].1 - uv[0].1) * normalmap::HEIGHT as f32);
    let texel_area2 = (du1 * dv2 - du2 * dv1).abs();
    NormalMap::lod_for((texel_area2 / pixel_area2).sqrt())
}

/// Rasteriza un triángulo en coordenadas de recorte. Devuelve false si se descartó.
fn fill_triangle(fb: &mut Framebuffer, tri: [ClipVertex; 3], material: &Material, scene: &Scene, mapped: bool) -> bool {
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

    let lod = if mapped { normal_map_lod(&tri, area.abs()) } else { 0.0 };

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

            // Atributos: se interpola attr/w y se divide por 1/w (corrección de perspectiva).
            let (q0, q1, q2) = (b0 * inv_w[0], b1 * inv_w[1], b2 * inv_w[2]);
            let k = 1.0 / (q0 + q1 + q2);
            let mix = |f: fn(&Varyings) -> Vec3| (f(&tri[0].var) * q0 + f(&tri[1].var) * q1 + f(&tri[2].var) * q2) * k;
            let frag = Fragment {
                world: mix(|v| v.world),
                normal: mix(|v| v.normal),
                local: mix(|v| v.local),
                tangent: if mapped { mix(|v| v.tangent) } else { Vec3::ZERO },
                lod,
            };
            // Un píxel descartado (hueco de los anillos) no escribe color ni profundidad.
            if discards(material, &frag) {
                continue;
            }
            fb.depth[idx] = z;
            fb.pixels[idx] = shade(material, &frag, scene);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escena() -> Scene {
        Scene { time: 0.0, light_pos: Vec3::new(0.0, 0.0, 100.0), eye: Vec3::new(0.0, 0.0, 5.0), normal_maps: true }
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

    fn esfera_rocosa() -> Material {
        Material::Rocky { sea: 0x1E5FB8, land: 0x3E8E41, peak: 0x8D7B68, seed: 4.0 }
    }

    fn dibujar_esfera(maps: bool) -> (Framebuffer, usize) {
        let mut fb = Framebuffer::new(160, 160);
        let esfera = crate::procedural::icosphere(4);
        let model = Mat4::translation(Vec3::new(0.0, 0.0, -3.0)) * Mat4::scale(Vec3::new(2.0, 2.0, 2.0));
        let scene = Scene { normal_maps: maps, light_pos: Vec3::new(0.0, 0.0, 100.0), ..escena() };
        let n = draw_mesh(&mut fb, &esfera, &model, &camara(), &esfera_rocosa(), &scene);
        (fb, n)
    }

    #[test]
    fn el_mapa_de_normales_agrega_detalle_sin_agregar_triangulos() {
        let (con, n_con) = dibujar_esfera(true);
        let (sin, n_sin) = dibujar_esfera(false);
        assert_eq!(n_con, n_sin);
        let distintos = con.pixels.iter().zip(&sin.pixels).filter(|(a, b)| a != b).count();
        assert!(distintos > 500, "solo {distintos} píxeles cambiaron");
        // Más variedad de tonos: el relieve rompe el degradado liso.
        let tonos = |fb: &Framebuffer| fb.pixels.iter().copied().collect::<std::collections::HashSet<u32>>().len();
        assert!(tonos(&con) > tonos(&sin), "{} vs {}", tonos(&con), tonos(&sin));
    }

    fn vertice(local: Vec3) -> ClipVertex {
        ClipVertex { pos: Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, var: Varyings { world: Vec3::ZERO, normal: local, local, tangent: Vec3::ZERO } }
    }

    #[test]
    fn el_lod_del_mapa_sube_cuando_cada_pixel_cubre_mas_texels() {
        let tri = [vertice(Vec3::new(0.5, 0.1, 0.8)), vertice(Vec3::new(0.52, 0.1, 0.8)), vertice(Vec3::new(0.5, 0.12, 0.8))];
        // El mismo triángulo con 16 veces menos píxeles (4 veces menos por lado): 2 niveles más.
        let (cerca, lejos) = (normal_map_lod(&tri, 4.0), normal_map_lod(&tri, 0.25));
        assert!(lejos > cerca + 1.9 && lejos < cerca + 2.1, "{cerca} {lejos}");
        assert_eq!(normal_map_lod(&tri, 1e9), 0.0, "si cada texel cubre varios píxeles se usa el nivel más fino");
    }

}
