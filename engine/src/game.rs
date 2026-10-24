use crate::{
    assets,
    camera::ChaseCamera,
    input::Input,
    obj, raster,
    remote::{RemotePlayer, RemoteShip},
    rng::Rng,
    Framebuffer, Mat4, Mesh, Ship, Vec3,
};
use std::f32::consts::PI;

const FOV_Y: f32 = 60.0 * PI / 180.0;
const FONDO: u32 = 0x02020A;
const LUZ: Vec3 = Vec3::new(-0.4, -0.7, -0.5);
const DT_MAX: f32 = 0.05; // evita saltos al volver de una pestaña inactiva
const COLOR_PROPIO: u32 = 0xDDDDDD;
const COLORES_JUGADORES: [u32; 6] = [0x42A5F5, 0xEF5350, 0x66BB6A, 0xFFCA28, 0xAB47BC, 0x26C6DA];

struct Model {
    name: String,
    mesh: Mesh,
}

struct Marker {
    pos: Vec3,
    size: f32,
    color: u32,
}

struct Star {
    dir: Vec3,
    color: u32,
}

/// Estado del juego. Native y web llaman a la misma lógica: `update(dt, input)` y `render(fb)`.
pub struct Game {
    /// Segundos acumulados; todo movimiento depende de esto, no de los cuadros.
    pub time: f32,
    ship: Ship,
    camera: ChaseCamera,
    models: Vec<Model>,
    selected: usize,
    cube: Mesh,
    markers: Vec<Marker>,
    stars: Vec<Star>,
    remotes: Vec<RemotePlayer>,
    online: bool,
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
        let paleta = [0xE53935, 0x43A047, 0x1E88E5, 0xFDD835, 0x8E24AA, 0xFB8C00];
        let mut markers = Vec::new();
        while markers.len() < 150 {
            let pos = Vec3::new(rng.range(-120.0, 120.0), rng.range(-40.0, 40.0), rng.range(-120.0, 120.0));
            if pos.length() < 10.0 {
                continue;
            }
            let color = paleta[rng.next_u32() as usize % paleta.len()];
            markers.push(Marker { pos, size: rng.range(1.0, 3.0), color });
        }

        let stars = (0..300)
            .map(|_| {
                let (z, a) = (rng.range(-1.0, 1.0), rng.range(0.0, 2.0 * PI));
                let r = (1.0 - z * z).sqrt();
                let g = rng.range(90.0, 255.0) as u32;
                Star { dir: Vec3::new(r * a.cos(), z, r * a.sin()), color: (g << 16) | (g << 8) | g }
            })
            .collect();

        let ship = Ship::new();
        let camera = ChaseCamera::new(&ship);
        Self {
            time: 0.0,
            ship,
            camera,
            models,
            selected,
            cube: Mesh::cube(),
            markers,
            stars,
            remotes: Vec::new(),
            online: false,
        }
    }

    pub fn update(&mut self, dt: f32, input: &Input) {
        let dt = dt.clamp(0.0, DT_MAX);
        self.time += dt;
        if input.cycle_model {
            self.selected = (self.selected + 1) % self.models.len();
        }
        self.ship.update(input, dt);
        self.camera.update(&self.ship, dt);
        for r in &mut self.remotes {
            r.update(dt);
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
        }
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
        }
    }

    pub fn remote_count(&self) -> usize {
        self.remotes.len()
    }

    pub fn render(&self, fb: &mut Framebuffer) {
        fb.clear(FONDO);
        let aspect = fb.width as f32 / fb.height as f32;
        let proj = Mat4::perspective(FOV_Y, aspect, 0.1, 500.0);
        let view = self.camera.view();

        // Estrellas: solo rotan con la cámara, no se acercan al moverte.
        let sky = proj * view.without_translation();
        for s in &self.stars {
            raster::draw_point(fb, &sky, s.dir, s.color);
        }

        let vp = proj * view;
        for m in &self.markers {
            let model = Mat4::translation(m.pos) * Mat4::scale(Vec3::new(m.size, m.size, m.size));
            raster::draw_mesh(fb, &self.cube, &model, &vp, m.color, LUZ);
        }

        for r in &self.remotes {
            let s = &r.shown;
            let mesh = &self.models[s.model as usize % self.models.len()].mesh;
            let model = Mat4::translation(s.position) * Mat4::rotation_y(s.yaw) * Mat4::rotation_x(s.pitch);
            let color = COLORES_JUGADORES[s.id as usize % COLORES_JUGADORES.len()];
            raster::draw_mesh(fb, mesh, &model, &vp, color, LUZ);
        }

        let model = Mat4::translation(self.ship.position) * self.ship.rotation();
        raster::draw_mesh(fb, &self.models[self.selected].mesh, &model, &vp, COLOR_PROPIO, LUZ);
    }

    /// Texto para el título de la ventana.
    pub fn status(&self) -> String {
        let red = if self.online {
            format!("online: {} jugadores", 1 + self.remotes.len())
        } else {
            "sin conexión".to_string()
        };
        format!(
            "Space Travel | nave: {} (Tab cambia) | vel {:.1} u/s | {red}",
            self.models[self.selected].name,
            self.ship.speed()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rival(id: u32, x: f32) -> RemoteShip {
        RemoteShip { id, position: Vec3::new(x, 0.0, -10.0), yaw: 0.0, pitch: 0.0, model: 0 }
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
    fn renderiza_sin_paniquear() {
        let mut g = Game::new(1);
        let mut fb = Framebuffer::new(160, 120);
        g.update(0.016, &Input { forward: true, ..Default::default() });
        g.render(&mut fb);
        assert!(fb.pixels.iter().any(|&p| p != FONDO));
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
        let mut sin = Framebuffer::new(320, 240);
        g.render(&mut sin);
        g.set_remote_players(&[rival(3, 4.0)]);
        let mut con = Framebuffer::new(320, 240);
        g.render(&mut con);
        assert_ne!(sin.pixels, con.pixels);
    }

    #[test]
    fn el_estado_local_refleja_la_nave() {
        let mut g = Game::new(1);
        g.update(0.05, &Input { forward: true, ..Default::default() });
        assert!(g.local_state().position.z < 0.0);
    }
}
