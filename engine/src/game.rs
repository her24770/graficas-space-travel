use crate::{
    assets,
    camera::ChaseCamera,
    frustum::Frustum,
    hud,
    input::Input,
    obj,
    orbit::{self, Orbit},
    physics::{self, Body},
    procedural,
    props::{self, PropInstance},
    race::{Phase, Race},
    raster::{self, draw_mesh},
    remote::{RemotePlayer, RemoteShip},
    rng::Rng,
    scenario::{self, Gate, GATE_COUNT, PLANETS, PROPS_SEED, SUN_GRAVITY, SUN_RADIUS},
    shaders::{Material, Scene},
    terrain, Framebuffer, Mat4, Mesh, Ship, Vec3,
};
use std::f32::consts::PI;

const FOV_Y: f32 = 60.0 * PI / 180.0;
const FONDO: u32 = 0x02020A;
const DT_MAX: f32 = 0.05; // evita saltos al volver de una pestaña inactiva
const COLOR_PROPIO: u32 = 0xDDDDDD;
const COLORES_JUGADORES: [u32; 6] = [0x42A5F5, 0xEF5350, 0x66BB6A, 0xFFCA28, 0xAB47BC, 0x26C6DA];
const COLOR_SIGUIENTE: u32 = 0xFFD54F;
const COLOR_PUNTO: u32 = 0x42A5F5;
/// Mitad del lado del minimapa, en unidades de HUD (se multiplica por la escala del texto).
const MINIMAPA_MITAD: i32 = 32;
/// Subdivisiones de la esfera, de menos a más detalle (LOD): 80, 320, 1280 y 5120 triángulos.
const LOD_NIVELES: [u32; 4] = [1, 2, 3, 4];
/// Radio de la nave para chocar (todo modelo de nave se normaliza a radio 1).
const NAVE_RADIO: f32 = 1.0;
/// Velocidad de impacto mínima para contar un choque (apoyarse en el suelo no cuenta).
const IMPACTO_MIN: f32 = 1.0;
/// Segundos que se muestra un aviso en pantalla.
const AVISO_S: f32 = 2.0;
/// Se puede entrar en órbita a menos de esta altura sobre la superficie (en radios del cuerpo).
const CAPTURA_RADIOS: f32 = 5.0;
/// Los objetos de superficie se dibujan a menos de esta distancia (en radios del planeta).
const PROPS_DISTANCIA: f32 = 14.0;

/// Pantalla en la que está el jugador: el menú de inicio o el juego.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Playing,
}

/// Desde dónde se ve la nave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    ThirdPerson,
    /// Desde la cabina: la nave propia no se dibuja para no tapar la vista.
    FirstPerson,
}

/// Un choque de la nave contra el sol o un planeta. Es el punto donde conectar efectos
/// visuales o de sonido: se leen con `Game::take_collisions`.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionEvent {
    /// "SOL" o el nombre del planeta.
    pub body: &'static str,
    /// Punto de la superficie donde chocó.
    pub position: Vec3,
    /// Hacia afuera de la superficie.
    pub normal: Vec3,
    /// Qué tan rápido iba hacia el suelo.
    pub impact_speed: f32,
}

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
    eye: Vec3,
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
    screen: Screen,
    prev_input: Input,
    /// Esferas con relieve de cada planeta, una por nivel de LOD (`None` = planeta liso).
    planet_lods: Vec<Option<Vec<Mesh>>>,
    /// Mallas de los objetos de superficie; la 0 es el cono de prueba.
    prop_meshes: Vec<Mesh>,
    /// `[planeta][tipo]` → índice en `prop_meshes`.
    prop_mesh_of: Vec<Vec<usize>>,
    placements: Vec<Vec<PropInstance>>,
    props_seed: u64,
    gravity_on: bool,
    view: ViewMode,
    orbit: Option<Orbit>,
    collisions: Vec<CollisionEvent>,
    /// Aviso en pantalla: texto, color y hasta cuándo (segundos de `time`).
    notice: Option<(String, u32, f32)>,
}

/// Nombre del cuerpo `i` (0 = sol, 1.. = planetas).
fn body_name(i: usize) -> &'static str {
    if i == 0 {
        "SOL"
    } else {
        PLANETS[i - 1].name
    }
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut models: Vec<Model> = assets::MODELS
            .iter()
            .map(|(name, bytes)| match obj::parse_with_mtl(bytes, assets::model_mtl(name)) {
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

        let planet_lods = PLANETS
            .iter()
            .map(|p| {
                (p.relief > 0.0).then(|| {
                    LOD_NIVELES
                        .iter()
                        .map(|&n| procedural::displaced_sphere(n, |d| 1.0 + p.relief * terrain::elevation(&p.material, d)))
                        .collect()
                })
            })
            .collect();

        let mut prop_meshes = vec![procedural::cone(8)];
        let prop_mesh_of = PLANETS
            .iter()
            .map(|p| {
                p.props
                    .iter()
                    .map(|def| {
                        let modelo = assets::PROPS.iter().find(|(n, _)| *n == def.model).and_then(|(n, b)| obj::parse_with_mtl(b, assets::prop_mtl(n)).ok());
                        modelo.map_or(0, |m| {
                            prop_meshes.push(m.grounded());
                            prop_meshes.len() - 1
                        })
                    })
                    .collect()
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
            screen: Screen::Menu,
            prev_input: Input::default(),
            planet_lods,
            prop_meshes,
            prop_mesh_of,
            placements: Vec::new(),
            props_seed: PROPS_SEED,
            gravity_on: true,
            view: ViewMode::ThirdPerson,
            orbit: None,
            collisions: Vec::new(),
            notice: None,
        };
        game.placements = Self::place_props(PROPS_SEED);
        game.place_on_grid();
        game
    }

    /// Coloca la nave en su casilla de salida, en reposo.
    pub fn place_on_grid(&mut self) {
        let (pos, yaw, pitch) = scenario::grid_slot(self.slot);
        self.ship.reset(pos, yaw, pitch);
        self.camera = ChaseCamera::new(&self.ship);
        self.orbit = None;
    }

    fn place_props(seed: u64) -> Vec<Vec<PropInstance>> {
        PLANETS.iter().enumerate().map(|(i, p)| props::place(i, p, seed)).collect()
    }

    /// Cambia la semilla de los objetos de superficie y los vuelve a colocar.
    pub fn set_props_seed(&mut self, seed: u64) {
        self.props_seed = seed;
        self.placements = Self::place_props(seed);
    }

    pub fn props_seed(&self) -> u64 {
        self.props_seed
    }

    pub fn view_mode(&self) -> ViewMode {
        self.view
    }

    pub fn gravity_enabled(&self) -> bool {
        self.gravity_on
    }

    /// Cuerpo que se está orbitando (0 = sol, 1.. = planetas), si hay.
    pub fn orbiting(&self) -> Option<usize> {
        self.orbit.map(|o| o.body)
    }

    /// Choques ocurridos desde la última llamada (para conectar efectos).
    pub fn take_collisions(&mut self) -> Vec<CollisionEvent> {
        std::mem::take(&mut self.collisions)
    }

    fn notice(&mut self, text: String, color: u32) {
        self.notice = Some((text, color, self.time + AVISO_S));
    }

    /// Sol y planetas en este instante, con su gravedad: 0 = sol, 1.. = planetas.
    fn bodies(&self) -> Vec<Body> {
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

    /// Radio de la superficie del cuerpo `i` en la dirección del punto `p`: tiene en cuenta el
    /// relieve y que el planeta está girando.
    fn surface_at(&self, i: usize, b: &Body, p: Vec3) -> f32 {
        if i == 0 {
            return SUN_RADIUS;
        }
        let planet = &PLANETS[i - 1];
        let local = scenario::planet_rotation(planet, self.world_time).transposed().transform_dir(p - b.center);
        scenario::surface_radius(planet, local)
    }

    fn max_surface(i: usize) -> f32 {
        if i == 0 {
            SUN_RADIUS
        } else {
            scenario::max_radius(&PLANETS[i - 1])
        }
    }

    fn resolve_collisions(&mut self, dt: f32) {
        for (i, b) in self.bodies().iter().enumerate() {
            let surface = self.surface_at(i, b, self.ship.position);
            let choque = physics::collide(&mut self.ship.position, &mut self.ship.velocity, NAVE_RADIO, b.center, b.velocity, surface, dt);
            if let Some(c) = choque.filter(|c| c.impact_speed > IMPACTO_MIN) {
                let body = body_name(i);
                self.collisions.push(CollisionEvent {
                    body,
                    position: self.ship.position - c.normal * NAVE_RADIO,
                    normal: c.normal,
                    impact_speed: c.impact_speed,
                });
                if self.collisions.len() > 32 {
                    self.collisions.remove(0); // nadie los está leyendo: no crecer sin límite
                }
                self.notice(format!("COLISION: {body}"), 0xEF5350);
            }
        }
    }

    fn toggle_orbit(&mut self) {
        if let Some(o) = self.orbit.take() {
            // La nave sigue con la velocidad que llevaba en la órbita.
            self.notice(format!("SALISTE DE LA ORBITA DE {}", body_name(o.body)), 0x90A4AE);
            return;
        }
        let bodies = self.bodies();
        let cercano = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b, self.ship.position.distance(b.center) - Self::max_surface(i)))
            .filter(|(_, b, altura)| *altura < b.radius * CAPTURA_RADIOS)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        match cercano {
            Some((i, b, _)) => {
                let offset = self.ship.position - b.center;
                let radio = offset.length().max(Self::max_surface(i) + NAVE_RADIO + 2.0);
                let speed = orbit::circular_speed(b.surface_gravity, b.radius, radio);
                let mut o = Orbit::enter(i, offset, self.ship.velocity - b.velocity, self.ship.forward(), speed);
                o.radius = radio;
                self.orbit = Some(o);
                self.notice(format!("ORBITANDO {}", body_name(i)), 0x66BB6A);
            }
            None => self.notice("NADA CERCA PARA ORBITAR".into(), 0x90A4AE),
        }
    }

    /// Avanza la órbita con los controles locales y coloca la nave en ella.
    fn fly_orbit(&mut self, mut o: Orbit, input: &Input, dt: f32) {
        let b = self.bodies()[o.body];
        let piso = Self::max_surface(o.body) + NAVE_RADIO + 2.0;
        let techo = Self::max_surface(o.body) + b.radius * CAPTURA_RADIOS * 2.0;
        let eje = |mas: bool, menos: bool| mas as i32 as f32 - menos as i32 as f32;
        o.update(dt, eje(input.pitch_up, input.pitch_down), eje(input.forward, input.backward), eje(input.yaw_left, input.yaw_right), piso, techo);
        let t = o.tangent();
        let (yaw, pitch) = scenario::ship_orientation(t);
        self.ship.position = o.position(b.center);
        self.ship.velocity = t * o.speed + b.velocity;
        self.ship.yaw = yaw;
        self.ship.pitch = pitch;
        self.orbit = Some(o);
    }

    fn next_model(&mut self, delta: isize) {
        let n = self.models.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(n) as usize;
    }

    pub fn update(&mut self, dt: f32, input: &Input) {
        let dt = dt.clamp(0.0, DT_MAX);
        self.time += dt;
        self.world_time += dt as f64;
        let before = std::mem::replace(&mut self.prev_input, *input);
        let pressed = |now: bool, was: bool| now && !was;

        if self.screen == Screen::Menu {
            if input.cycle_model || pressed(input.yaw_right, before.yaw_right) {
                self.next_model(1);
            }
            if pressed(input.yaw_left, before.yaw_left) {
                self.next_model(-1);
            }
            if input.start {
                self.screen = Screen::Playing;
            }
        } else {
            if input.cycle_model {
                self.next_model(1);
            }
            if input.menu && matches!(self.race.phase, Phase::Lobby | Phase::Results) {
                self.back_to_menu();
            } else if input.start {
                if self.online {
                    self.start_request = true;
                } else {
                    self.race.start_local();
                }
            }
        }

        if self.screen == Screen::Playing {
            if input.toggle_gravity {
                self.gravity_on = !self.gravity_on;
            }
            if input.toggle_view {
                self.view = match self.view {
                    ViewMode::ThirdPerson => ViewMode::FirstPerson,
                    ViewMode::FirstPerson => ViewMode::ThirdPerson,
                };
            }
            if input.toggle_orbit && self.race.phase != Phase::Countdown {
                self.toggle_orbit();
            }
        }
        if input.reseed {
            self.set_props_seed(self.props_seed + 1);
            self.notice(format!("SEMILLA {}", self.props_seed), 0x90A4AE);
        }

        self.race.update(dt, self.online);
        if self.race.phase != self.last_phase {
            if self.race.phase == Phase::Countdown {
                self.progress = 0;
                self.finish_time = None;
                self.screen = Screen::Playing; // si alguien larga, nadie se queda en el menú
                self.place_on_grid();
            }
            self.last_phase = self.race.phase;
        }

        // En el menú y en la cuenta regresiva la nave espera quieta en la parrilla.
        let quieta = self.screen == Screen::Menu || self.race.phase == Phase::Countdown;
        match self.orbit.filter(|_| !quieta) {
            Some(o) => self.fly_orbit(o, input, dt),
            None => {
                let ctrl = if quieta { Input::default() } else { *input };
                if self.gravity_on && !quieta {
                    self.ship.velocity += physics::gravity_at(self.ship.position, &self.bodies()) * dt;
                }
                self.ship.update(&ctrl, dt);
                if !quieta {
                    self.resolve_collisions(dt);
                }
            }
        }
        match self.orbit {
            // En órbita la cámara va detrás y por encima, mirando hacia el planeta para que se vea.
            Some(o) => {
                let centro = self.bodies()[o.body].center;
                let ojo = self.ship.position + o.radial() * 8.0 - o.tangent() * 10.0;
                self.camera.follow(ojo, self.ship.position.lerp(centro, 0.35), dt);
            }
            None => self.camera.update(&self.ship, dt),
        }
        self.check_gates();
        for r in &mut self.remotes {
            r.update(dt);
        }
    }

    fn back_to_menu(&mut self) {
        self.screen = Screen::Menu;
        if !self.online {
            self.race.reset();
            self.last_phase = Phase::Lobby;
            self.progress = 0;
            self.finish_time = None;
        }
        self.place_on_grid();
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Cámara del menú: gira despacio alrededor del sol mostrando el sistema.
    fn menu_camera(&self) -> (Vec3, Vec3) {
        let a = (self.world_time * 0.08) as f32;
        (Vec3::new(a.cos() * 230.0, 70.0 + 25.0 * (a * 0.7).sin(), a.sin() * 230.0), Vec3::ZERO)
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
        if slot != self.slot {
            self.slot = slot;
            // En el menú la nave está estacionada: que se vea en su casilla y no encima de las demás.
            if self.screen == Screen::Menu {
                self.place_on_grid();
            }
        }
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

    /// Nivel de detalle según cuántos píxeles mide el objeto en pantalla.
    fn lod_index(dist: f32, radius: f32, focal: f32) -> usize {
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
    fn camera_pose(&self) -> (Vec3, Vec3) {
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
            scene: Scene { time: self.time, light_pos: Vec3::ZERO, eye },
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
        let mesh = &self.models[self.selected].mesh;
        if menu {
            // Vista previa: la nave gira frente a la cámara, con una luz propia desde un costado.
            let f = (target - eye).normalize();
            let (right, up) = (f.cross(Vec3::Y).normalize(), f.cross(Vec3::Y).normalize().cross(f));
            let pos = eye + f * 7.0 + up * 0.5;
            let model = Mat4::translation(pos) * Mat4::rotation_y(self.time * 0.9) * Mat4::rotation_x(0.35);
            let luz = Scene { light_pos: eye + right * 25.0 + up * 18.0 - f * 8.0, ..v.scene };
            let vista = View { vp: v.vp, frustum: v.frustum, scene: luz, focal: v.focal, eye: v.eye };
            self.draw_object(fb, &vista, &mut st, mesh, &model, pos, 1.2, &Material::Flat { color: COLOR_PROPIO });
            self.draw_menu(fb);
        } else {
            // En primera persona la nave propia no se dibuja: no tapa la vista.
            if self.view == ViewMode::ThirdPerson {
                let model = Mat4::translation(self.ship.position) * self.ship.rotation();
                self.draw_object(fb, &v, &mut st, mesh, &model, self.ship.position, 1.2, &Material::Flat { color: COLOR_PROPIO });
            }
            self.draw_hud(fb, &v, &st);
        }
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
                hud::centered(fb, cx, m, s * 2, white, &fmt_time(self.finish_time.unwrap_or(self.race.time)));
                let estado = match self.finish_time {
                    Some(_) => "TERMINASTE! ESPERANDO A LOS DEMAS".to_string(),
                    None => format!("PUNTO {}/{}", self.progress + 1, GATE_COUNT),
                };
                hud::centered(fb, cx, m + 2 * line + 4 * s as i32, s, white, &estado);
            }
            Phase::Results => banner(fb, s, "CARRERA TERMINADA", "ENTER REVANCHA   M MENU"),
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
        let en_sala = self.race.phase == Phase::Lobby;
        hud::label(fb, m, base - line, s, 0x90A4AE, if en_sala { "EN LA SALA" } else { "POSICIONES" });
        for (i, e) in entries.iter().enumerate() {
            let texto = if en_sala {
                e.label.clone()
            } else {
                let estado = if e.finish_ms > 0 { fmt_time(e.finish_ms as f32 / 1000.0) } else { format!("{}/{}", e.progress, GATE_COUNT) };
                format!("{} {} {}", i + 1, e.label, estado)
            };
            hud::label(fb, m, base + i as i32 * line, s, e.color, &texto);
        }

        self.draw_minimap(fb, s as i32, m);
    }

    /// Pantalla de inicio: título, vista previa de la nave, controles y estado de la sala.
    fn draw_menu(&self, fb: &mut Framebuffer) {
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
        let py = h * 3 / 10;
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
            filas.push(("TU".into(), blanco));
            for r in &self.remotes {
                filas.push((format!("J{}", r.shown.id), COLORES_JUGADORES[r.shown.id as usize % COLORES_JUGADORES.len()]));
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
        hud::centered(fb, cx, h - m - 30 * si, s * 3, color, "ENTER = JUGAR");
    }

    /// Mapa visto desde arriba: sol, órbitas, planetas, circuito y jugadores.
    fn draw_minimap(&self, fb: &mut Framebuffer, s: i32, m: i32) {
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
        for (i, g) in self.course.iter().enumerate() {
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

/// ¿Hay algo dibujado delante de la esfera (`pos`, `radius`) en el píxel (x, y)?
/// Compara el z-buffer con la profundidad del punto de la esfera más cercano a la cámara.
fn tapado(fb: &Framebuffer, vp: &Mat4, eye: Vec3, pos: Vec3, radius: f32, x: f32, y: f32) -> bool {
    let frente = pos + (eye - pos).normalize() * radius;
    let c = vp.transform_point(frente);
    if c.w <= 1e-4 {
        return false;
    }
    let idx = y as usize * fb.width + x as usize;
    fb.depth.get(idx).is_some_and(|&d| d < c.z / c.w - 1e-4)
}

/// Cartel centrado arriba con un título y una pista de teclas.
fn banner(fb: &mut Framebuffer, s: usize, titulo: &str, pista: &str) {
    let si = s as i32;
    let pad = 6 * si;
    let w = hud::width(titulo, s * 2).max(hud::width(pista, s)) + 2 * pad;
    let (x, y) = (fb.width as i32 / 2 - w / 2, 6 * si);
    hud::panel(fb, x, y, w, 25 * si + 2 * pad);
    hud::centered(fb, fb.width as i32 / 2, y + pad, s * 2, COLOR_SIGUIENTE, titulo);
    hud::centered(fb, fb.width as i32 / 2, y + pad + 18 * si, s, 0xFFFFFF, pista);
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

    /// Un juego que ya salió del menú.
    fn jugando() -> Game {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        g
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
        let mut g = jugando();
        let (mut sin, mut con) = (Framebuffer::new(320, 240), Framebuffer::new(320, 240));
        g.render(&mut sin);
        let adelante = g.ship.position + g.ship.forward() * 12.0;
        g.set_remote_players(&[RemoteShip { position: adelante + Vec3::new(3.0, 0.0, 0.0), ..rival(3, 0.0) }]);
        g.render(&mut con);
        assert_ne!(sin.pixels, con.pixels);
    }

    #[test]
    fn el_estado_local_refleja_la_nave() {
        let mut g = jugando();
        let antes = g.local_state().position;
        g.update(0.05, &Input { forward: true, ..Default::default() });
        assert_ne!(g.local_state().position, antes);
    }

    #[test]
    fn una_carrera_completa_sin_servidor() {
        let mut g = jugando();
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
        let mut g = jugando();
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        g.ship.position = g.course[3].pos; // saltarse puntos no cuenta
        g.update(0.05, &Input::default());
        assert_eq!(g.progress(), 0);
    }

    #[test]
    fn en_la_cuenta_regresiva_la_nave_no_se_mueve() {
        let mut g = jugando();
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
        let mut g = jugando();
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
        let en_juego = jugando();
        for (w, h) in [(160, 90), (800, 600), (1920, 1080), (100, 1000)] {
            g.render(&mut Framebuffer::new(w, h)); // menú
            en_juego.render(&mut Framebuffer::new(w, h));
        }
    }

    #[test]
    fn empieza_en_el_menu_y_la_nave_no_se_mueve() {
        let mut g = Game::new(1);
        assert_eq!(g.screen(), Screen::Menu);
        let pos = g.ship.position;
        for _ in 0..20 {
            g.update(0.05, &Input { forward: true, yaw_left: true, ..Default::default() });
        }
        assert!(g.ship.position.distance(pos) < 1e-3);
        assert_eq!(g.screen(), Screen::Menu);
    }

    #[test]
    fn enter_en_el_menu_entra_al_juego_sin_empezar_la_carrera() {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        assert_eq!((g.screen(), g.phase()), (Screen::Playing, Phase::Lobby));
        g.update(0.05, &empezar());
        assert_eq!(g.phase(), Phase::Countdown, "el segundo Enter sí larga");
    }

    #[test]
    fn m_vuelve_al_menu_en_la_sala_pero_no_en_plena_carrera() {
        let m = Input { menu: true, ..Default::default() };
        let mut g = jugando();
        g.update(0.05, &m);
        assert_eq!(g.screen(), Screen::Menu);

        let mut g = jugando();
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        assert_eq!(g.phase(), Phase::Racing);
        g.update(0.05, &m);
        assert_eq!(g.screen(), Screen::Playing, "en carrera no se puede salir");
    }

    #[test]
    fn volver_al_menu_despues_de_una_carrera_la_reinicia() {
        let mut g = jugando();
        g.update(0.05, &empezar());
        avanzar(&mut g, 3.2);
        for i in 0..GATE_COUNT {
            g.ship.position = g.course[i].pos;
            g.update(0.05, &Input::default());
        }
        assert_eq!(g.phase(), Phase::Results);
        g.update(0.05, &Input { menu: true, ..Default::default() });
        assert_eq!((g.screen(), g.phase(), g.progress()), (Screen::Menu, Phase::Lobby, 0));
    }

    #[test]
    fn en_el_menu_a_d_cambian_de_nave_una_vez_por_pulsacion() {
        let mut g = Game::new(1);
        let (n, inicio) = (g.models.len(), g.selected);
        let der = Input { yaw_right: true, ..Default::default() };
        for _ in 0..5 {
            g.update(0.05, &der); // tecla mantenida: cuenta una sola vez
        }
        assert_eq!(g.selected, (inicio + 1) % n);
        g.update(0.05, &Input::default());
        g.update(0.05, &Input { yaw_left: true, ..Default::default() });
        assert_eq!(g.selected, inicio);
    }

    #[test]
    fn si_el_servidor_larga_el_que_esta_en_el_menu_entra_a_la_carrera() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_race_server(1, 0);
        g.update(0.01, &Input::default());
        assert_eq!((g.screen(), g.phase()), (Screen::Playing, Phase::Countdown));
    }

    #[test]
    fn en_el_menu_cada_jugador_espera_en_su_casilla() {
        let mut g = Game::new(1);
        g.set_slot(3);
        assert!(g.ship.position.distance(scenario::grid_slot(3).0) < 1.0);
    }

    #[test]
    fn el_minimapa_ocupa_poco_de_la_pantalla() {
        for h in [270usize, 540, 1080] {
            let s = (h / 270).max(1) as i32;
            let lado = 2 * MINIMAPA_MITAD * s;
            assert!(lado * 4 < h as i32, "a {h}px de alto el minimapa mide {lado}px: debe ser menos de la cuarta parte");
        }
    }

    // ---- física, órbita, vistas y objetos de superficie ----

    const NADA: Input = Input {
        forward: false, backward: false, yaw_left: false, yaw_right: false, pitch_up: false, pitch_down: false,
        brake: false, cycle_model: false, start: false, menu: false, toggle_gravity: false, toggle_orbit: false,
        toggle_view: false, reseed: false,
    };

    /// Coloca la nave en `pos`, quieta, con la cámara detrás.
    fn poner(g: &mut Game, pos: Vec3) {
        g.ship.reset(pos, 0.0, 0.0);
        g.camera = ChaseCamera::new(&g.ship);
    }

    fn aqua(g: &Game) -> (Vec3, f32) {
        let i = PLANETS.iter().position(|p| p.name == "AQUA").unwrap();
        (scenario::planet_position(&PLANETS[i], g.world_time), PLANETS[i].radius)
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
    fn el_modo_orbita_sigue_al_planeta_a_altura_fija() {
        let mut g = jugando();
        g.update(0.05, &Input { toggle_gravity: true, ..NADA }); // la órbita no depende de la gravedad
        let (centro, radio) = aqua(&g);
        poner(&mut g, centro + Vec3::new(radio * 2.0, 0.0, 0.0));
        g.update(0.05, &Input { toggle_orbit: true, ..NADA });
        let cuerpo = g.orbiting().expect("cerca de AQUA debe entrar en órbita");
        assert_eq!(body_name(cuerpo), "AQUA");

        let distancia = |g: &Game| g.ship.position.distance(aqua(g).0);
        let d0 = distancia(&g);
        let inicio = aqua(&g).0;
        avanzar(&mut g, 3.0);
        assert!((distancia(&g) - d0).abs() < 0.5, "altura constante sin tocar controles");
        assert!(aqua(&g).0.distance(inicio) > 10.0, "el planeta se movió y la nave lo siguió");

        avanzar_con(&mut g, 1.0, Input { pitch_up: true, ..NADA });
        assert!(distancia(&g) > d0 + 5.0, "flecha arriba sube la órbita");

        g.update(0.05, &Input { toggle_orbit: true, ..NADA });
        assert!(g.orbiting().is_none());
        assert!(g.ship.velocity.length() > 1.0, "al salir conserva su velocidad");
    }

    fn avanzar_con(g: &mut Game, segundos: f32, i: Input) {
        for _ in 0..(segundos / 0.05).ceil() as usize {
            g.update(0.05, &i);
        }
    }

    #[test]
    fn en_orbita_el_planeta_queda_a_la_vista() {
        let mut g = jugando();
        let i = PLANETS.iter().position(|p| p.name == "GLACIA").unwrap();
        let p = &PLANETS[i];
        let c = scenario::planet_position(p, g.world_time);
        poner(&mut g, c + Vec3::new(p.radius * 2.2, 0.0, 0.0));
        g.update(0.05, &Input { toggle_orbit: true, ..NADA });
        avanzar(&mut g, 2.0);
        let (eye, target) = g.camera_pose();
        let vp = Mat4::perspective(FOV_Y, 16.0 / 9.0, 0.5, 6000.0) * Mat4::look_at(eye, target, Vec3::Y);
        let c = scenario::planet_position(p, g.world_time);
        assert!(Frustum::from_view_proj(&vp).sphere_visible(c, p.radius));
        assert!(Frustum::from_view_proj(&vp).sphere_visible(g.ship.position, NAVE_RADIO), "y la nave también");
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

    #[test]
    fn sin_nada_cerca_no_se_entra_en_orbita() {
        let mut g = jugando();
        poner(&mut g, Vec3::new(0.0, 3000.0, 0.0));
        g.update(0.05, &Input { toggle_orbit: true, ..NADA });
        assert!(g.orbiting().is_none());
        assert!(g.notice.as_ref().is_some_and(|n| n.0.contains("NADA CERCA")));
    }

    #[test]
    fn la_cuenta_regresiva_saca_de_la_orbita() {
        let mut g = jugando();
        let (centro, radio) = aqua(&g);
        poner(&mut g, centro + Vec3::new(radio * 2.0, 0.0, 0.0));
        g.update(0.05, &Input { toggle_orbit: true, ..NADA });
        assert!(g.orbiting().is_some());
        g.update(0.05, &empezar());
        assert!(g.orbiting().is_none());
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
    fn r_cambia_la_semilla_y_reacomoda_los_objetos() {
        let mut g = jugando();
        let i = PLANETS.iter().position(|p| !p.props.is_empty()).unwrap();
        let antes = g.placements[i][0].base;
        let semilla = g.props_seed();
        g.update(0.05, &Input { reseed: true, ..NADA });
        assert_eq!(g.props_seed(), semilla + 1);
        assert_ne!(g.placements[i][0].base, antes);
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

    #[test]
    fn los_planetas_con_relieve_usan_su_propia_malla() {
        let g = Game::new(1);
        for (i, p) in PLANETS.iter().enumerate() {
            assert_eq!(g.planet_lods[i].is_some(), p.relief > 0.0, "{}", p.name);
        }
    }

    #[test]
    fn sin_modelos_de_superficie_se_usa_el_cono_de_prueba() {
        let g = Game::new(1);
        for (i, p) in PLANETS.iter().enumerate() {
            for (k, def) in p.props.iter().enumerate() {
                let existe = assets::PROPS.iter().any(|(n, _)| *n == def.model);
                assert_eq!(g.prop_mesh_of[i][k] == 0, !existe, "{}", def.model);
            }
        }
    }
}
