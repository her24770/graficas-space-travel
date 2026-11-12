//! Gravedad, choques y derrota: lo que pasa cuando la nave toca algo.
use super::*;

impl Game {
    /// cercania de agujero negro y ovni para el sonido 
    pub fn black_hole_proximity(&self) -> f32 {
        BLACK_HOLES
            .iter()
            .map(|h| 1.0 - (self.ship.position.distance(h.position) - h.core_radius()) / AGUJERO_ALCANCE)
            .fold(0.0, f32::max)
            .clamp(0.0, 1.0)
    }

    pub fn ufo_proximity(&self) -> f32 {
        let t = self.world_time;
        UFOS.iter()
            .map(|u| 1.0 - (self.ship.position.distance(scenario::ufo_position(u, t)) - u.size) / OVNI_ALCANCE)
            .fold(self.cazador_cercania(), f32::max)
            .clamp(0.0, 1.0)
    }

    /// Sol y planetas en este instante, con su gravedad: 0 = sol, 1.. = planetas.
    pub(super) fn bodies(&self) -> Vec<Body> {
        let t = self.world_time;
        std::iter::once(Body { center: Vec3::ZERO, radius: SUN_RADIUS, surface_gravity: SUN_GRAVITY, velocity: Vec3::ZERO })
            .chain(PLANETS.iter().map(|p| Body {
                center: scenario::planet_position(p, t),
                radius: p.radius,
                surface_gravity: p.gravity,
                velocity: scenario::planet_velocity(p, t),
            }))
            .collect()
    }

    /// El núcleo de cada agujero negro es una esfera para que la nave colisione
    pub(super) fn collide_holes(&mut self, dt: f32) {
        let mut golpes = Vec::new();
        for h in BLACK_HOLES.iter() {
            let radio = h.core_radius();
            if self.ship.position.distance(h.position) > radio + NAVE_RADIO + 1.0 {
                continue;
            }
            let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, h.position, Vec3::ZERO, radio, dt);
            if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                golpes.push(c);
            }
        }
        for c in golpes {
            self.registrar_choque("AGUJERO NEGRO", None, c);
        }
    }

    /// El ovni es una esfera sólida 
    pub(super) fn collide_ufos(&mut self, dt: f32) {
        let t = self.world_time;
        let mut golpes = Vec::new();
        for u in UFOS.iter() {
            let (centro, radio) = (scenario::ufo_position(u, t), u.size);
            if self.ship.position.distance(centro) > radio + NAVE_RADIO + 1.0 {
                continue;
            }
            let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, centro, scenario::ufo_velocity(u, t), radio, dt);
            if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                golpes.push(c);
            }
        }
        for c in golpes {
            self.registrar_choque("OVNI", None, c);
        }
    }

    /// Todo lo que atrae a la nave: el sol, los planetas, los agujeros negros y los ovnis
    pub(super) fn gravity_sources(&self) -> Vec<Body> {
        let t = self.world_time;
        let mut fuentes = self.bodies();
        fuentes.extend(BLACK_HOLES.iter().map(|h| Body { center: h.position, radius: h.radius, surface_gravity: h.gravity, velocity: Vec3::ZERO }));
        fuentes.extend(UFOS.iter().map(|u| Body { center: scenario::ufo_position(u, t), radius: u.size, surface_gravity: u.gravity, velocity: scenario::ufo_velocity(u, t) }));
        fuentes
    }

    /// Radio de la superficie del cuerpo `i` en la dirección del punto `p`: tiene en cuenta el
    /// relieve y que el planeta está girando.
    pub(super) fn surface_at(&self, i: usize, b: &Body, p: Vec3) -> f32 {
        if i == 0 {
            return SUN_RADIUS;
        }
        let planet = &PLANETS[i - 1];
        let local = scenario::planet_rotation(planet, self.world_time).transposed().transform_dir(p - b.center);
        scenario::surface_radius(planet, local)
    }

    pub(super) fn max_surface(i: usize) -> f32 {
        if i == 0 {
            SUN_RADIUS
        } else {
            scenario::max_radius(&PLANETS[i - 1])
        }
    }

    pub(super) fn resolve_collisions(&mut self, dt: f32) {
        // Primero los objetos y al final el cuerpo: así empujar la nave fuera de un árbol nunca la deja dentro del suelo.
        self.collide_props(dt);
        self.collide_meteors(dt);
        self.collide_moons(dt);
        self.collide_holes(dt);
        self.collide_ufos(dt);
        for (i, b) in self.bodies().iter().enumerate() {
            let surface = self.surface_at(i, b, self.ship.position);
            let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, b.center, b.velocity, surface, dt);
            if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                self.registrar_choque(body_name(i), None, c);
            }
        }
    }

    /// Choques contra los objetos de superficie (árboles, rocas, edificios...). Cada uno es una
    /// esfera a media altura y de radio la mitad de su tamaño; giran con el planeta.
    pub(super) fn collide_props(&mut self, dt: f32) {
        let t = self.world_time;
        let mut golpes = Vec::new();
        for (i, p) in PLANETS.iter().enumerate() {
            let pos = scenario::planet_position(p, t);
            // Los objetos miden pocas unidades: lejos de la superficie no hay nada que revisar.
            if self.ship.position.distance(pos) > scenario::max_radius(p) + PROPS_ALCANCE {
                continue;
            }
            let planeta = Mat4::translation(pos) * scenario::planet_rotation(p, t);
            let velocidad = scenario::planet_velocity(p, t);
            for x in &self.placements[i] {
                let centro = (planeta * x.local).transform_point3(Vec3::new(0.0, 0.5, 0.0));
                let radio = x.size * PROP_COLISION;
                let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, centro, velocidad, radio, dt);
                if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                    golpes.push((p.name, p.props[x.kind].model, c));
                }
            }
        }
        for (body, objeto, c) in golpes {
            self.registrar_choque(body, Some(objeto), c);
        }
    }

    /// Los meteoritos son esferas sólidas que se mueven: se choca contra ellos como contra un cuerpo.
    pub(super) fn collide_meteors(&mut self, dt: f32) {
        let t = self.world_time;
        let mut golpes = Vec::new();
        for p in PLANETS.iter() {
            let Some(m) = &p.meteors else { continue };
            for k in 0..m.count {
                let (centro, radio) = (scenario::meteor_position(p, m, k, t), scenario::meteor(m, k).size);
                if self.ship.position.distance(centro) > radio + NAVE_RADIO + 1.0 {
                    continue;
                }
                let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, centro, scenario::meteor_velocity(p, m, k, t), radio, dt);
                if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                    golpes.push((p.name, m.model, c));
                }
            }
        }
        for (body, objeto, c) in golpes {
            self.registrar_choque(body, Some(objeto), c);
        }
    }

    /// Las lunas son esferas sólidas que orbitan: la nave choca contra ellas como contra un cuerpo.
    pub(super) fn collide_moons(&mut self, dt: f32) {
        let t = self.world_time;
        let mut golpes = Vec::new();
        for p in PLANETS.iter() {
            let Some(m) = &p.moon else { continue };
            let centro = scenario::moon_position(p, m, t);
            if self.ship.position.distance(centro) > m.size + NAVE_RADIO + 1.0 {
                continue;
            }
            let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, centro, scenario::moon_velocity(p, m, t), m.size, dt);
            if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                golpes.push((p.name, m.model, c));
            }
        }
        for (body, objeto, c) in golpes {
            self.registrar_choque(body, Some(objeto), c);
        }
    }

    /// Los disparos sueltan chispas al chocar con algo
    pub(super) fn shots_vs_bodies(&mut self, dt: f32) {
        if self.shots.is_empty() {
            return;
        }
        let t = self.world_time;
        let mut solidos: Vec<(Vec3, f32, Vec3)> =
            self.bodies().iter().enumerate().map(|(i, b)| (b.center, Self::max_surface(i), b.velocity)).collect();
        for p in PLANETS.iter() {
            if let Some(m) = &p.moon {
                solidos.push((scenario::moon_position(p, m, t), m.size, scenario::moon_velocity(p, m, t)));
            }
        }
        let mut impactos = Vec::new();
        self.shots.retain(|s| {
            let (a, b) = s.tramo(dt);
            match solidos.iter().find(|(c, r, _)| shots::segment_hits_sphere(a, b, *c, *r)) {
                Some(&(c, _, vel)) => {
                    impactos.push((b, (b - c).normalize(), vel));
                    false
                }
                None => true,
            }
        });
        for (pos, normal, vel) in impactos {
            self.sparks.burst(pos, normal, vel, 4.0);
        }
    }

    pub(super) fn registrar_choque(&mut self, body: &'static str, object: Option<&'static str>, c: physics::Contact) {
        // Sin chispas contra el agujero negro ni el ovni: tienen sonido propio.
        if body != "AGUJERO NEGRO" && body != "OVNI" {
            self.sparks.burst(self.ship.position - c.normal * NAVE_RADIO, c.normal, c.body_velocity, c.impact_speed);
        }
        self.collisions.push(CollisionEvent {
            body,
            object,
            position: self.ship.position - c.normal * NAVE_RADIO,
            normal: c.normal,
            impact_speed: c.impact_speed,
        });
        if self.collisions.len() > 32 {
            self.collisions.remove(0); // nadie los está leyendo: no crecer sin límite
        }
        match body {
            "AGUJERO NEGRO" => self.derrotar(Causa::AgujeroNegro),
            "OVNI" => self.derrotar(Causa::Ovni),
            _ => {
                let nombre = object.unwrap_or(body);
                match self.danar(c.impact_speed) {
                    Some(dano) => self.notice(format!("COLISION: {nombre}  -{dano:.0}%"), 0xEF5350),
                    None => self.notice(format!("COLISION: {nombre}"), 0xEF5350),
                }
                return;
            }
        }
        self.notice(format!("COLISION: {}", object.unwrap_or(body)), 0xEF5350);
    }

    /// Un choque contra algo sólido quita vida según qué tan fuerte fue.
    pub(super) fn danar(&mut self, impact_speed: f32) -> Option<f32> {
        if self.derrota.is_some() || self.time < self.invulnerable_hasta {
            return None;
        }
        let dano = (impact_speed * DANO_POR_VELOCIDAD).clamp(DANO_MIN, DANO_MAX).min(self.vida);
        self.vida -= dano;
        self.invulnerable_hasta = self.time + INVULNERABLE_S;
        if self.vida <= 0.0 {
            self.derrotar(Causa::Destruida);
        }
        Some(dano)
    }

    /// La nave perdió. En plena carrera queda eliminada y en la sala o los resultados solo ve el cartel y reaparece en el mapa.
    pub(super) fn derrotar(&mut self, causa: Causa) {
        if self.derrota.is_some() {
            return;
        }
        let elimina = self.modo == Modo::Carrera && self.en_carrera && self.race.phase == Phase::Racing && self.finish_time.is_none() && !self.eliminado;
        self.derrota = Some(Derrota { causa, elimina });
        if self.modo == Modo::Ataque {
            self.guardar_resultado();
        }
        self.orbit = None;
        self.ship.velocity = Vec3::ZERO;
        self.vida = 0.0;
        if elimina {
            self.eliminado = true;
            self.muerte_time = Some(self.race.time);
            if !self.online {
                self.race.finish_local();
                self.last_phase = Phase::Results;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::*;

    /// Pone la nave pegada al núcleo del primer agujero negro, yendo hacia él, y deja correr un cuadro.
    fn caer_en_agujero(g: &mut Game) {
        let h = &BLACK_HOLES[0];
        poner(g, h.position + Vec3::new(h.core_radius() + NAVE_RADIO - 0.2, 0.0, 0.0));
        g.ship.velocity = Vec3::new(-30.0, 0.0, 0.0);
        g.update(0.016, &Input::default());
    }

    fn caer_en_ovni(g: &mut Game) {
        let u = &UFOS[0];
        let c = scenario::ufo_position(u, g.world_time);
        poner(g, c + Vec3::new(u.size + NAVE_RADIO - 0.2, 0.0, 0.0));
        g.ship.velocity = Vec3::new(-30.0, 0.0, 0.0);
        g.update(0.016, &Input::default());
    }

    #[test]
    fn en_la_sala_caer_en_un_agujero_negro_muestra_el_cartel_y_reaparece() {
        let mut g = jugando();
        assert_eq!(g.phase(), Phase::Lobby);
        caer_en_agujero(&mut g);
        let d = g.derrota.expect("perdió");
        assert_eq!((d.causa, d.elimina), (Causa::AgujeroNegro, false));
        assert_eq!(d.causa.mensaje(), "CAISTE EN UN AGUJERO NEGRO, YA NO PUEDES ESCAPAR");
        assert!(!g.eliminado);
        let quieta = g.ship.position;
        g.update(0.5, &Input { forward: true, ..Default::default() });
        assert!(g.ship.position.distance(quieta) < 1e-3, "con el cartel la nave no se mueve");

        avanzar(&mut g, 5.0);
        assert!(g.derrota.is_some(), "el cartel espera a que elijas");
        g.update(0.05, &Input { start: true, ..NADA });
        assert!(g.derrota.is_none());
        let (pos, _, _) = scenario::grid_slot(g.slot);
        assert!(g.ship.position.distance(pos) < 1.0, "reaparece en su casilla de salida");
        assert_eq!(g.phase(), Phase::Lobby);
    }

    #[test]
    fn caer_en_un_ovni_dice_que_los_aliens_te_raptaron() {
        let mut g = jugando();
        caer_en_ovni(&mut g);
        let d = g.derrota.expect("perdió");
        assert_eq!(d.causa, Causa::Ovni);
        assert_eq!(d.causa.mensaje(), "LOS ALIENS TE RAPTARON, YA NO PUEDES ESCAPAR");
    }

    #[test]
    fn en_plena_carrera_perder_termina_la_carrera_y_la_tabla_muestra_aros_y_tiempo() {
        let mut g = jugando();
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        assert_eq!(g.phase(), Phase::Racing);
        for i in 0..3 {
            g.ship.position = g.course[i].pos;
            g.update(0.05, &Input::default());
        }
        assert_eq!(g.progress(), 3);
        caer_en_agujero(&mut g);

        assert!(g.eliminado && g.derrota.is_some_and(|d| d.elimina));
        assert_eq!(g.phase(), Phase::Results, "offline la carrera termina");
        let e = &g.entries()[0];
        assert!(e.eliminado && e.progress == 3 && e.finish_ms > 0);

        // Con Enter vuelve a su casilla de salida, lejos del agujero.
        g.update(0.05, &Input { start: true, ..NADA });
        assert!(g.derrota.is_none() && g.eliminado);
        let (casilla, _, _) = scenario::grid_slot(g.slot);
        assert!(g.ship.position.distance(casilla) < 1.0);
        assert_eq!(g.black_hole_proximity(), 0.0, "ya no se oye el agujero negro");
        let antes = g.ship.position;
        avanzar_con(&mut g, 1.0, Input { forward: true, ..NADA });
        assert!(g.ship.position.distance(antes) > 5.0, "eliminada puede moverse");
        assert!(g.eliminado, "pero sigue contando como eliminada");
    }

    #[test]
    fn online_el_eliminado_espera_a_los_demas_y_avisa_con_el_bit_de_eliminado() {
        let mut g = jugando();
        g.set_online(true);
        g.set_race_server(Phase::Countdown as u8, 0);
        g.update(0.05, &Input::default());
        g.set_race_server(Phase::Racing as u8, 5000);
        g.update(0.05, &Input::default());
        g.progress = 4;
        caer_en_ovni(&mut g);

        assert!(g.eliminado);
        assert_eq!(g.phase(), Phase::Racing, "online solo el servidor termina la carrera");
        let s = g.local_state();
        assert_eq!(s.progress, 4 | ELIMINADO);
        assert!(s.finish_ms > 0);

        // Un rival eliminado se ve en la tabla con sus aros, sin contar como terminado.
        g.set_remote_players(&[RemoteShip { progress: 2 | ELIMINADO, finish_ms: 7000, ..rival(7, 0.0) }]);
        let rival_entry = g.entries().into_iter().find(|e| e.progress == 2).unwrap();
        assert!(rival_entry.eliminado);
    }

    #[test]
    fn la_siguiente_carrera_borra_la_derrota() {
        let mut g = jugando();
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        caer_en_agujero(&mut g);
        assert!(g.eliminado);
        g.update(0.05, &empezar()); // revancha
        assert!(!g.eliminado && g.derrota.is_none() && g.muerte_time.is_none());
        assert_eq!(g.local_state().progress, 0);
    }

    #[test]
    fn la_nave_no_atraviesa_el_nucleo_de_un_agujero_negro_y_avisa_el_choque() {
        let mut g = jugando();
        let h = &BLACK_HOLES[0];
        let radio = h.core_radius();
        poner(&mut g, h.position + Vec3::new(radio + NAVE_RADIO - 0.2, 0.0, 0.0));
        g.ship.velocity = Vec3::new(-30.0, 0.0, 0.0);
        g.collide_holes(0.016);
        assert!(g.ship.position.distance(h.position) >= radio + NAVE_RADIO - 1e-3);
        assert!(g.take_collisions().iter().any(|c| c.body == "AGUJERO NEGRO" && c.object.is_none()));
    }

    #[test]
    fn el_ovni_atrae_a_la_nave_pero_siempre_se_puede_escapar() {
        let mut g = jugando();
        for u in UFOS.iter() {
            assert!(u.gravity < crate::ship::EMPUJE);
            let c = scenario::ufo_position(u, g.world_time);
            // Solo el ovni cerca: en la superficie la atracción apunta hacia él.
            poner(&mut g, c + Vec3::new(u.size * 2.0, 0.0, 0.0));
            let ovnis: Vec<Body> = g.gravity_sources().into_iter().skip(1 + PLANETS.len() + BLACK_HOLES.len()).collect();
            let a = physics::gravity_at(g.ship.position, &ovnis);
            assert!(a.x < 0.0, "apunta hacia el ovni");
        }
    }

    #[test]
    fn el_ovni_patrulla_en_circulo_y_la_nave_no_lo_atraviesa() {
        let mut g = jugando();
        let u = &UFOS[0];
        assert_eq!(g.ufo_meshes.len(), UFOS.len());
        let (a, b) = (scenario::ufo_position(u, 0.0), scenario::ufo_position(u, 10.0));
        assert!(a != b, "se mueve");
        let horizontal = |p: Vec3| ((p.x - u.center.x).powi(2) + (p.z - u.center.z).powi(2)).sqrt();
        assert!((horizontal(a) - u.orbit).abs() < 1e-2 && (horizontal(b) - u.orbit).abs() < 1e-2, "vuelta circular");
        let t = g.world_time;
        let centro = scenario::ufo_position(u, t);
        poner(&mut g, centro + Vec3::new(u.size + NAVE_RADIO - 0.2, 0.0, 0.0));
        g.ship.velocity = scenario::ufo_velocity(u, t) - Vec3::new(30.0, 0.0, 0.0);
        g.collide_ufos(0.016);
        assert!(g.ship.position.distance(centro) >= u.size + NAVE_RADIO - 1e-3);
        assert!(g.take_collisions().iter().any(|c| c.body == "OVNI"));
    }

    #[test]
    fn los_agujeros_negros_atraen_a_la_nave_mas_fuerte_de_lo_que_ella_empuja() {
        let g = jugando();
        for h in BLACK_HOLES.iter() {
            let p = h.position + Vec3::new(h.radius * 1.5, 0.0, 0.0);
            let a = physics::gravity_at(p, &g.gravity_sources());
            assert!(a.length() > crate::ship::EMPUJE, "cerca del disco la nave no puede escapar empujando: {}", a.length());
            assert!(a.x < 0.0, "apunta hacia el agujero negro");
            // Lejos, la atracción es mucho menor (cae con el cuadrado de la distancia).
            let lejos = physics::gravity_at(h.position + Vec3::new(h.radius * 12.0, 0.0, 0.0), &g.gravity_sources());
            assert!(lejos.length() < crate::ship::EMPUJE);
        }
        // Los índices de los cuerpos con los que se choca no cambian.
        assert_eq!(g.bodies().len(), 1 + PLANETS.len());
        assert_eq!(g.gravity_sources().len(), 1 + PLANETS.len() + BLACK_HOLES.len() + UFOS.len());
    }

    #[test]
    fn la_gravedad_atrae_hacia_el_sol() {
        let mut g = jugando();
        poner(&mut g, Vec3::new(0.0, 100.0, 0.0));
        avanzar(&mut g, 1.0);
        assert!(g.ship.velocity.y < -1.0, "debe caer hacia el sol: {:?}", g.ship.velocity);
    }

    #[test]
    fn g_apaga_y_prende_la_gravedad() {
        let mut g = jugando();
        g.update(0.05, &Input { toggle_gravity: true, ..NADA });
        assert!(!g.gravity_enabled());
        poner(&mut g, Vec3::new(0.0, 100.0, 0.0));
        avanzar(&mut g, 1.0);
        assert!(g.ship.velocity.length() < 1e-3, "sin gravedad no se mueve");
        g.update(0.05, &Input { toggle_gravity: true, ..NADA });
        assert!(g.gravity_enabled());
    }

    #[test]
    fn en_el_menu_no_hay_gravedad_ni_se_cambia_nada() {
        let mut g = Game::new(1);
        let pos = g.ship.position;
        g.update(0.05, &Input { toggle_gravity: true, toggle_view: true, toggle_orbit: true, ..NADA });
        avanzar(&mut g, 1.0);
        assert!(g.ship.position.distance(pos) < 1e-3);
        assert!(g.gravity_enabled() && g.view_mode() == ViewMode::ThirdPerson && g.orbiting().is_none());
    }

    #[test]
    fn la_nave_no_atraviesa_el_sol_y_avisa_el_choque() {
        let mut g = jugando();
        poner(&mut g, Vec3::new(0.0, 60.0, 0.0));
        g.ship.velocity = Vec3::new(0.0, -80.0, 0.0);
        for _ in 0..40 {
            g.update(0.05, &Input::default());
            assert!(g.ship.position.length() >= SUN_RADIUS + NAVE_RADIO - 1e-3);
        }
        let choques = g.take_collisions();
        assert!(choques.iter().any(|c| c.body == "SOL" && c.impact_speed > 10.0), "{choques:?}");
        assert!(g.take_collisions().is_empty(), "se entregan una sola vez");
    }

    #[test]
    fn la_nave_no_atraviesa_un_objeto_de_superficie_y_avisa_el_choque() {
        let mut g = jugando();
        let i = PLANETS.iter().position(|p| p.name == "AQUA").unwrap();
        let p = &PLANETS[i];
        let x = g.placements[i][0];
        let t = g.world_time;
        let planeta = Mat4::translation(scenario::planet_position(p, t)) * scenario::planet_rotation(p, t);
        let centro = (planeta * x.local).transform_point3(Vec3::new(0.0, 0.5, 0.0));
        let radio = x.size * PROP_COLISION;
        let arriba = (planeta * x.local).transform_dir(Vec3::Y).normalize();
        poner(&mut g, centro + arriba * (radio + NAVE_RADIO - 0.2));
        g.ship.velocity = scenario::planet_velocity(p, t) - arriba * 20.0;
        g.collide_props(0.016);
        assert!(g.ship.position.distance(centro) >= radio + NAVE_RADIO - 1e-3, "la nave sale del objeto");
        let choques = g.take_collisions();
        let modelo = p.props[x.kind].model;
        assert!(choques.iter().any(|c| c.body == "AQUA" && c.object == Some(modelo)), "{choques:?}");
    }

    #[test]
    fn la_nave_no_atraviesa_un_meteorito_y_los_gaseosos_tienen_su_modelo() {
        let mut g = jugando();
        let i = PLANETS.iter().position(|p| p.meteors.is_some()).expect("algún planeta tiene meteoritos");
        let (p, m) = (&PLANETS[i], PLANETS[i].meteors.as_ref().unwrap());
        assert!(m.count > 1 && g.meteor_meshes[i].is_some());
        assert!(g.ring_meshes.iter().zip(PLANETS.iter()).all(|(mesh, p)| mesh.is_some() == p.rings.is_some()), "cada anillo carga su modelo");
        let t = g.world_time;
        let (centro, radio) = (scenario::meteor_position(p, m, 0, t), scenario::meteor(m, 0).size);
        poner(&mut g, centro + Vec3::new(radio + NAVE_RADIO - 0.2, 0.0, 0.0));
        g.ship.velocity = scenario::meteor_velocity(p, m, 0, t) - Vec3::new(20.0, 0.0, 0.0);
        g.collide_meteors(0.016);
        assert!(g.ship.position.distance(centro) >= radio + NAVE_RADIO - 1e-3);
        assert!(g.take_collisions().iter().any(|c| c.body == p.name && c.object == Some(m.model)));
    }

    #[test]
    fn los_meteoritos_orbitan_distinto_y_siempre_igual() {
        let p = PLANETS.iter().find(|p| p.meteors.is_some()).unwrap();
        let m = p.meteors.as_ref().unwrap();
        let (a, b) = (scenario::meteor(m, 0), scenario::meteor(m, 1));
        assert!(a.distance != b.distance && a.phase != b.phase);
        assert_eq!(scenario::meteor_position(p, m, 3, 7.0), scenario::meteor_position(p, m, 3, 7.0));
        assert!(scenario::meteor_position(p, m, 3, 0.0) != scenario::meteor_position(p, m, 3, 5.0), "se mueve");
        let d = scenario::meteor_position(p, m, 3, 5.0).distance(scenario::planet_position(p, 5.0));
        assert!((d - scenario::meteor(m, 3).distance).abs() < 1e-2, "órbita circular alrededor del planeta");
    }

    #[test]
    fn chocar_con_un_planeta_respeta_su_relieve_y_quedarse_apoyado_no_repite_avisos() {
        let mut g = jugando();
        let (centro, radio) = aqua(&g);
        poner(&mut g, centro + Vec3::new(0.0, radio * 1.6, 0.0));
        g.ship.velocity = Vec3::new(0.0, -60.0, 0.0);
        let i = 1 + PLANETS.iter().position(|p| p.name == "AQUA").unwrap();
        for _ in 0..80 {
            g.update(0.05, &Input::default());
            let b = g.bodies()[i];
            let superficie = g.surface_at(i, &b, g.ship.position);
            assert!(g.ship.position.distance(b.center) >= superficie + NAVE_RADIO - 0.05);
        }
        let choques = g.take_collisions();
        assert!(choques.iter().any(|c| c.body == "AQUA"));
        assert!(choques.len() <= 4, "apoyada no debe avisar en cada cuadro: {}", choques.len());
    }

    #[test]
    fn chocar_quita_vida_y_en_cero_la_nave_pierde_y_reaparece_con_vida_llena() {
        let mut g = jugando();
        let c = physics::Contact { normal: Vec3::Y, impact_speed: 20.0, body_velocity: Vec3::ZERO };
        g.registrar_choque("AQUA", None, c);
        assert_eq!(g.health(), VIDA_MAX - 10.0);
        g.registrar_choque("AQUA", None, c);
        assert_eq!(g.health(), VIDA_MAX - 10.0, "el mismo choque no descuenta dos veces");
        for _ in 0..20 {
            if g.derrota.is_some() {
                break;
            }
            g.time += INVULNERABLE_S;
            g.registrar_choque("AQUA", None, c);
        }
        assert_eq!(g.derrota.map(|d| d.causa), Some(Causa::Destruida));
        assert_eq!(g.local_state().health, 0, "los demás ven la barra vacía");
        g.update(0.05, &Input { start: true, ..NADA });
        assert!(g.derrota.is_none());
        assert_eq!(g.health(), VIDA_MAX, "reaparece con la vida llena");
    }
}
