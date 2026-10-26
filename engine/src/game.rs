use crate::{
    assets,
    camera::ChaseCamera,
    frustum::Frustum,
    hud,
    input::Input,
    obj, procedural,
    race::{Phase, Race},
    raster::{self, draw_mesh},
    remote::{RemotePlayer, RemoteShip},
    rng::Rng,
    scenario::{self, Gate, GATE_COUNT, PLANETS, SUN_RADIUS},
    shaders::{Material, Scene},
    Framebuffer, Mat4, Mesh, Ship, Vec3,
};
use std::f32::consts::PI;

const FOV_Y: f32 = 60.0 * PI / 180.0;
const FONDO: u32 = 0x02020A;
const DT_MAX: f32 = 0.05; // evita saltos al volver de una pestaña inactiva
const COLOR_PROPIO: u32 = 0xDDDDDD;
const COLORES_JUGADORES: [u32; 6] = [0x42A5F5, 0xEF5350, 0x66BB6A, 0xFFCA28, 0xAB47BC, 0x26C6DA];
const COLOR_SIGUIENTE: u32 = 0xFFD54F;
const COLOR_PUNTO: u32 = 0x42A5F5;
/// Subdivisiones de la esfera, de menos a más detalle (LOD): 80, 320, 1280 y 5120 triángulos.
const LOD_NIVELES: [u32; 4] = [1, 2, 3, 4];

struct Model {
    name: String,
    mesh: Mesh,
}

struct Star {
    dir: Vec3,
    color: u32,
}

/// Contadores del último cuadro, para el HUD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub triangles: usize,
    pub visible: usize,
    pub total: usize,
}

struct View<'a> {
    vp: Mat4,
    frustum: &'a Frustum,
    scene: Scene,
    /// Píxeles por unidad a distancia 1; sirve para elegir el LOD.
    focal: f32,
}

struct Entry {
    label: String,
    color: u32,
    progress: usize,
    finish_ms: u32,
}

/// Estado del juego. Native y web llaman a la misma lógica: `update(dt, input)` y `render(fb)`.
pub struct Game {
    /// Segundos acumulados; todo movimiento depende de esto, no de los cuadros.
    pub time: f32,
    /// Reloj del mundo: de él dependen las órbitas. Online se sincroniza con el del servidor.
    world_time: f64,
    ship: Ship,
    camera: ChaseCamera,
    models: Vec<Model>,
    selected: usize,
    spheres: Vec<Mesh>,
    gate_mesh: Mesh,
    course: Vec<Gate>,
    stars: Vec<Star>,
    remotes: Vec<RemotePlayer>,
    online: bool,
    race: Race,
    last_phase: Phase,
    progress: usize,
    finish_time: Option<f32>,
    slot: usize,
    start_request: bool,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut models: Vec<Model> = assets::MODELS
            .iter()
            .map(|(name, bytes)| match obj::parse(bytes) {
                Ok(m) => Model { name: name.to_string(), mesh: m.normalized() },
                Err(e) => Model { name: format!("{name} (ERROR: {e})"), mesh: Mesh::cube().normalized() },
            })
            .collect();
        if models.is_empty() {
            models.push(Model { name: "cubo (sin modelos)".into(), mesh: Mesh::cube().normalized() });
        }
        let selected = models.iter().position(|m| m.name == "nave").unwrap_or(0);

        let mut rng = Rng::new(seed);
        let stars = (0..400)
            .map(|_| {
                let (z, a) = (rng.range(-1.0, 1.0), rng.range(0.0, 2.0 * PI));
                let r = (1.0 - z * z).sqrt();
                let g = rng.range(90.0, 255.0) as u32;
                Star { dir: Vec3::new(r * a.cos(), z, r * a.sin()), color: (g << 16) | (g << 8) | g }
            })
            .collect();

        let mut game = Self {
            time: 0.0,
            world_time: 0.0,
            ship: Ship::new(),
            camera: ChaseCamera::new(&Ship::new()),
            models,
            selected,
            spheres: LOD_NIVELES.iter().map(|&n| procedural::icosphere(n)).collect(),
            gate_mesh: procedural::torus(scenario::GATE_RADIUS, 1.0, 28, 8),
            course: scenario::course(),
            stars,
            remotes: Vec::new(),
            online: false,
            race: Race::new(),
            last_phase: Phase::Lobby,
            progress: 0,
            finish_time: None,
            slot: 0,
            start_request: false,
        };
        game.place_on_grid();
        game
    }

    /// Coloca la nave en su casilla de salida, en reposo.
    pub fn place_on_grid(&mut self) {
        let (pos, yaw, pitch) = scenario::grid_slot(self.slot);
        self.ship.reset(pos, yaw, pitch);
        self.camera = ChaseCamera::new(&self.ship);
    }

    pub fn update(&mut self, dt: f32, input: &Input) {
        let dt = dt.clamp(0.0, DT_MAX);
        self.time += dt;
        self.world_time += dt as f64;
        if input.cycle_model {
            self.selected = (self.selected + 1) % self.models.len();
        }
        if input.start {
            if self.online {
                self.start_request = true;
            } else {
                self.race.start_local();
            }
        }

        self.race.update(dt, self.online);
        if self.race.phase != self.last_phase {
            if self.race.phase == Phase::Countdown {
                self.progress = 0;
                self.finish_time = None;
                self.place_on_grid();
            }
            self.last_phase = self.race.phase;
        }

        // En la cuenta regresiva la nave espera en la parrilla.
        let ctrl = if self.race.phase == Phase::Countdown { Input::default() } else { *input };
        self.ship.update(&ctrl, dt);
        self.camera.update(&self.ship, dt);
        self.check_gates();
        for r in &mut self.remotes {
            r.update(dt);
        }
    }

    fn check_gates(&mut self) {
        if self.race.phase != Phase::Racing || self.progress >= GATE_COUNT {
            return;
        }
        let gate = self.course[self.progress];
        if self.ship.position.distance(gate.pos) < gate.radius {
            self.progress += 1;
            if self.progress == GATE_COUNT {
                self.finish_time = Some(self.race.time);
                if !self.online {
                    self.race.finish_local();
                    self.last_phase = Phase::Results;
                }
            }
        }
    }

    /// Mi nave tal como la ven los demás (el id lo pone el servidor).
    pub fn local_state(&self) -> RemoteShip {
        RemoteShip {
            id: 0,
            position: self.ship.position,
            yaw: self.ship.yaw,
            pitch: self.ship.pitch,
            model: self.selected as u8,
            progress: self.progress as u8,
            finish_ms: self.finish_ms(),
        }
    }

    fn finish_ms(&self) -> u32 {
        self.finish_time.map_or(0, |t| ((t * 1000.0) as u32).max(1))
    }

    /// Reemplaza la lista de otros jugadores; los que ya no están desaparecen.
    pub fn set_remote_players(&mut self, list: &[RemoteShip]) {
        self.remotes.retain(|r| list.iter().any(|s| s.id == r.target.id));
        for s in list {
            match self.remotes.iter_mut().find(|r| r.target.id == s.id) {
                Some(r) => r.target = *s,
                None => self.remotes.push(RemotePlayer::new(*s)),
            }
        }
    }

    pub fn set_online(&mut self, online: bool) {
        self.online = online;
        if !online {
            self.remotes.clear();
            self.race.reset();
            self.last_phase = Phase::Lobby;
            self.slot = 0;
        }
    }

    pub fn remote_count(&self) -> usize {
        self.remotes.len()
    }

    /// Casilla de salida de este jugador (según su orden entre los conectados).
    pub fn set_slot(&mut self, slot: usize) {
        self.slot = slot;
    }

    /// Ajusta el reloj del mundo al del servidor para que todos vean los planetas en el mismo lugar.
    pub fn sync_clock(&mut self, server_ms: u32) {
        let server = server_ms as f64 / 1000.0;
        let diff = server - self.world_time;
        self.world_time += if diff.abs() > 0.3 { diff } else { diff * 0.2 };
    }

    /// La fase de la carrera la manda el servidor.
    pub fn set_race_server(&mut self, phase: u8, ms: u32) {
        if let Some(p) = Phase::from_u8(phase) {
            self.race.set_server(p, ms as f32 / 1000.0);
        }
    }

    /// ¿Se pidió empezar la carrera? Devuelve true una sola vez por pulsación.
    pub fn take_start_request(&mut self) -> bool {
        std::mem::take(&mut self.start_request)
    }

    pub fn phase(&self) -> Phase {
        self.race.phase
    }

    pub fn progress(&self) -> usize {
        self.progress
    }

    fn lod_mesh(&self, dist: f32, radius: f32, focal: f32) -> &Mesh {
        let px = radius / dist.max(0.001) * focal;
        let i = if px > 120.0 { 3 } else if px > 40.0 { 2 } else if px > 12.0 { 1 } else { 0 };
        &self.spheres[i]
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_object(
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
        let view = self.camera.view();

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
            scene: Scene { time: self.time, light_pos: Vec3::ZERO, eye: self.camera.eye },
            focal: fb.height as f32 * 0.5 / (FOV_Y * 0.5).tan(),
        };
        let eye = self.camera.eye;
        let mut st = RenderStats::default();
        let t = self.world_time;

        // Sol
        let spin = Mat4::rotation_y((t * 0.02) as f32);
        let model = spin * Mat4::scale(Vec3::new(SUN_RADIUS, SUN_RADIUS, SUN_RADIUS));
        let mesh = self.lod_mesh(eye.length(), SUN_RADIUS, v.focal);
        self.draw_object(fb, &v, &mut st, mesh, &model, Vec3::ZERO, SUN_RADIUS, &Material::Sun);

        // Planetas
        for p in &PLANETS {
            let pos = scenario::planet_position(p, t);
            let rot = Mat4::rotation_z(p.tilt) * Mat4::rotation_y(scenario::planet_spin(p, t));
            let model = Mat4::translation(pos) * rot * Mat4::scale(Vec3::new(p.radius, p.radius, p.radius));
            let mesh = self.lod_mesh(pos.distance(eye), p.radius, v.focal);
            self.draw_object(fb, &v, &mut st, mesh, &model, pos, p.radius, &p.material);
        }

        // Puntos de control (en carrera se ven los que faltan; el siguiente resalta)
        let racing = matches!(self.race.phase, Phase::Racing | Phase::Countdown);
        let desde = if racing { self.progress } else { 0 };
        for (i, g) in self.course.iter().enumerate().skip(desde) {
            let (yaw, pitch) = scenario::orientation(g.dir);
            let model = Mat4::translation(g.pos) * Mat4::rotation_y(yaw) * Mat4::rotation_x(pitch);
            let color = if racing && i == self.progress { COLOR_SIGUIENTE } else { COLOR_PUNTO };
            self.draw_object(fb, &v, &mut st, &self.gate_mesh, &model, g.pos, g.radius + 1.0, &Material::Emissive { color });
        }

        // Rivales y mi nave
        for r in &self.remotes {
            let s = &r.shown;
            let mesh = &self.models[s.model as usize % self.models.len()].mesh;
            let model = Mat4::translation(s.position) * Mat4::rotation_y(s.yaw) * Mat4::rotation_x(s.pitch);
            let color = COLORES_JUGADORES[s.id as usize % COLORES_JUGADORES.len()];
            self.draw_object(fb, &v, &mut st, mesh, &model, s.position, 1.2, &Material::Flat { color });
        }
        let model = Mat4::translation(self.ship.position) * self.ship.rotation();
        let mesh = &self.models[self.selected].mesh;
        self.draw_object(fb, &v, &mut st, mesh, &model, self.ship.position, 1.2, &Material::Flat { color: COLOR_PROPIO });

        self.draw_hud(fb, &v, &st);
        st
    }

    fn entries(&self) -> Vec<Entry> {
        let mut list = vec![Entry { label: "TU".into(), color: 0xFFFFFF, progress: self.progress, finish_ms: self.finish_ms() }];
        for r in &self.remotes {
            list.push(Entry {
                label: format!("J{}", r.shown.id),
                color: COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()],
                progress: r.target.progress as usize,
                finish_ms: r.target.finish_ms,
            });
        }
        // Primero los que terminaron (por tiempo), luego por puntos de control.
        list.sort_by_key(|e| (e.finish_ms == 0, if e.finish_ms == 0 { 0 } else { e.finish_ms }, usize::MAX - e.progress));
        list
    }

    fn draw_hud(&self, fb: &mut Framebuffer, v: &View, st: &RenderStats) {
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
            let dist = pos.distance(self.camera.eye);
            if let Some((x, y, depth)) = raster::project(&v.vp, pos, fb.width, fb.height) {
                let r = (radius * v.focal / depth) as i32;
                if dist < 1500.0 && x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32 {
                    hud::centered(fb, x as i32, (y as i32 - r - 2 * line).max(m), s, color, name);
                    hud::centered(fb, x as i32, (y as i32 - r - line).max(m + line), s, 0x78909C, &format!("{dist:.0} M"));
                }
            }
        }

        // Datos de vuelo y de render
        hud::label(fb, m, m, s, white, &format!("VEL {:.0} U/S", self.ship.speed()));
        hud::label(fb, m, m + line, s, 0x90A4AE, &format!("TRIANGULOS {}", st.triangles));
        hud::label(fb, m, m + 2 * line, s, 0x90A4AE, &format!("OBJETOS {}/{}", st.visible, st.total));
        let red = if self.online { format!("ONLINE {} JUGADORES", 1 + self.remotes.len()) } else { "SIN CONEXION".to_string() };
        hud::label(fb, m, m + 3 * line, s, if self.online { 0x66BB6A } else { 0xEF9A9A }, &red);

        // Carrera
        let cx = w / 2;
        match self.race.phase {
            Phase::Lobby => {
                hud::centered(fb, cx, m, s * 2, COLOR_SIGUIENTE, "ENTER = EMPEZAR CARRERA");
                hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, 0xB0BEC5, "PUEDES PRACTICAR LIBREMENTE");
            }
            Phase::Countdown => {
                let n = self.race.countdown_number().unwrap_or(1).to_string();
                hud::centered(fb, cx, h / 3, s * 10, COLOR_SIGUIENTE, &n);
                hud::centered(fb, cx, h / 3 + 90 * s as i32, s * 2, white, "PREPARATE");
            }
            Phase::Racing => {
                hud::centered(fb, cx, m, s * 2, white, &fmt_time(self.finish_time.unwrap_or(self.race.time)));
                let estado = match self.finish_time {
                    Some(_) => "TERMINASTE! ESPERANDO A LOS DEMAS".to_string(),
                    None => format!("PUNTO {}/{}", self.progress + 1, GATE_COUNT),
                };
                hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, COLOR_SIGUIENTE, &estado);
            }
            Phase::Results => {
                hud::centered(fb, cx, m, s * 2, COLOR_SIGUIENTE, "CARRERA TERMINADA");
                hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, white, "ENTER = REVANCHA");
            }
        }

        // Marcador del siguiente punto de control (en pantalla o en el borde)
        if self.race.phase == Phase::Racing && self.progress < GATE_COUNT {
            let g = self.course[self.progress].pos;
            let dist = g.distance(self.ship.position);
            let (cxf, cyf) = (w as f32 / 2.0, h as f32 / 2.0);
            let (mut x, mut y, dentro) = match raster::project(&v.vp, g, fb.width, fb.height) {
                Some((x, y, _)) if x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32 => (x, y, true),
                Some((x, y, _)) => (x, y, false),
                None => {
                    // detrás: se apunta al lado contrario para que el jugador gire
                    let c = v.vp.transform_point(g);
                    (cxf - c.x.signum() * cxf * 2.0, cyf + c.y.signum() * cyf * 2.0, false)
                }
            };
            if !dentro {
                let (dx, dy) = (x - cxf, y - cyf);
                // El margen vertical deja libres el reloj de arriba y la lista y los controles de abajo.
                let (mx, my) = (4.0 * m as f32, 5.0 * m as f32 + 2.0 * line as f32);
                let k = ((cxf - mx) / dx.abs().max(1e-3)).min((cyf - my) / dy.abs().max(1e-3));
                x = cxf + dx * k;
                y = cyf + dy * k;
            }
            let b = 5 * s as i32;
            for d in 0..b {
                hud::rect(fb, x as i32 - d, y as i32 - b + d, 2 * d + 1, 1, COLOR_SIGUIENTE);
                hud::rect(fb, x as i32 - d, y as i32 + b - d, 2 * d + 1, 1, COLOR_SIGUIENTE);
            }
            hud::centered(fb, x as i32, y as i32 + b + 3 * s as i32, s, COLOR_SIGUIENTE, &format!("{dist:.0} M"));
        }

        // Posiciones
        let entries = self.entries();
        let base = h - m - line * (entries.len() as i32 + 1);
        hud::label(fb, m, base - line, s, 0x90A4AE, "POSICIONES");
        for (i, e) in entries.iter().enumerate() {
            let estado = if e.finish_ms > 0 { fmt_time(e.finish_ms as f32 / 1000.0) } else { format!("{}/{}", e.progress, GATE_COUNT) };
            hud::label(fb, m, base + i as i32 * line, s, e.color, &format!("{} {} {}", i + 1, e.label, estado));
        }

        self.draw_minimap(fb, s as i32, m);

        // Controles
        let ayuda = "W/S ACELERAR  A/D GIRAR  FLECHAS INCLINAR  ESPACIO FRENO  TAB NAVE  F PANTALLA";
        hud::centered(fb, cx, h - m - 8 * s as i32, s, 0x78909C, ayuda);
    }

    /// Mapa visto desde arriba: sol, órbitas, planetas, circuito y jugadores.
    fn draw_minimap(&self, fb: &mut Framebuffer, s: i32, m: i32) {
        const ALCANCE: f32 = 580.0;
        let size = 70 * s;
        let (cx, cy) = (fb.width as i32 - m - size, m + size);
        hud::rect(fb, cx - size - 1, cy - size - 1, 2 * size + 2, 2 * size + 2, 0x37474F);
        hud::rect(fb, cx - size, cy - size, 2 * size, 2 * size, 0x05070D);
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
        for (i, g) in self.course.iter().enumerate() {
            let (x, y) = to_map(g.pos);
            let (color, r) = if racing && i == self.progress { (COLOR_SIGUIENTE, s + 1) } else if i < self.progress && racing { (0x2E7D32, s) } else { (COLOR_PUNTO, s) };
            hud::rect(fb, x - r, y - r, 2 * r + 1, 2 * r + 1, color);
        }
        for r in &self.remotes {
            let (x, y) = to_map(r.shown.position);
            if en_mapa(x, y) {
                hud::dot(fb, x, y, s + 1, COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()]);
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

    /// Texto para el título de la ventana.
    pub fn status(&self) -> String {
        let red = if self.online { format!("online: {} jugadores", 1 + self.remotes.len()) } else { "sin conexion".to_string() };
        format!("Space Travel | nave: {} (Tab cambia) | vel {:.0} u/s | {red}", self.models[self.selected].name, self.ship.speed())
    }
}

fn fmt_time(secs: f32) -> String {
    let cs = (secs * 100.0) as u32;
    format!("{}:{:02}.{:02}", cs / 6000, cs / 100 % 60, cs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rival(id: u32, x: f32) -> RemoteShip {
        RemoteShip { id, position: Vec3::new(x, 0.0, -10.0), yaw: 0.0, pitch: 0.0, model: 0, progress: 0, finish_ms: 0 }
    }

    fn avanzar(g: &mut Game, segundos: f32) {
        for _ in 0..(segundos / 0.05).ceil() as usize {
            g.update(0.05, &Input::default());
        }
    }

    fn empezar() -> Input {
        Input { start: true, ..Default::default() }
    }

    #[test]
    fn el_tiempo_avanza_con_dt() {
        let mut g = Game::new(1);
        g.update(0.02, &Input::default());
        g.update(0.03, &Input::default());
        assert!((g.time - 0.05).abs() < 1e-6);
    }

    #[test]
    fn dt_enorme_se_limita() {
        let mut g = Game::new(1);
        g.update(10.0, &Input::default());
        assert!(g.time <= DT_MAX + 1e-6);
    }

    #[test]
    fn tab_cambia_de_modelo() {
        let mut g = Game::new(1);
        let antes = g.selected;
        g.update(0.01, &Input { cycle_model: true, ..Default::default() });
        assert_eq!(g.selected, (antes + 1) % g.models.len());
    }

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
        let mut g = Game::new(1);
        // Mirando hacia afuera del sistema solo se ve una parte de los objetos.
        g.ship.reset(Vec3::new(900.0, 0.0, 0.0), -PI / 2.0, 0.0);
        g.camera = ChaseCamera::new(&g.ship);
        let st = g.render(&mut Framebuffer::new(160, 90));
        assert!(st.visible < st.total, "{st:?}");
    }

    #[test]
    fn el_lod_usa_menos_triangulos_de_lejos() {
        let g = Game::new(1);
        let (cerca, lejos) = (g.lod_mesh(40.0, 10.0, 400.0), g.lod_mesh(4000.0, 10.0, 400.0));
        assert!(cerca.triangles.len() > lejos.triangles.len());
    }

    #[test]
    fn los_jugadores_remotos_entran_y_salen() {
        let mut g = Game::new(1);
        g.set_remote_players(&[rival(1, 0.0), rival(2, 3.0)]);
        assert_eq!(g.remote_count(), 2);
        g.set_remote_players(&[rival(2, 4.0)]);
        assert_eq!(g.remote_count(), 1);
        g.set_online(false);
        assert_eq!(g.remote_count(), 0);
    }

    #[test]
    fn un_rival_a_la_vista_cambia_la_imagen() {
        let mut g = Game::new(1);
        let (mut sin, mut con) = (Framebuffer::new(320, 240), Framebuffer::new(320, 240));
        g.render(&mut sin);
        let adelante = g.ship.position + g.ship.forward() * 12.0;
        g.set_remote_players(&[RemoteShip { position: adelante + Vec3::new(3.0, 0.0, 0.0), ..rival(3, 0.0) }]);
        g.render(&mut con);
        assert_ne!(sin.pixels, con.pixels);
    }

    #[test]
    fn el_estado_local_refleja_la_nave() {
        let mut g = Game::new(1);
        let antes = g.local_state().position;
        g.update(0.05, &Input { forward: true, ..Default::default() });
        assert_ne!(g.local_state().position, antes);
    }

    #[test]
    fn una_carrera_completa_sin_servidor() {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        assert_eq!(g.phase(), Phase::Countdown);
        avanzar(&mut g, 3.2);
        assert_eq!(g.phase(), Phase::Racing);
        for i in 0..GATE_COUNT {
            g.ship.position = g.course[i].pos;
            g.update(0.05, &Input::default());
            assert_eq!(g.progress(), i + 1);
        }
        assert_eq!(g.phase(), Phase::Results);
        assert!(g.finish_time.is_some());
        g.update(0.05, &empezar());
        assert_eq!((g.phase(), g.progress()), (Phase::Countdown, 0), "la revancha reinicia todo");
    }

    #[test]
    fn los_puntos_se_cruzan_en_orden() {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        g.ship.position = g.course[3].pos; // saltarse puntos no cuenta
        g.update(0.05, &Input::default());
        assert_eq!(g.progress(), 0);
    }

    #[test]
    fn en_la_cuenta_regresiva_la_nave_no_se_mueve() {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        let pos = g.ship.position;
        for _ in 0..20 {
            g.update(0.05, &Input { forward: true, ..Default::default() });
        }
        assert!(g.ship.position.distance(pos) < 1e-3);
    }

    #[test]
    fn online_el_servidor_manda_la_fase_y_la_nave_va_a_su_casilla() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_slot(2);
        g.ship.position = Vec3::new(5.0, 5.0, 5.0);
        g.set_race_server(1, 0);
        g.update(0.01, &Input::default());
        assert_eq!(g.phase(), Phase::Countdown);
        assert!(g.ship.position.distance(scenario::grid_slot(2).0) < 1.0);
        avanzar(&mut g, 5.0);
        assert_eq!(g.phase(), Phase::Countdown, "online solo el servidor cambia la fase");
    }

    #[test]
    fn online_pedir_empezar_no_arranca_solo_avisa() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.update(0.05, &empezar());
        assert_eq!(g.phase(), Phase::Lobby);
        assert!(g.take_start_request());
        assert!(!g.take_start_request(), "se entrega una sola vez");
    }

    #[test]
    fn el_reloj_se_sincroniza_con_el_servidor() {
        let mut g = Game::new(1);
        g.sync_clock(120_000);
        assert!((g.world_time - 120.0).abs() < 1e-6);
        g.sync_clock(120_050);
        assert!(g.world_time > 120.0 && g.world_time < 120.05);
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
    fn el_formato_de_tiempo() {
        assert_eq!(fmt_time(0.0), "0:00.00");
        assert_eq!(fmt_time(75.5), "1:15.50");
    }

    #[test]
    fn el_hud_se_dibuja_en_cualquier_resolucion_sin_paniquear() {
        let g = Game::new(1);
        for (w, h) in [(160, 90), (800, 600), (1920, 1080), (100, 1000)] {
            g.render(&mut Framebuffer::new(w, h));
        }
    }
}
