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
    remote::{RemotePlayer, RemoteShip, ELIMINADO, ESPERA, NAME_LEN, SOLO},
    rng::Rng,
    scenario::{self, Gate, BLACK_HOLES, GATE_COUNT, PLANETS, PROPS_SEED, SUN_GRAVITY, SUN_RADIUS, UFOS},
    shaders::{Material, Scene},
    sparks::Sparks,
    terrain, Framebuffer, Mat4, Mesh, Ship, Vec3,
};
use std::f32::consts::PI;

mod chase;
mod collisions;
mod render;
mod ui;

use chase::Persecucion;
pub use chase::Modo;

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
/// Distancia de alcance de sonido
const AGUJERO_ALCANCE: f32 = 160.0;
const OVNI_ALCANCE: f32 = 100.0;
/// Segundos que se muestra un aviso en pantalla.
const AVISO_S: f32 = 2.0;
/// Cuánto dura el cartel de "perdiste" antes de reaparecer
const DERROTA_S: f32 = 4.0;
/// Se puede entrar en órbita a menos de esta altura sobre la superficie (en radios del cuerpo).
const CAPTURA_RADIOS: f32 = 5.0;
/// Los objetos de superficie se dibujan a menos de esta distancia (en radios del planeta).
const PROPS_DISTANCIA: f32 = 14.0;
/// Altura sobre la nave rival a la que se dibuja su nombre, y distancia máxima a la que se ve.
const NOMBRE_ALTURA: f32 = 2.3;
const NOMBRE_DISTANCIA: f32 = 800.0;
/// Los objetos de superficie chocan como esferas de este radio (fracción de su tamaño).
const PROP_COLISION: f32 = 0.5;
/// Altura sobre la superficie hasta la que puede haber objetos con los que chocar.
const PROPS_ALCANCE: f32 = 8.0;
/// Giro de 180 grados y escala para el modelo, los modelos vienen normalizados a radio 1
const AJUSTES_NAVE: &[(&str, bool, f32)] = &[("xwing", true, 2.0), ("Spaceship", true, 2.0)];
const ESCALA_NAVE: f32 = 1.2;

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
    /// Modelo del objeto de superficie contra el que chocó (`None` = chocó con el cuerpo mismo).
    pub object: Option<&'static str>,
    /// Punto de la superficie donde chocó.
    pub position: Vec3,
    /// Hacia afuera de la superficie.
    pub normal: Vec3,
    /// Qué tan rápido iba hacia el suelo.
    pub impact_speed: f32,
}

/// Qué se tragó a la nave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Causa {
    AgujeroNegro,
    Ovni,
}

impl Causa {
    fn mensaje(self) -> &'static str {
        match self {
            Causa::AgujeroNegro => "CAISTE EN UN AGUJERO NEGRO, YA NO PUEDES ESCAPAR",
            Causa::Ovni => "LOS ALIENS TE RAPTARON, YA NO PUEDES ESCAPAR",
        }
    }
}

/// La nave perdió: se muestra el cartel y la nave queda quieta hasta que termine.
#[derive(Clone, Copy, Debug)]
struct Derrota {
    causa: Causa,
    desde: f32,
    elimina: bool,
}

struct Model {
    name: String,
    mesh: Mesh,
    scale: f32,
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
    /// Perdió en plena carrera: con los aros que alcanzó y tiempo cuándo perdio.
    eliminado: bool,
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
    /// pierde al colisionar con ovni o agujero negro
    derrota: Option<Derrota>,
    /// Perdió en plena carrera: se queda quieta hasta la siguiente carrera o el menú.
    eliminado: bool,
    /// Entró a la carrera actual por su cuenta regresiva. Quien llega después, o está en persecución
    /// cuando se larga, no corre: espera en la sala a que termine.
    en_carrera: bool,
    /// modo carrera o persecucion
    modo: Modo,
    persecucion: Persecucion,
    /// Mejor tiempo sobrevivido en persecución.
    mejor_persecucion: Option<f32>,
    /// Segundos de carrera en que quedó eliminado.
    muerte_time: Option<f32>,
    slot: usize,
    my_id: Option<u32>,
    /// Mi nombre (ASCII, ceros al final); todo ceros = sin nombre.
    name: [u8; NAME_LEN],
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
    /// Malla del meteorito de cada planeta (radio 1) y de sus anillos (radio exterior 1).
    meteor_meshes: Vec<Option<Mesh>>,
    ring_meshes: Vec<Option<Mesh>>,
    /// Malla de cada agujero negro
    hole_meshes: Vec<Mesh>,
    /// Malla de cada ovni
    ufo_meshes: Vec<Mesh>,
    props_seed: u64,
    gravity_on: bool,
    view: ViewMode,
    orbit: Option<Orbit>,
    collisions: Vec<CollisionEvent>,
    sparks: Sparks,
    /// Aros cruzados
    gates_passed: u32,
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

fn nombre_mostrado(name: &[u8; NAME_LEN], por_defecto: String) -> String {
    let largo = name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
    if largo == 0 { por_defecto } else { String::from_utf8_lossy(&name[..largo]).into_owned() }
}

/// Color de un jugador: el de su nombre en la sala y el tinte de su nave.
fn color_jugador(id: u32) -> u32 {
    COLORES_JUGADORES[id as usize % COLORES_JUGADORES.len()]
}

fn escala_nave(escala: f32) -> Mat4 {
    Mat4::scale(Vec3::new(escala, escala, escala))
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut models: Vec<Model> = assets::MODELS
            .iter()
            .map(|(name, bytes)| match obj::parse_with_mtl(bytes, assets::model_mtl(name)) {
                Ok(m) => {
                    let (girar, scale) = AJUSTES_NAVE.iter().find(|a| a.0 == *name).map_or((false, ESCALA_NAVE), |a| (a.1, a.2));
                    let mesh = if girar { m.rotated_y_180().normalized() } else { m.normalized() };
                    Model { name: name.to_string(), mesh, scale }
                }
                Err(e) => Model { name: format!("{name} (ERROR: {e})"), mesh: Mesh::cube().normalized(), scale: ESCALA_NAVE },
            })
            .collect();
        if models.is_empty() {
            models.push(Model { name: "cubo (sin modelos)".into(), mesh: Mesh::cube().normalized(), scale: ESCALA_NAVE });
        }
        let selected = models.iter().position(|m| m.name == "xwing").unwrap_or(0);

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

        let modelo_aparte = |nombre: &str| {
            assets::PROPS.iter().find(|(n, _)| *n == nombre).and_then(|(_, b)| obj::parse(b).ok()).map(|m| m.normalized())
        };
        // Sin normales en el .obj, el meteorito se sombrea por caras y se ve anguloso como una roca.
        let meteor_meshes = PLANETS
            .iter()
            .map(|p| p.meteors.as_ref().map(|m| modelo_aparte(m.model).unwrap_or_else(|| procedural::icosphere(1))))
            .collect();
        let ring_meshes = PLANETS.iter().map(|p| p.rings.as_ref().and_then(|r| modelo_aparte(r.model))).collect();
        let ufo_meshes = UFOS
            .iter()
            .map(|u| {
                let mut mesh = assets::PROPS.iter().find(|(n, _)| *n == u.model).and_then(|(_, b)| obj::parse(b).ok()).unwrap_or_else(|| procedural::icosphere(2));
                mesh.recompute_normals();
                mesh
            })
            .collect();
        let hole_meshes = BLACK_HOLES.iter().map(|h| modelo_aparte(h.model).unwrap_or_else(|| procedural::torus(1.0, 0.2, 28, 8))).collect();

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
            derrota: None,
            eliminado: false,
            en_carrera: false,
            modo: Modo::Carrera,
            persecucion: Persecucion::new(),
            mejor_persecucion: None,
            muerte_time: None,
            slot: 0,
            my_id: None,
            name: [0; NAME_LEN],
            start_request: false,
            screen: Screen::Menu,
            prev_input: Input::default(),
            planet_lods,
            prop_meshes,
            prop_mesh_of,
            placements: Vec::new(),
            meteor_meshes,
            ring_meshes,
            hole_meshes,
            ufo_meshes,
            props_seed: PROPS_SEED,
            gravity_on: true,
            view: ViewMode::ThirdPerson,
            orbit: None,
            collisions: Vec::new(),
            sparks: Sparks::new(),
            gates_passed: 0,
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

    pub fn take_gates_passed(&mut self) -> u32 {
        std::mem::take(&mut self.gates_passed)
    }

    fn notice(&mut self, text: String, color: u32) {
        self.notice = Some((text, color, self.time + AVISO_S));
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
        self.sparks.update(dt);
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
                self.modo = Modo::Carrera;
            } else if input.chase {
                self.screen = Screen::Playing;
                self.iniciar_persecucion();
            }
        } else {
            if input.cycle_model {
                self.next_model(1);
            }
            if input.menu && (self.modo == Modo::Persecucion || self.esperando() || matches!(self.race.phase, Phase::Lobby | Phase::Results)) {
                self.back_to_menu();
            } else if input.start {
                if self.modo == Modo::Persecucion {
                    self.iniciar_persecucion();
                } else if self.esperando() {
                    // Hay una carrera en marcha en la que no corro: no se puede empezar otra.
                } else if self.online {
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
            if input.toggle_orbit && !(self.en_carrera && self.race.phase == Phase::Countdown) {
                self.toggle_orbit();
            }
        }
        if input.reseed {
            self.set_props_seed(self.props_seed + 1);
            self.notice(format!("SEMILLA {}", self.props_seed), 0x90A4AE);
        }

        self.race.update(dt, self.online);
        if self.race.phase != self.last_phase {
            if self.race.phase == Phase::Lobby {
                self.en_carrera = false;
            }
            // Solo juegan la carrera jugadores en sala de espera
            if self.race.phase == Phase::Countdown && self.screen == Screen::Playing && self.modo == Modo::Carrera {
                self.en_carrera = true;
                self.progress = 0;
                self.finish_time = None;
                self.eliminado = false;
                self.muerte_time = None;
                self.derrota = None;
                self.place_on_grid();
            }
            self.last_phase = self.race.phase;
        }

        // Pasado el cartel de "perdiste", vuelve a su casilla de salida, Si fue eliminada se queda quieta hasta la revancha o el menú.
        if let Some(d) = self.derrota {
            if self.time - d.desde >= DERROTA_S {
                self.derrota = None;
                if self.modo == Modo::Persecucion {
                    self.iniciar_persecucion();
                } else {
                    self.place_on_grid();
                }
            }
        }

        let quieta = self.screen == Screen::Menu
            || (self.en_carrera && self.race.phase == Phase::Countdown)
            || self.derrota.is_some();
        match self.orbit.filter(|_| !quieta) {
            Some(o) => self.fly_orbit(o, input, dt),
            None => {
                let ctrl = if quieta { Input::default() } else { *input };
                if self.gravity_on && !quieta {
                    self.ship.velocity += physics::gravity_at(self.ship.position, &self.gravity_sources()) * dt;
                }
                self.ship.update(&ctrl, dt);
                if !quieta {
                    self.resolve_collisions(dt);
                }
            }
        }
        if self.modo == Modo::Persecucion && self.screen == Screen::Playing && !quieta {
            self.update_persecucion(dt);
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
        self.modo = Modo::Carrera;
        self.derrota = None;
        self.eliminado = false;
        self.muerte_time = None;
        if !self.online {
            self.race.reset();
            self.last_phase = Phase::Lobby;
            self.en_carrera = false;
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

    // Si hay carrera en progreso y jugadores entran despues se mantiene en sala de espera
    pub fn esperando(&self) -> bool {
        matches!(self.last_phase, Phase::Countdown | Phase::Racing) && !self.en_carrera
    }

    /// Jugadores conectados que están en la sala o en la carrera 
    fn en_linea(&self) -> impl Iterator<Item = &RemotePlayer> {
        self.remotes.iter().filter(|r| r.target.progress & SOLO == 0)
    }

    // jugadores conectados en carrera o en sala de espera
    /// Los de mi grupo, para la tabla de la carrera: los que corren (eliminados incluidos) o, si espero, los que esperan.
    fn grupo(&self) -> impl Iterator<Item = &RemotePlayer> {
        let (solo, espero) = (self.modo == Modo::Persecucion, self.esperando());
        let en_marcha = matches!(self.last_phase, Phase::Countdown | Phase::Racing);
        self.en_linea().filter(move |r| !solo && (!en_marcha || (r.target.progress & ESPERA != 0) == espero))
    }

    /// Los que veo volar. Un eliminado vuelve a la sala y puede moverse: los que siguen corriendo
    /// no lo ven (solo figura en la tabla); otro eliminado, o quien espera, sí.
    fn rivales(&self) -> impl Iterator<Item = &RemotePlayer> {
        let corriendo = matches!(self.last_phase, Phase::Countdown | Phase::Racing) && self.en_carrera && !self.eliminado;
        self.grupo().filter(move |r| !(corriendo && r.target.progress & ELIMINADO != 0))
    }

    fn check_gates(&mut self) {
        if self.modo != Modo::Carrera || !self.en_carrera || self.race.phase != Phase::Racing || self.progress >= GATE_COUNT {
            return;
        }
        let gate = self.course[self.progress];
        if self.ship.position.distance(gate.pos) < gate.radius {
            self.progress += 1;
            self.gates_passed += 1;
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
            progress: self.progress as u8
                | if self.eliminado { ELIMINADO } else { 0 }
                | if self.modo == Modo::Persecucion {
                    SOLO
                } else if self.esperando() {
                    ESPERA
                } else {
                    0
                },
            finish_ms: self.finish_ms(),
            name: self.name,
        }
    }

    fn finish_ms(&self) -> u32 {
        self.finish_time.or(self.muerte_time).map_or(0, |t| ((t * 1000.0) as u32).max(1))
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
            self.en_carrera = false;
            self.last_phase = Phase::Lobby;
            self.slot = 0;
        }
    }

    pub fn remote_count(&self) -> usize {
        self.remotes.len()
    }

    pub fn set_name(&mut self, name: [u8; NAME_LEN]) {
        self.name = name;
    }

    pub fn set_my_id(&mut self, id: Option<u32>) {
        self.my_id = id;
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

    /// Texto para el título de la ventana.
    pub fn status(&self) -> String {
        let red = if self.online { format!("online: {} jugadores", 1 + self.en_linea().count()) } else { "sin conexion".to_string() };
        format!("Space Travel | nave: {} (Tab cambia) | vel {:.0} u/s | {red}", self.models[self.selected].name, self.ship.speed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn rival(id: u32, x: f32) -> RemoteShip {
        RemoteShip { id, position: Vec3::new(x, 0.0, -10.0), yaw: 0.0, pitch: 0.0, model: 0, progress: 0, finish_ms: 0, name: [0; NAME_LEN] }
    }

    pub(crate) fn avanzar(g: &mut Game, segundos: f32) {
        for _ in 0..(segundos / 0.05).ceil() as usize {
            g.update(0.05, &Input::default());
        }
    }

    pub(crate) fn empezar() -> Input {
        Input { start: true, ..Default::default() }
    }

    /// Un juego que ya salió del menú.
    pub(crate) fn jugando() -> Game {
        let mut g = Game::new(1);
        g.update(0.05, &empezar());
        g
    }

    /// Un `Input` sin nada presionado, para armar otros con `..NADA`.
    pub(crate) const NADA: Input = Input {
        forward: false, backward: false, yaw_left: false, yaw_right: false, pitch_up: false, pitch_down: false,
        brake: false, cycle_model: false, start: false, menu: false, toggle_gravity: false, toggle_orbit: false,
        toggle_view: false, reseed: false, chase: false,
    };

    /// Coloca la nave en `pos`, quieta, con la cámara detrás.
    pub(crate) fn poner(g: &mut Game, pos: Vec3) {
        g.ship.reset(pos, 0.0, 0.0);
        g.camera = ChaseCamera::new(&g.ship);
    }

    pub(crate) fn aqua(g: &Game) -> (Vec3, f32) {
        let i = PLANETS.iter().position(|p| p.name == "AQUA").unwrap();
        (scenario::planet_position(&PLANETS[i], g.world_time), PLANETS[i].radius)
    }

    pub(crate) fn avanzar_con(g: &mut Game, segundos: f32, i: Input) {
        for _ in 0..(segundos / 0.05).ceil() as usize {
            g.update(0.05, &i);
        }
    }

    #[test]
    fn dt_enorme_se_limita() {
        let mut g = Game::new(1);
        g.update(10.0, &Input::default());
        assert!(g.time <= DT_MAX + 1e-6);
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
        let mut g = jugando(); // en la sala: solo ahí se entra a la carrera
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
    fn si_el_servidor_larga_el_que_esta_en_el_menu_no_entra_a_la_carrera() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_race_server(1, 0);
        g.update(0.01, &Input::default());
        assert_eq!((g.screen(), g.phase()), (Screen::Menu, Phase::Countdown), "sigue en el menú");
        assert!(g.esperando() && g.local_state().progress & ESPERA != 0, "avisa que no corre");
        g.update(0.01, &empezar());
        assert_eq!(g.screen(), Screen::Playing);
        assert!(g.esperando(), "si entra a la sala con la carrera en marcha, espera");
        g.set_race_server(Phase::Racing as u8, 1000);
        g.update(0.01, &Input::default());
        assert!(g.esperando() && g.progress() == 0);
    }

    #[test]
    fn si_el_servidor_larga_el_que_esta_en_la_sala_entra_a_la_carrera() {
        let mut g = jugando();
        g.set_online(true);
        g.set_race_server(1, 0);
        g.update(0.01, &Input::default());
        assert_eq!((g.screen(), g.phase()), (Screen::Playing, Phase::Countdown));
        assert!(!g.esperando());
        assert_eq!(g.local_state().progress & (ESPERA | SOLO), 0);
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
    fn r_cambia_la_semilla_y_reacomoda_los_objetos() {
        let mut g = jugando();
        let i = PLANETS.iter().position(|p| !p.props.is_empty()).unwrap();
        let antes = g.placements[i][0].base;
        let semilla = g.props_seed();
        g.update(0.05, &Input { reseed: true, ..NADA });
        assert_eq!(g.props_seed(), semilla + 1);
        assert_ne!(g.placements[i][0].base, antes);
    }

    /// Un cliente online que ya está jugando, con la carrera del servidor en la fase `fase`.
    fn online_en(fase: Phase) -> Game {
        let mut g = jugando();
        g.set_online(true);
        g.set_race_server(fase as u8, 0);
        g.update(0.05, &Input::default());
        g
    }

    /// Un cliente online que vio arrancar la carrera desde la cuenta regresiva y ya corre.
    fn corriendo() -> Game {
        let mut g = online_en(Phase::Countdown);
        g.set_race_server(Phase::Racing as u8, 1000);
        g.update(0.05, &Input::default());
        g
    }

    #[test]
    fn quien_llega_con_la_carrera_empezada_espera_en_la_sala_y_no_corre() {
        let mut g = online_en(Phase::Racing);
        assert!(g.esperando(), "no entra como tardío");
        assert!(g.local_state().progress & ESPERA != 0, "avisa al servidor que no corre");
        g.ship.position = g.course[0].pos;
        g.update(0.05, &Input::default());
        assert_eq!(g.progress(), 0, "los aros no le cuentan");

        g.update(0.05, &empezar());
        assert!(!g.take_start_request(), "no puede pedir otra carrera encima");
        g.render(&mut Framebuffer::new(640, 360)); // con el aviso de carrera en curso

        g.set_race_server(Phase::Results as u8, 30_000);
        g.update(0.05, &Input::default());
        assert!(!g.esperando(), "cuando termina vuelve a ser uno más de la sala");
        assert_eq!(g.local_state().progress & ESPERA, 0);
    }

    #[test]
    fn quien_vio_la_cuenta_regresiva_corre_normalmente() {
        let g = corriendo();
        assert!(!g.esperando());
        assert_eq!(g.local_state().progress & (ESPERA | SOLO), 0);
    }

    #[test]
    fn la_persecucion_no_se_interrumpe_si_otros_largan_una_carrera_y_no_cuenta_como_tardio() {
        let mut g = Game::new(1);
        g.update(0.05, &Input { chase: true, ..NADA });
        g.set_online(true);
        g.set_race_server(Phase::Countdown as u8, 0);
        g.update(0.05, &Input::default());
        assert_eq!(g.mode(), Modo::Persecucion, "sigue en persecución");
        assert!(g.local_state().progress & SOLO != 0);
        let antes = g.ship.position;
        g.update(0.05, &Input { forward: true, ..NADA });
        g.update(0.05, &Input { forward: true, ..NADA });
        assert!(g.ship.position.distance(antes) > 1e-4, "la cuenta regresiva de otros no la congela");

        g.set_race_server(Phase::Racing as u8, 1000);
        g.update(0.05, &Input::default());
        g.ship.position = g.course[0].pos;
        g.update(0.05, &Input::default());
        assert_eq!(g.progress(), 0, "los aros no cuentan en persecución");

        // Al salir con la carrera en marcha no entra tarde: queda esperando en la sala.
        g.update(0.05, &Input { menu: true, ..NADA });
        g.update(0.05, &empezar());
        assert_eq!((g.screen(), g.mode()), (Screen::Playing, Modo::Carrera));
        assert!(g.esperando());
    }

    #[test]
    fn cada_uno_ve_solo_a_los_de_su_grupo() {
        let corre = RemoteShip { progress: 2, ..rival(1, 0.0) };
        let persigue = RemoteShip { progress: SOLO, ..rival(2, 0.0) };
        let espera = RemoteShip { progress: ESPERA, ..rival(3, 0.0) };
        let todos = [corre, persigue, espera];
        let ids = |g: &Game| g.rivales().map(|r| r.target.id).collect::<Vec<_>>();

        let mut g = corriendo();
        g.set_remote_players(&todos);
        assert_eq!(ids(&g), [1], "el que corre solo ve a los que corren");
        assert_eq!(g.entries().len(), 2, "y el ranking solo los incluye a ellos");

        let mut g = online_en(Phase::Racing);
        g.set_remote_players(&todos);
        assert_eq!(ids(&g), [3], "el que espera solo ve a los que esperan");

        let mut g = Game::new(1);
        g.update(0.05, &Input { chase: true, ..NADA });
        g.set_remote_players(&todos);
        assert!(ids(&g).is_empty(), "en persecución no se ve a nadie");

        let mut g = online_en(Phase::Lobby);
        g.set_remote_players(&todos);
        assert_eq!(ids(&g), [1, 3], "en la sala se ven todos menos los de persecución");
    }

    #[test]
    fn el_eliminado_vuela_libre_pero_los_que_corren_solo_lo_ven_en_la_tabla() {
        let eliminado = RemoteShip { progress: 3 | ELIMINADO, finish_ms: 8000, ..rival(1, 0.0) };
        let corre = RemoteShip { progress: 5, ..rival(2, 0.0) };

        // Yo corro: el eliminado sale en la tabla pero su nave no se ve.
        let mut g = corriendo();
        g.set_remote_players(&[eliminado, corre]);
        assert_eq!(g.rivales().map(|r| r.target.id).collect::<Vec<_>>(), [2]);
        assert_eq!(g.entries().len(), 3, "yo + el eliminado + el que corre");

        // Yo también estoy eliminado: ya no corro, vuelo libre y veo a todos.
        g.derrotar(Causa::Ovni);
        avanzar(&mut g, DERROTA_S + 0.2);
        assert!(g.eliminado && g.derrota.is_none());
        let antes = g.ship.position;
        avanzar_con(&mut g, 1.0, Input { forward: true, ..NADA });
        assert!(g.ship.position.distance(antes) > 5.0, "después del cartel puedo volar");
        assert_eq!(g.rivales().count(), 2);
    }

    #[test]
    fn perder_con_la_carrera_en_marcha_solo_elimina_a_quien_corre() {
        let mut g = online_en(Phase::Racing);
        g.derrotar(Causa::Ovni);
        assert!(!g.eliminado, "el que espera no queda eliminado");
        let mut g = corriendo();
        g.derrotar(Causa::Ovni);
        assert!(g.eliminado);
    }
}
