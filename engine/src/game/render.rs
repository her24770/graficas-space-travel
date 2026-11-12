//! Dibujo de la escena: sol, planetas, objetos, naves y la cámara.
use super::*;

impl Game {
    /// Nivel de detalle según cuántos píxeles mide el objeto en pantalla.
    pub(super) fn lod_index(dist: f32, radius: f32, focal: f32) -> usize {
        let px = radius / dist.max(0.001) * focal;
        if px > 120.0 {
            3
        } else if px > 40.0 {
            2
        } else if px > 12.0 {
            1
        } else {
            0
        }
    }

    /// Cámara activa: la del menú, la de cabina o la que sigue a la nave.
    pub(super) fn camera_pose(&self) -> (Vec3, Vec3) {
        if self.screen == Screen::Menu {
            return self.menu_camera();
        }
        match self.view {
            ViewMode::FirstPerson => {
                let rot = self.ship.rotation();
                let eye = self.ship.position + rot.transform_dir(Vec3::new(0.0, 0.3, 0.0));
                (eye, eye + rot.transform_dir(Vec3::new(0.0, 0.0, -10.0)))
            }
            ViewMode::ThirdPerson => (self.camera.eye, self.camera.target),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_object(
        &self, fb: &mut Framebuffer, v: &View, st: &mut RenderStats, mesh: &Mesh, model: &Mat4,
        center: Vec3, radius: f32, material: &Material,
    ) {
        st.total += 1;
        if v.frustum.sphere_visible(center, radius) {
            st.visible += 1;
            st.triangles += draw_mesh(fb, mesh, model, &v.vp, material, &v.scene);
        }
    }

    pub fn render(&self, fb: &mut Framebuffer) -> RenderStats {
        fb.clear(FONDO);
        let aspect = fb.width as f32 / fb.height as f32;
        let proj = Mat4::perspective(FOV_Y, aspect, 0.5, 6000.0);
        let menu = self.screen == Screen::Menu;
        let (eye, target) = self.camera_pose();
        let view = Mat4::look_at(eye, target, Vec3::Y);

        // Estrellas: solo rotan con la cámara, no se acercan al moverte.
        let sky = proj * view.without_translation();
        for s in &self.stars {
            raster::draw_point(fb, &sky, s.dir, s.color);
        }

        let vp = proj * view;
        let frustum = Frustum::from_view_proj(&vp);
        let v = View {
            vp,
            frustum: &frustum,
            scene: Scene { time: self.time, light_pos: Vec3::ZERO, eye, normal_maps: true },
            focal: fb.height as f32 * 0.5 / (FOV_Y * 0.5).tan(),
            eye,
        };
        let mut st = RenderStats::default();
        let t = self.world_time;

        // Sol
        let spin = Mat4::rotation_y((t * 0.02) as f32);
        let model = spin * Mat4::scale(Vec3::new(SUN_RADIUS, SUN_RADIUS, SUN_RADIUS));
        let mesh = &self.spheres[Self::lod_index(eye.length(), SUN_RADIUS, v.focal)];
        self.draw_object(fb, &v, &mut st, mesh, &model, Vec3::ZERO, SUN_RADIUS, &Material::Sun);

        // Planetas
        for (i, p) in PLANETS.iter().enumerate() {
            let pos = scenario::planet_position(p, t);
            let rot = scenario::planet_rotation(p, t);
            let model = Mat4::translation(pos) * rot * Mat4::scale(Vec3::new(p.radius, p.radius, p.radius));
            let lod = Self::lod_index(pos.distance(eye), p.radius, v.focal);
            let mesh = self.planet_lods[i].as_ref().map_or(&self.spheres[lod], |l| &l[lod]);
            self.draw_object(fb, &v, &mut st, mesh, &model, pos, scenario::max_radius(p), &p.material);

            // Anillos: modelo aparte, inclinados como el eje del planeta. Se dibujan por las dos caras.
            if let (Some(r), Some(mesh)) = (&p.rings, &self.ring_meshes[i]) {
                let k = p.radius * r.scale;
                let base = Mat4::translation(pos) * Mat4::rotation_z(p.tilt);
                let material = Material::Ring { a: r.colors.0, b: r.colors.1 };
                for cara in [Mat4::scale(Vec3::new(k, k, k)), Mat4::rotation_x(PI) * Mat4::scale(Vec3::new(k, k, k))] {
                    self.draw_object(fb, &v, &mut st, mesh, &(base * cara), pos, k, &material);
                }
            }
            // Meteoritos: modelo aparte, cada uno en su órbita y girando sobre sí mismo.
            if let (Some(m), Some(mesh)) = (&p.meteors, &self.meteor_meshes[i]) {
                for k in 0..m.count {
                    let o = scenario::meteor(m, k);
                    let centro = scenario::meteor_position(p, m, k, t);
                    let giro = Mat4::rotation_y(o.spin * t as f32) * Mat4::rotation_x(o.spin * 0.7 * t as f32);
                    let model = Mat4::translation(centro) * giro * Mat4::scale(Vec3::new(o.size, o.size, o.size));
                    self.draw_object(fb, &v, &mut st, mesh, &model, centro, o.size, &Material::Meteor);
                }
            }

            // Objetos de superficie: giran con el planeta porque cuelgan de su misma matriz. Solo de cerca.
            if pos.distance(eye) < p.radius * PROPS_DISTANCIA {
                let planeta = Mat4::translation(pos) * rot;
                for x in &self.placements[i] {
                    let model = planeta * x.local;
                    let centro = model.transform_point3(Vec3::new(0.0, 0.5, 0.0));
                    let mesh = &self.prop_meshes[self.prop_mesh_of[i][x.kind]];
                    self.draw_object(fb, &v, &mut st, mesh, &model, centro, x.size, &Material::Flat { color: p.props[x.kind].color });
                }
            }
        }

        // Agujeros negros
        for (mesh, h) in self.hole_meshes.iter().zip(BLACK_HOLES.iter()) {
            let k = h.radius;
            let base = Mat4::translation(h.position) * Mat4::rotation_z(h.tilt) * Mat4::rotation_y(h.spin * t as f32);
            for cara in [Mat4::scale(Vec3::new(k, k, k)), Mat4::rotation_x(PI) * Mat4::scale(Vec3::new(k, k, k))] {
                self.draw_object(fb, &v, &mut st, mesh, &(base * cara), h.position, k, &Material::BlackHole);
            }
            // Núcleo: una esfera negra
            let r = h.core_radius();
            let nucleo = &self.spheres[Self::lod_index(h.position.distance(eye), r, v.focal)];
            self.draw_object(fb, &v, &mut st, nucleo, &(Mat4::translation(h.position) * Mat4::scale(Vec3::new(r, r, r))), h.position, r, &Material::Emissive { color: 0x000000 });
        }

        // Ovnis se mueve en circulos, inclinados y girando
        for (mesh, u) in self.ufo_meshes.iter().zip(UFOS.iter()) {
            let a = (2.0 * std::f64::consts::PI * t / u.period as f64) as f32;
            let centro = scenario::ufo_position(u, t);
            let rumbo = Mat4::rotation_y(-a) * Mat4::rotation_z(0.25);
            let model = Mat4::translation(centro) * rumbo * Mat4::scale(Vec3::new(u.size, u.size, u.size));
            self.draw_object(fb, &v, &mut st, mesh, &model, centro, u.size * 1.1, &Material::Ufo);
        }

        // Ovnis modo persecucion
        if self.modo == Modo::Persecucion && !menu {
            let r = chase::CAZADOR_RADIO;
            if let Some(mesh) = self.ufo_meshes.first() {
                for o in &self.persecucion.ovnis {
                    let model = Mat4::translation(o.position) * Mat4::rotation_y(self.time * 2.0) * Mat4::rotation_z(0.25) * Mat4::scale(Vec3::new(r, r, r));
                    self.draw_object(fb, &v, &mut st, mesh, &model, o.position, r * 1.1, &Material::Ufo);
                }
            }
        }

        // Puntos de control (en carrera se ven los que faltan; el siguiente resalta)
        let racing = matches!(self.race.phase, Phase::Racing | Phase::Countdown) && !self.esperando();
        let desde = if racing { self.progress } else { 0 };
        let aros = if self.modo == Modo::Persecucion { self.course.len() } else { 0 };
        for (i, g) in self.course.iter().enumerate().skip(desde.max(aros)) {
            let (yaw, pitch) = scenario::orientation(g.dir);
            let model = Mat4::translation(g.pos) * Mat4::rotation_y(yaw) * Mat4::rotation_x(pitch);
            let color = if racing && i == self.progress { COLOR_SIGUIENTE } else { COLOR_PUNTO };
            self.draw_object(fb, &v, &mut st, &self.gate_mesh, &model, g.pos, g.radius + 1.0, &Material::Emissive { color });
        }

        // Rivales y mi nave
        for r in self.rivales() {
            let s = &r.shown;
            let modelo = &self.models[s.model as usize % self.models.len()];
            let (mesh, escala) = (&modelo.mesh, modelo.scale);
            let model = Mat4::translation(s.position) * Mat4::rotation_y(s.yaw) * Mat4::rotation_x(s.pitch) * escala_nave(escala);
            let color = COLORES_JUGADORES[s.id as usize % COLORES_JUGADORES.len()];
            let material = self.material_nave(s.model as usize % self.models.len(), color, Some(s.id));
            self.draw_object(fb, &v, &mut st, mesh, &model, s.position, escala, &material);
        }
        let (mesh, escala) = (&self.models[self.selected].mesh, self.models[self.selected].scale);
        if menu {
            // Vista previa: la nave gira frente a la cámara, con una luz propia desde un costado.
            let f = (target - eye).normalize();
            let (right, up) = (f.cross(Vec3::Y).normalize(), f.cross(Vec3::Y).normalize().cross(f));
            let pos = eye + f * 7.0 + up * 0.5;
            let model = Mat4::translation(pos) * Mat4::rotation_y(self.time * 0.9) * Mat4::rotation_x(0.35) * escala_nave(escala);
            let luz = Scene { light_pos: eye + right * 25.0 + up * 18.0 - f * 8.0, ..v.scene };
            let vista = View { vp: v.vp, frustum: v.frustum, scene: luz, focal: v.focal, eye: v.eye };
            self.draw_object(fb, &vista, &mut st, mesh, &model, pos, escala, &Material::Metal { color: COLOR_PROPIO, tint: None });
            self.draw_menu(fb);
        } else {
            // En primera persona la nave propia no se dibuja: no tapa la vista.
            if self.view == ViewMode::ThirdPerson {
                let model = Mat4::translation(self.ship.position) * self.ship.rotation() * escala_nave(escala);
                let material = self.material_nave(self.selected, COLOR_PROPIO, self.my_id);
                self.draw_object(fb, &v, &mut st, mesh, &model, self.ship.position, escala, &material);
            }
            self.sparks.draw(fb, &v.vp);
            self.draw_hud(fb, &v, &st);
        }
        st
    }

    /// Si otro jugador usa el mismo modelo de nave, se le pon encima el tinte del color del jugador 
    pub(super) fn material_nave(&self, modelo: usize, color: u32, id: Option<u32>) -> Material {
        let n = self.models.len();
        let usos = usize::from(self.selected == modelo) + self.rivales().filter(|r| r.shown.model as usize % n == modelo).count();
        let tint = id.filter(|_| usos > 1).map(color_jugador);
        Material::Metal { color, tint }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::*;

    #[test]
    fn renderiza_planetas_y_cuenta_triangulos_y_objetos() {
        let g = Game::new(1);
        let mut fb = Framebuffer::new(320, 180);
        let st = g.render(&mut fb);
        assert!(st.triangles > 0);
        assert!(st.visible > 0 && st.visible <= st.total);
        assert!(fb.pixels.iter().any(|&p| p != FONDO));
    }

    #[test]
    fn el_frustum_culling_descarta_lo_que_queda_fuera() {
        let mut g = jugando();
        // Mirando hacia afuera del sistema solo se ve una parte de los objetos.
        g.ship.reset(Vec3::new(900.0, 0.0, 0.0), -PI / 2.0, 0.0);
        g.camera = ChaseCamera::new(&g.ship);
        let st = g.render(&mut Framebuffer::new(160, 90));
        assert!(st.visible < st.total, "{st:?}");
    }

    #[test]
    fn el_lod_usa_menos_triangulos_de_lejos() {
        let g = Game::new(1);
        let (cerca, lejos) = (Game::lod_index(40.0, 10.0, 400.0), Game::lod_index(4000.0, 10.0, 400.0));
        assert!(g.spheres[cerca].triangles.len() > g.spheres[lejos].triangles.len());
    }

    #[test]
    fn un_rival_a_la_vista_cambia_la_imagen() {
        let mut g = jugando();
        let (mut sin, mut con) = (Framebuffer::new(320, 240), Framebuffer::new(320, 240));
        g.render(&mut sin);
        let adelante = g.ship.position + g.ship.forward() * 12.0;
        g.set_remote_players(&[RemoteShip { position: adelante + Vec3::new(3.0, 0.0, 0.0), ..rival(3, 0.0) }]);
        g.render(&mut con);
        assert_ne!(sin.pixels, con.pixels);
    }

    #[test]
    fn el_agujero_negro_se_ve_cuando_se_mira_hacia_el_y_deja_pasar_el_fondo() {
        let mut g = jugando();
        let h = &BLACK_HOLES[0];
        assert_eq!(g.hole_meshes.len(), BLACK_HOLES.len());
        let (mut lejos, mut cerca) = (Framebuffer::new(320, 240), Framebuffer::new(320, 240));
        g.render(&mut lejos);
        // Ponemos la nave frente al disco, mirando hacia él.
        let (yaw, pitch) = scenario::orientation(Vec3::new(0.0, 0.0, -1.0));
        g.ship.reset(h.position + Vec3::new(0.0, 8.0, h.radius * 2.5), yaw, pitch);
        g.camera = ChaseCamera::new(&g.ship);
        g.render(&mut cerca);
        assert_ne!(lejos.pixels, cerca.pixels);
        // Hay píxeles brillantes de las grietas y los puntos: más claros que el fondo del espacio.
        let claros = cerca.pixels.iter().filter(|&&p| (p >> 16) > 0x90 && (p & 0xFF) > 0x70).count();
        assert!(claros > 0, "el disco tiene que mostrar algo brillante");
    }

    #[test]
    fn v_alterna_la_vista_y_en_primera_persona_no_se_dibuja_la_nave_propia() {
        let mut g = jugando();
        poner(&mut g, Vec3::new(0.0, 900.0, 0.0)); // lejos de todo: solo cambia la nave propia
        let tercera = g.render(&mut Framebuffer::new(160, 90));
        g.update(0.0, &Input { toggle_view: true, ..NADA });
        assert_eq!(g.view_mode(), ViewMode::FirstPerson);
        let primera = g.render(&mut Framebuffer::new(160, 90));
        assert_eq!(tercera.total, primera.total + 1);
        g.update(0.0, &Input { toggle_view: true, ..NADA });
        assert_eq!(g.view_mode(), ViewMode::ThirdPerson);
    }

    #[test]
    fn la_camara_de_primera_persona_esta_en_la_nave_y_mira_al_frente() {
        let mut g = jugando();
        g.update(0.0, &Input { toggle_view: true, ..NADA });
        let (eye, target) = g.camera_pose();
        assert!(eye.distance(g.ship.position) < 0.5);
        assert!((target - eye).normalize().dot(g.ship.forward()) > 0.999);
    }

    #[test]
    fn los_objetos_de_superficie_se_dibujan_solo_de_cerca() {
        let mut g = jugando();
        let (centro, radio) = aqua(&g);
        poner(&mut g, centro + Vec3::new(0.0, radio * 3.0, radio * 3.0));
        let cerca = g.render(&mut Framebuffer::new(160, 90));
        poner(&mut g, Vec3::new(0.0, 3000.0, 0.0));
        let lejos = g.render(&mut Framebuffer::new(160, 90));
        assert!(cerca.total > lejos.total + 50, "cerca {} / lejos {}", cerca.total, lejos.total);
    }
}
