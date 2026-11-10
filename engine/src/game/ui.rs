//! Interfaz: HUD, vista de derrota, menú de inicio y minimapa.
use super::*;

impl Game {
    pub(super) fn entries(&self) -> Vec<Entry> {
        let mut list = vec![Entry { label: nombre_mostrado(&self.name, "TU".into()), color: self.my_id.map_or(0xFFFFFF, color_jugador), progress: self.progress, finish_ms: self.finish_ms(), eliminado: self.eliminado }];
        for r in &self.remotes {
            list.push(Entry {
                label: nombre_mostrado(&r.shown.name, format!("J{}", r.shown.id)),
                color: COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()],
                progress: (r.target.progress & !ELIMINADO) as usize,
                finish_ms: r.target.finish_ms,
                eliminado: r.target.progress & ELIMINADO != 0,
            });
        }
        // Primero los que terminaron (por tiempo), luego por puntos de control; los eliminados cuentan como no terminados.
        list.sort_by_key(|e| {
            let termino = e.finish_ms > 0 && !e.eliminado;
            (!termino, if termino { e.finish_ms } else { 0 }, usize::MAX - e.progress)
        });
        list
    }

    pub(super) fn draw_hud(&self, fb: &mut Framebuffer, v: &View, st: &RenderStats) {
        let s = (fb.height / 270).max(1);
        let m = 6 * s as i32;
        let (w, h) = (fb.width as i32, fb.height as i32);
        let line = 10 * s as i32;
        let white = 0xFFFFFF;

        // Nombres de planetas
        let labels = std::iter::once(("SOL", Vec3::ZERO, SUN_RADIUS, 0xFFC107)).chain(
            PLANETS.iter().map(|p| (p.name, scenario::planet_position(p, self.world_time), p.radius, 0xB0BEC5)),
        );
        for (name, pos, radius, color) in labels {
            let dist = pos.distance(v.eye);
            if let Some((x, y, depth)) = raster::project(&v.vp, pos, fb.width, fb.height) {
                let r = (radius * v.focal / depth) as i32;
                // Si el planeta llena media pantalla ya se sabe cuál es: el nombre solo estorbaría.
                // Si otro cuerpo lo tapa, tampoco se muestra.
                let en_pantalla = x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32;
                if dist < 1500.0 && r < h / 4 && en_pantalla && !tapado(fb, &v.vp, v.eye, pos, radius, x, y) {
                    hud::centered(fb, x as i32, (y as i32 - r - 2 * line).max(m), s, color, name);
                    hud::centered(fb, x as i32, (y as i32 - r - line).max(m + line), s, 0x78909C, &format!("{dist:.0} M"));
                }
            }
        }

        // Nombre de cada rival sobre su nave, con el color con el que sale en la tabla de carrera.
        for r in &self.remotes {
            let arriba = r.shown.position + Vec3::new(0.0, NOMBRE_ALTURA, 0.0);
            if r.shown.position.distance(v.eye) > NOMBRE_DISTANCIA {
                continue;
            }
            if let Some((x, y, _)) = raster::project(&v.vp, arriba, fb.width, fb.height) {
                let en_pantalla = x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32;
                if en_pantalla && !tapado(fb, &v.vp, v.eye, arriba, 0.0, x, y) {
                    let nombre = nombre_mostrado(&r.shown.name, format!("J{}", r.shown.id));
                    hud::centered(fb, x as i32, y as i32 - 8 * s as i32, s, color_jugador(r.shown.id), &nombre);
                }
            }
        }

        // Datos de vuelo y de render
        hud::label(fb, m, m, s, white, &format!("VEL {:.0} U/S", self.ship.speed()));
        hud::label(fb, m, m + line, s, 0x90A4AE, &format!("TRIANGULOS {}", st.triangles));
        hud::label(fb, m, m + 2 * line, s, 0x90A4AE, &format!("OBJETOS {}/{}", st.visible, st.total));
        let red = if self.online { format!("ONLINE {} JUGADORES", 1 + self.remotes.len()) } else { "SIN CONEXION".to_string() };
        hud::label(fb, m, m + 3 * line, s, if self.online { 0x66BB6A } else { 0xEF9A9A }, &red);
        let vista = match self.view {
            ViewMode::ThirdPerson => "3A PERSONA",
            ViewMode::FirstPerson => "1A PERSONA",
        };
        let mut filas = vec![
            (format!("GRAVEDAD {} (G)", if self.gravity_on { "SI" } else { "NO" }), 0x90A4AE),
            (format!("VISTA {vista} (V)"), 0x90A4AE),
            (format!("SEMILLA {} (R)", self.props_seed), 0x90A4AE),
        ];
        if let Some(o) = &self.orbit {
            filas.push((format!("ORBITA {} (O SALIR)", body_name(o.body)), 0x66BB6A));
        }
        if let Some((texto, color, hasta)) = &self.notice {
            if self.time < *hasta {
                filas.push((texto.clone(), *color));
            }
        }
        for (i, (texto, color)) in filas.iter().enumerate() {
            hud::label(fb, m, m + (4 + i as i32) * line, s, *color, texto);
        }

        if self.modo == Modo::Persecucion {
            self.draw_persecucion(fb, v, s, m, line);
            self.draw_minimap(fb, s as i32, m);
            self.draw_derrota(fb, s);
            return;
        }

        // Carrera
        let cx = w / 2;
        match self.race.phase {
            Phase::Lobby => banner(fb, s, "SALA DE ESPERA", "ENTER EMPEZAR CARRERA   M MENU"),
            Phase::Countdown => {
                let n = self.race.countdown_number().unwrap_or(1).to_string();
                hud::centered(fb, cx, h / 3, s * 10, COLOR_SIGUIENTE, &n);
                hud::centered(fb, cx, h / 3 + 90 * s as i32, s * 2, white, "PREPARATE");
            }
            Phase::Racing => {
                hud::centered(fb, cx, m, s * 2, white, &fmt_time(self.finish_time.or(self.muerte_time).unwrap_or(self.race.time)));
                let estado = match self.finish_time {
                    _ if self.eliminado => "ELIMINADO! ESPERANDO A LOS DEMAS".to_string(),
                    Some(_) => "TERMINASTE! ESPERANDO A LOS DEMAS".to_string(),
                    None => format!("PUNTO {}/{}", self.progress + 1, GATE_COUNT),
                };
                hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, white, &estado);
            }
            Phase::Results => banner(fb, s, "CARRERA TERMINADA", "ENTER REVANCHA   M MENU"),
        }

        // Marcador del siguiente punto de control (en pantalla o en el borde)
        if self.race.phase == Phase::Racing && self.progress < GATE_COUNT && !self.eliminado {
            let g = self.course[self.progress].pos;
            marcador(fb, v, s, m, g, g.distance(self.ship.position), COLOR_SIGUIENTE);
        }

        // Posiciones
        let entries = self.entries();
        let base = h - m - line * (entries.len() as i32 + 1);
        let en_sala = self.race.phase == Phase::Lobby;
        hud::label(fb, m, base - line, s, 0x90A4AE, if en_sala { "EN LA SALA" } else { "POSICIONES" });
        for (i, e) in entries.iter().enumerate() {
            let texto = if en_sala {
                e.label.clone()
            } else {
                let tiempo = fmt_time(e.finish_ms as f32 / 1000.0);
                let estado = if e.eliminado {
                    format!("{}/{} AROS {}", e.progress, GATE_COUNT, tiempo)
                } else if e.finish_ms > 0 {
                    tiempo
                } else {
                    format!("{}/{}", e.progress, GATE_COUNT)
                };
                format!("{} {} {}", i + 1, e.label, estado)
            };
            hud::label(fb, m, base + i as i32 * line, s, e.color, &texto);
        }

        self.draw_minimap(fb, s as i32, m);
        self.draw_derrota(fb, s);
    }

    /// HUD del modo persecución: tiempo sobrevivido, mejor tiempo, ovnis y una flecha hacia cada uno.
    pub(super) fn draw_persecucion(&self, fb: &mut Framebuffer, v: &View, s: usize, m: i32, line: i32) {
        let (w, h) = (fb.width as i32, fb.height as i32);
        let cx = w / 2;
        let p = &self.persecucion;
        hud::centered(fb, cx, m, s * 2, 0xFFFFFF, &fmt_time(p.tiempo));
        let mejor = self.mejor_persecucion.map_or("MEJOR --".to_string(), |t| format!("MEJOR {}", fmt_time(t)));
        hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, 0x90A4AE, &mejor);
        let siguiente = match p.proximo_en() {
            Some(t) if !p.en_gracia() => format!("   OTRO EN {:.0}", t.ceil()),
            _ => String::new(),
        };
        hud::centered(fb, cx, m + 3 * line + 4 * s as i32, s, 0xEF9A9A, &format!("OVNIS {}{}", p.ovnis.len(), siguiente));

        if self.derrota.is_none() {
            let dist = p.distancia_minima(self.ship.position).unwrap_or(f32::MAX);
            if p.en_gracia() {
                let n = p.segundos_de_gracia().ceil().max(1.0);
                // Arriba, para no tapar al ovni, que ahora aparece de frente.
                hud::centered(fb, cx, m + 5 * line + 8 * s as i32, s * 3, 0xEF5350, "HUYE!");
                hud::centered(fb, cx, m + 5 * line + 8 * s as i32 + 30 * s as i32, s, 0xFFFFFF, &format!("EL OVNI SALE EN {n:.0}"));
            } else if dist < chase::CAZADOR_PELIGRO && (self.time * 4.0) as i32 % 2 == 0 {
                hud::centered(fb, cx, m + 5 * line + 8 * s as i32, s * 2, 0xEF5350, "PELIGRO");
            }
            for o in &p.ovnis {
                marcador(fb, v, s, m, o.position, o.position.distance(self.ship.position), 0xEF5350);
            }
        }
        hud::label(fb, m, h - m - line, s, 0x78909C, "ENTER REINICIAR   M MENU");
    }

    /// Vista de "perdiste" con la causa y qué pasa después.
    pub(super) fn draw_derrota(&self, fb: &mut Framebuffer, s: usize) {
        let Some(d) = &self.derrota else { return };
        let si = s as i32;
        let (cx, h) = (fb.width as i32 / 2, fb.height as i32);
        hud::shade_rect(fb, 0, 0, fb.width as i32, h, 0x000000, 130);
        hud::centered(fb, cx, h / 3, s * 4, 0xEF5350, "PERDISTE");
        hud::centered(fb, cx, h / 3 + 40 * si, s, 0xFFFFFF, d.causa.mensaje());
        let pie = if self.modo == Modo::Persecucion {
            let mejor = self.mejor_persecucion.map_or(String::new(), |t| format!("   MEJOR {}", fmt_time(t)));
            format!("SOBREVIVISTE {}{}", fmt_time(self.persecucion.tiempo), mejor)
        } else if d.elimina {
            let t = self.muerte_time.unwrap_or(self.race.time);
            format!("PASASTE {}/{} AROS EN {}", self.progress, GATE_COUNT, fmt_time(t))
        } else {
            "VOLVIENDO AL MAPA...".to_string()
        };
        let pie = if self.modo == Modo::Persecucion { format!("{pie}   REINICIANDO...") } else { pie };
        hud::centered(fb, cx, h / 3 + 60 * si, s, 0x90A4AE, &pie);
    }

    /// Pantalla de inicio: título, vista previa de la nave, controles y estado de la sala.
    pub(super) fn draw_menu(&self, fb: &mut Framebuffer) {
        let s = (fb.height / 270).max(1);
        let si = s as i32;
        let (m, pad, line) = (6 * si, 6 * si, 10 * si);
        let (w, h) = (fb.width as i32, fb.height as i32);
        let cx = w / 2;
        let (oro, gris, blanco) = (COLOR_SIGUIENTE, 0x90A4AE, 0xFFFFFF);

        hud::centered(fb, cx, h / 14, s * 4, oro, "SPACE TRAVEL");
        hud::centered(fb, cx, h / 14 + 40 * si, s, 0xB0BEC5, "CARRERA POR EL SISTEMA SOLAR");

        // Selector de nave bajo la vista previa
        let nombre: String = self.models[self.selected].name.to_uppercase().chars().take(18).collect();
        hud::centered(fb, cx, h * 7 / 10, s * 2, blanco, &format!("< {nombre} >"));
        let pista = format!("NAVE {}/{}  A D TAB", self.selected + 1, self.models.len());
        hud::centered(fb, cx, h * 7 / 10 + 20 * si, s, 0x78909C, &pista);

        // Controles (izquierda)
        let controles = [
            ("W S", "ACELERAR Y REVERSA"),
            ("A D", "GIRAR"),
            ("FLECHAS", "INCLINAR LA NARIZ"),
            ("ESPACIO", "FRENAR"),
            ("ENTER", "EMPEZAR CARRERA"),
            ("P", "PERSECUCION (SOLO)"),
            ("TAB", "CAMBIAR DE NAVE"),
            ("M", "VOLVER AL MENU"),
            ("G", "GRAVEDAD SI O NO"),
            ("O", "ORBITAR PLANETA"),
            ("V", "PRIMERA PERSONA"),
            ("R", "NUEVA SEMILLA"),
            ("F", "PANTALLA COMPLETA"),
        ];
        let col = 9;
        let cols = controles.iter().map(|(_, a)| col + a.len()).max().unwrap_or(col);
        let (pw, ph) = (cols as i32 * 6 * si + 2 * pad, (controles.len() as i32 + 1) * line + 2 * pad);
        let py = h * 13 / 50;
        hud::panel(fb, m, py, pw, ph);
        hud::label(fb, m + pad, py + pad, s, gris, "CONTROLES");
        for (i, (tecla, accion)) in controles.iter().enumerate() {
            let y = py + pad + (i as i32 + 1) * line;
            hud::label(fb, m + pad, y, s, oro, tecla);
            hud::label(fb, m + pad + col as i32 * 6 * si, y, s, blanco, accion);
        }

        // Sala (derecha)
        let mut filas: Vec<(String, u32)> = Vec::new();
        if self.online {
            filas.push(("ONLINE".into(), 0x66BB6A));
            filas.push((format!("{} JUGADORES", 1 + self.remotes.len()), blanco));
            filas.push((nombre_mostrado(&self.name, "TU".into()), self.my_id.map_or(blanco, color_jugador)));
            for r in &self.remotes {
                filas.push((nombre_mostrado(&r.shown.name, format!("J{}", r.shown.id)), COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()]));
            }
        } else {
            filas.push(("SIN CONEXION".into(), 0xEF9A9A));
            filas.push(("JUEGAS SOLO".into(), blanco));
        }
        let pie = ["CUALQUIERA PUEDE", "EMPEZAR LA CARRERA"];
        let ancho = filas.iter().map(|(t, _)| t.len()).chain(pie.iter().map(|t| t.len())).max().unwrap_or(10).max(14);
        let pw2 = ancho as i32 * 6 * si + 2 * pad;
        let ph2 = (filas.len() as i32 + pie.len() as i32 + 3) * line + 2 * pad;
        let px2 = w - m - pw2;
        hud::panel(fb, px2, py, pw2, ph2);
        hud::label(fb, px2 + pad, py + pad, s, gris, "SALA");
        for (i, (texto, color)) in filas.iter().enumerate() {
            hud::label(fb, px2 + pad, py + pad + (i as i32 + 1) * line, s, *color, texto);
        }
        for (i, t) in pie.iter().enumerate() {
            hud::label(fb, px2 + pad, py + pad + (filas.len() as i32 + 2 + i as i32) * line, s, 0x78909C, t);
        }

        // Invitación a jugar
        let color = if (self.time * 2.0) as i32 % 2 == 0 { oro } else { 0xFFF59D };
        hud::centered(fb, cx, h - m - 40 * si, s * 2, color, "ENTER = CARRERA");
        if self.online {
            hud::centered(fb, cx, h - m - 16 * si, s, 0x78909C, "PERSECUCION SOLO SIN CONEXION");
        } else {
            hud::centered(fb, cx, h - m - 20 * si, s * 2, 0xEF9A9A, "P = PERSECUCION");
        }
    }

    /// Mapa visto desde arriba: sol, órbitas, planetas, circuito y jugadores.
    pub(super) fn draw_minimap(&self, fb: &mut Framebuffer, s: i32, m: i32) {
        const ALCANCE: f32 = 580.0;
        let size = MINIMAPA_MITAD * s;
        let (cx, cy) = (fb.width as i32 - m - size, m + size);
        hud::panel(fb, cx - size - 1, cy - size - 1, 2 * size + 2, 2 * size + 2);
        let to_map = |p: Vec3| (cx + (p.x / ALCANCE * size as f32) as i32, cy + (p.z / ALCANCE * size as f32) as i32);
        let en_mapa = |x: i32, y: i32| (x - cx).abs() < size && (y - cy).abs() < size;

        for pl in &PLANETS {
            for i in 0..90 {
                let a = i as f32 / 90.0 * 2.0 * PI;
                let (x, y) = to_map(Vec3::new(a.cos() * pl.orbit_radius, 0.0, a.sin() * pl.orbit_radius));
                if en_mapa(x, y) {
                    hud::rect(fb, x, y, 1, 1, 0x263238);
                }
            }
        }
        hud::dot(fb, cx, cy, (SUN_RADIUS / ALCANCE * size as f32) as i32 + 2, Material::Sun.tint());
        for pl in &PLANETS {
            let (x, y) = to_map(scenario::planet_position(pl, self.world_time));
            hud::dot(fb, x, y, ((pl.radius / ALCANCE * size as f32) as i32).max(s), pl.material.tint());
        }
        let racing = self.race.phase == Phase::Racing;
        let aros = if self.modo == Modo::Persecucion { 0 } else { self.course.len() };
        for (i, g) in self.course.iter().enumerate().take(aros) {
            let (x, y) = to_map(g.pos);
            let (color, r) = if racing && i == self.progress { (COLOR_SIGUIENTE, s) } else if i < self.progress && racing { (0x2E7D32, 1) } else { (COLOR_PUNTO, 1) };
            hud::rect(fb, x - r, y - r, 2 * r + 1, 2 * r + 1, color);
        }
        for r in &self.remotes {
            let (x, y) = to_map(r.shown.position);
            if en_mapa(x, y) {
                hud::dot(fb, x, y, s + 1, COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()]);
            }
        }
        if self.modo == Modo::Persecucion {
            for o in &self.persecucion.ovnis {
                let (x, y) = to_map(o.position);
                if en_mapa(x, y) {
                    hud::dot(fb, x, y, s + 2, 0xEF5350);
                }
            }
        }
        // Mi nave: punto blanco con una línea hacia donde mira.
        let (x, y) = to_map(self.ship.position);
        let f = self.ship.forward();
        let (fx, fz) = (f.x, f.z);
        for i in 0..(5 * s) {
            hud::rect(fb, x + (fx * i as f32) as i32, y + (fz * i as f32) as i32, 1, 1, 0xFFFFFF);
        }
        hud::dot(fb, x, y, s + 1, 0xFFFFFF);
    }
}

fn marcador(fb: &mut Framebuffer, v: &View, s: usize, m: i32, objetivo: Vec3, dist: f32, color: u32) {
    let (w, h) = (fb.width as i32, fb.height as i32);
    let line = 10 * s as i32;
    let (cxf, cyf) = (w as f32 / 2.0, h as f32 / 2.0);
    let (mut x, mut y, dentro) = match raster::project(&v.vp, objetivo, fb.width, fb.height) {
        Some((x, y, _)) if x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32 => (x, y, true),
        Some((x, y, _)) => (x, y, false),
        None => {
            let c = v.vp.transform_point(objetivo);
            (cxf - c.x.signum() * cxf * 2.0, cyf + c.y.signum() * cyf * 2.0, false)
        }
    };
    if !dentro {
        let (dx, dy) = (x - cxf, y - cyf);
        let (mx, my) = (4.0 * m as f32, 5.0 * m as f32 + 2.0 * line as f32);
        let k = ((cxf - mx) / dx.abs().max(1e-3)).min((cyf - my) / dy.abs().max(1e-3));
        x = cxf + dx * k;
        y = cyf + dy * k;
    }
    let b = 5 * s as i32;
    for d in 0..b {
        hud::rect(fb, x as i32 - d, y as i32 - b + d, 2 * d + 1, 1, color);
        hud::rect(fb, x as i32 - d, y as i32 + b - d, 2 * d + 1, 1, color);
    }
    hud::centered(fb, x as i32, y as i32 + b + 3 * s as i32, s, color, &format!("{dist:.0} M"));
}

/// ¿Hay algo dibujado delante de la esfera (`pos`, `radius`) en el píxel (x, y)?
/// Compara el z-buffer con la profundidad del punto de la esfera más cercano a la cámara.
pub(super) fn tapado(fb: &Framebuffer, vp: &Mat4, eye: Vec3, pos: Vec3, radius: f32, x: f32, y: f32) -> bool {
    let frente = pos + (eye - pos).normalize() * radius;
    let c = vp.transform_point(frente);
    if c.w <= 1e-4 {
        return false;
    }
    let idx = y as usize * fb.width + x as usize;
    fb.depth.get(idx).is_some_and(|&d| d < c.z / c.w - 1e-4)
}

/// Cartel centrado arriba con un título y una pista de teclas.
pub(super) fn banner(fb: &mut Framebuffer, s: usize, titulo: &str, pista: &str) {
    let si = s as i32;
    let pad = 6 * si;
    let w = hud::width(titulo, s * 2).max(hud::width(pista, s)) + 2 * pad;
    let (x, y) = (fb.width as i32 / 2 - w / 2, 6 * si);
    hud::panel(fb, x, y, w, 25 * si + 2 * pad);
    hud::centered(fb, fb.width as i32 / 2, y + pad, s * 2, COLOR_SIGUIENTE, titulo);
    hud::centered(fb, fb.width as i32 / 2, y + pad + 18 * si, s, 0xFFFFFF, pista);
}

pub(super) fn fmt_time(secs: f32) -> String {
    let cs = (secs * 100.0) as u32;
    format!("{}:{:02}.{:02}", cs / 6000, cs / 100 % 60, cs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::*;

    #[test]
    fn los_eliminados_quedan_detras_de_los_que_terminaron() {
        let mut g = jugando();
        g.set_online(true);
        g.set_race_server(Phase::Results as u8, 20_000);
        g.update(0.05, &Input::default());
        g.set_remote_players(&[
            RemoteShip { progress: 1 | ELIMINADO, finish_ms: 2000, ..rival(1, 0.0) },
            RemoteShip { progress: GATE_COUNT as u8, finish_ms: 30_000, ..rival(2, 0.0) },
        ]);
        let lista = g.entries();
        assert_eq!(lista[0].finish_ms, 30_000, "el que terminó va primero aunque sea más lento");
        assert!(lista.last().is_some());
    }

    #[test]
    fn el_nombre_del_rival_se_dibuja_sobre_su_nave_y_cambia_con_el_nombre() {
        let nombre = |t: &[u8]| {
            let mut n = [0; NAME_LEN];
            n[..t.len()].copy_from_slice(t);
            n
        };
        let dibujar = |t: &[u8]| {
            let mut g = jugando();
            let pos = g.ship.position + g.ship.forward() * 30.0;
            g.set_remote_players(&[RemoteShip { position: pos, name: nombre(t), ..rival(3, 0.0) }]);
            let mut fb = Framebuffer::new(640, 480);
            g.render(&mut fb);
            fb.pixels
        };
        // Misma nave en el mismo lugar: solo cambia el texto, así que la imagen cambia solo por el nombre.
        assert_ne!(dibujar(b"ZORRO"), dibujar(b"LUNA"));
    }

    #[test]
    fn el_ranking_pone_primero_al_que_termino() {
        let mut g = Game::new(1);
        g.set_remote_players(&[
            RemoteShip { progress: 10, finish_ms: 40_000, ..rival(1, 0.0) },
            RemoteShip { progress: 6, ..rival(2, 0.0) },
        ]);
        g.progress = 8;
        let orden: Vec<String> = g.entries().into_iter().map(|e| e.label).collect();
        assert_eq!(orden, ["J1", "TU", "J2"]);
    }

    #[test]
    fn el_hud_se_dibuja_en_cualquier_resolucion_sin_paniquear() {
        let g = Game::new(1);
        let en_juego = jugando();
        for (w, h) in [(160, 90), (800, 600), (1920, 1080), (100, 1000)] {
            g.render(&mut Framebuffer::new(w, h)); // menú
            en_juego.render(&mut Framebuffer::new(w, h));
        }
    }

    #[test]
    fn el_nombre_de_un_planeta_tapado_no_se_muestra() {
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let vp = Mat4::perspective(1.0, 1.0, 0.5, 1000.0) * Mat4::look_at(eye, Vec3::ZERO, Vec3::Y);
        let mut fb = Framebuffer::new(64, 64);
        let lejos = Vec3::new(0.0, 0.0, -100.0);
        assert!(!tapado(&fb, &vp, eye, lejos, 5.0, 32.0, 32.0), "sin nada delante se ve");
        // Un objeto cercano dibujado en ese píxel (profundidad menor) lo tapa.
        let cerca = vp.transform_point(Vec3::ZERO);
        fb.depth[32 * 64 + 32] = cerca.z / cerca.w;
        assert!(tapado(&fb, &vp, eye, lejos, 5.0, 32.0, 32.0));
        // Su propia superficie no lo tapa a sí mismo.
        let propia = vp.transform_point(Vec3::new(0.0, 0.0, -95.0));
        fb.depth[32 * 64 + 32] = propia.z / propia.w;
        assert!(!tapado(&fb, &vp, eye, lejos, 5.0, 32.0, 32.0));
    }
}
