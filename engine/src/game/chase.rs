//Modo persecución de un jugador, ovnis persiguen y hay uno nuevo cada 10 segundos, hasta max 5 y hay que sobrevivir el mayor tiempo
use super::*;

pub(super) const CAZADOR_RADIO: f32 = 7.0;
const CAZADOR_DISTANCIA: f32 = 160.0;
const CAZADOR_DISPERSION: f32 = 50.0;
pub(super) const NUEVO_OVNI_CADA_S: f32 = 10.0;
pub(super) const MAX_OVNIS: usize = 5;
pub(super) const CAZADOR_GRACIA_S: f32 = 3.0;
const CAZADOR_GRACIA_NUEVO_S: f32 = 1.0;
const CAZADOR_VEL_INICIAL: f32 = 35.0;
const CAZADOR_VEL_FINAL: f32 = 80.0;
const CAZADOR_RAMPA_S: f32 = 90.0;
const CAZADOR_GIRO: f32 = 1.3;
pub(super) const CAZADOR_PELIGRO: f32 = 90.0;

/// Modo de juego elegido 
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modo {
    Carrera,
    Persecucion,
}

/// Un ovni que persigue a la nave.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cazador {
    pub(super) position: Vec3,
    rumbo: Vec3,
    edad: f32,
}

impl Cazador {
    /// Velocidad que le toca a los `t` segundos de haber empezado a moverse.
    fn velocidad_en(t: f32) -> f32 {
        let k = (t / CAZADOR_RAMPA_S).clamp(0.0, 1.0);
        CAZADOR_VEL_INICIAL + (CAZADOR_VEL_FINAL - CAZADOR_VEL_INICIAL) * k
    }

    fn en_gracia(&self) -> bool {
        self.edad < CAZADOR_GRACIA_S
    }
}

#[derive(Clone, Debug)]
pub(super) struct Persecucion {
    /// Segundos sobrevividos
    pub(super) tiempo: f32,
    pub(super) ovnis: Vec<Cazador>,
}

impl Persecucion {
    pub(super) fn new() -> Self {
        Self { tiempo: 0.0, ovnis: Vec::new() }
    }

    pub(super) fn proximo_en(&self) -> Option<f32> {
        (self.ovnis.len() < MAX_OVNIS).then(|| NUEVO_OVNI_CADA_S - self.tiempo % NUEVO_OVNI_CADA_S)
    }

    pub(super) fn en_gracia(&self) -> bool {
        self.tiempo < CAZADOR_GRACIA_S
    }

    pub(super) fn segundos_de_gracia(&self) -> f32 {
        (CAZADOR_GRACIA_S - self.tiempo).max(0.0)
    }

    pub(super) fn distancia_minima(&self, p: Vec3) -> Option<f32> {
        self.ovnis.iter().map(|o| o.position.distance(p)).min_by(f32::total_cmp)
    }
}

impl Game {
    pub fn mode(&self) -> Modo {
        self.modo
    }

    pub fn best_chase_time(&self) -> Option<f32> {
        self.mejor_persecucion
    }

    /// Cuántos ovnis persiguen ahora.
    pub fn chasers(&self) -> usize {
        self.persecucion.ovnis.len()
    }

    pub(super) fn iniciar_persecucion(&mut self) {
        self.modo = Modo::Persecucion;
        self.derrota = None;
        self.place_on_grid();
        self.persecucion = Persecucion::new();
        self.sumar_ovni();
    }

    /// Un ovni más, delante de la nave y a la vista
    fn sumar_ovni(&mut self) {
        let n = self.persecucion.ovnis.len();
        let f = self.ship.forward();
        let derecha = match f.cross(Vec3::Y).normalize() {
            d if d.length() > 0.5 => d,
            _ => Vec3::new(1.0, 0.0, 0.0),
        };
        let arriba = derecha.cross(f);
        let a = n as f32 * 2.4; // ángulo áureo: cada uno cae lejos de los anteriores
        let desvio = if n == 0 { Vec3::ZERO } else { (derecha * a.cos() + arriba * a.sin()) * CAZADOR_DISPERSION };
        let position = self.sacar_de_cuerpos(self.ship.position + f * CAZADOR_DISTANCIA + desvio);
        let edad = if n == 0 { 0.0 } else { CAZADOR_GRACIA_S - CAZADOR_GRACIA_NUEVO_S };
        let rumbo = (self.ship.position - position).normalize();
        self.persecucion.ovnis.push(Cazador { position, rumbo, edad });
    }

    pub(super) fn cazador_cercania(&self) -> f32 {
        if self.modo != Modo::Persecucion {
            return 0.0;
        }
        self.persecucion
            .distancia_minima(self.ship.position)
            .map_or(0.0, |d| (1.0 - (d - CAZADOR_RADIO) / OVNI_ALCANCE).clamp(0.0, 1.0))
    }

    fn sacar_de_cuerpos(&self, mut p: Vec3) -> Vec3 {
        for (i, b) in self.bodies().iter().enumerate() {
            let limite = self.surface_at(i, b, p) + CAZADOR_RADIO;
            let d = p - b.center;
            if d.length() < limite {
                p = b.center + (if d.length() > 1e-6 { d.normalize() } else { Vec3::Y }) * limite;
            }
        }
        for h in BLACK_HOLES.iter() {
            let limite = h.core_radius() + CAZADOR_RADIO;
            let d = p - h.position;
            if d.length() < limite {
                p = h.position + (if d.length() > 1e-6 { d.normalize() } else { Vec3::Y }) * limite;
            }
        }
        p
    }

    /// Avanza el reloj y los ovnis, suma uno nuevo cada 10 s y, si alguno alcanza a la nave, pierde.
    pub(super) fn update_persecucion(&mut self, dt: f32) {
        self.persecucion.tiempo += dt;
        let esperados = ((self.persecucion.tiempo / NUEVO_OVNI_CADA_S) as usize + 1).min(MAX_OVNIS);
        while self.persecucion.ovnis.len() < esperados {
            self.sumar_ovni();
        }

        let nave = self.ship.position;
        let mut ovnis = std::mem::take(&mut self.persecucion.ovnis);
        let mut atrapado = None;
        for o in ovnis.iter_mut() {
            o.edad += dt;
            if o.en_gracia() {
                continue;
            }
            let vel = Cazador::velocidad_en(o.edad - CAZADOR_GRACIA_S);
            let deseado = (nave - o.position).normalize();
            let k = 1.0 - (-CAZADOR_GIRO * dt).exp();
            let mezcla = (o.rumbo + (deseado - o.rumbo) * k).normalize();
            o.rumbo = if mezcla.length() > 0.5 { mezcla } else { deseado };
            o.position = self.sacar_de_cuerpos(o.position + o.rumbo * (vel * dt));

            let d = nave - o.position;
            if atrapado.is_none() && d.length() < CAZADOR_RADIO + NAVE_RADIO {
                let normal = if d.length() > 1e-6 { d.normalize() } else { Vec3::Y };
                atrapado = Some(physics::Contact { normal, impact_speed: vel.max(IMPACTO_MIN * 2.0), body_velocity: o.rumbo * vel });
            }
        }
        separar(&mut ovnis);
        self.persecucion.ovnis = ovnis;

        if let Some(contacto) = atrapado {
            self.registrar_choque("OVNI", None, contacto);
        }
    }
}

// Los ovnis se empujan
fn separar(ovnis: &mut [Cazador]) {
    let minimo = 2.0 * CAZADOR_RADIO;
    for i in 0..ovnis.len() {
        for j in (i + 1)..ovnis.len() {
            let d = ovnis[i].position - ovnis[j].position;
            let dist = d.length();
            if dist < minimo {
                let dir = if dist > 1e-6 { d * (1.0 / dist) } else { Vec3::Y };
                let empuje = dir * ((minimo - dist) * 0.5);
                ovnis[i].position += empuje;
                ovnis[j].position += -empuje;
            }
        }
    }
}
