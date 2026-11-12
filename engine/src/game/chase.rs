//! Modo ataque de un jugador: salen 5 ovnis a la vez que persiguen a la nave y hay que destruirlos a disparos.
use super::*;

pub(super) const CAZADOR_RADIO: f32 = 7.0;
const CAZADOR_DISTANCIA: f32 = 160.0;
const CAZADOR_DISPERSION: f32 = 60.0;
pub(super) const OVNIS_ATAQUE: usize = 5;
/// Disparos que aguanta cada ovni.
pub(super) const OVNI_VIDA: u32 = 6;
/// Segundos al inicio en que los ovnis todavía no se mueven.
pub(super) const CAZADOR_GRACIA_S: f32 = 3.0;
const CAZADOR_VEL_INICIAL: f32 = 35.0;
const CAZADOR_VEL_FINAL: f32 = 80.0;
const CAZADOR_RAMPA_S: f32 = 90.0;
const CAZADOR_GIRO: f32 = 1.3;
pub(super) const CAZADOR_PELIGRO: f32 = 90.0;

/// Modo de juego elegido
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modo {
    Carrera,
    Ataque,
}

/// Un ovni que persigue a la nave.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cazador {
    pub(super) position: Vec3,
    rumbo: Vec3,
    /// Disparos que le faltan para caer.
    pub(super) vida: u32,
}

#[derive(Clone, Debug)]
pub(super) struct Ataque {
    pub(super) tiempo: f32,
    pub(super) ovnis: Vec<Cazador>,
    pub(super) destruidos: u32,
}

impl Ataque {
    pub(super) fn new() -> Self {
        Self { tiempo: 0.0, ovnis: Vec::new(), destruidos: 0 }
    }

    pub(super) fn en_gracia(&self) -> bool {
        self.tiempo < CAZADOR_GRACIA_S
    }

    pub(super) fn segundos_de_gracia(&self) -> f32 {
        (CAZADOR_GRACIA_S - self.tiempo).max(0.0)
    }

    /// Velocidad de los ovnis: suben poco a poco desde que termina la gracia.
    fn velocidad(&self) -> f32 {
        let k = ((self.tiempo - CAZADOR_GRACIA_S) / CAZADOR_RAMPA_S).clamp(0.0, 1.0);
        CAZADOR_VEL_INICIAL + (CAZADOR_VEL_FINAL - CAZADOR_VEL_INICIAL) * k
    }

    pub(super) fn distancia_minima(&self, p: Vec3) -> Option<f32> {
        self.ovnis.iter().map(|o| o.position.distance(p)).min_by(f32::total_cmp)
    }
}

impl Game {
    pub fn mode(&self) -> Modo {
        self.modo
    }

    pub(super) fn iniciar_ataque(&mut self) {
        self.modo = Modo::Ataque;
        self.derrota = None;
        self.place_on_grid();
        self.ataque = Ataque::new();
        for _ in 0..OVNIS_ATAQUE {
            self.sumar_ovni();
        }
    }

    /// Un ovni más, delante de la nave y a la vista
    fn sumar_ovni(&mut self) {
        let n = self.ataque.ovnis.len();
        let f = self.ship.forward();
        let derecha = match f.cross(Vec3::Y).normalize() {
            d if d.length() > 0.5 => d,
            _ => Vec3::new(1.0, 0.0, 0.0),
        };
        let arriba = derecha.cross(f);
        let a = n as f32 * 2.4; // ángulo áureo: cada uno cae lejos de los anteriores
        let desvio = if n == 0 { Vec3::ZERO } else { (derecha * a.cos() + arriba * a.sin()) * CAZADOR_DISPERSION };
        let position = self.sacar_de_cuerpos(self.ship.position + f * CAZADOR_DISTANCIA + desvio);
        let rumbo = (self.ship.position - position).normalize();
        self.ataque.ovnis.push(Cazador { position, rumbo, vida: OVNI_VIDA });
    }

    pub(super) fn cazador_cercania(&self) -> f32 {
        if self.modo != Modo::Ataque {
            return 0.0;
        }
        self.ataque
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

    /// Avanza el reloj y los ovnis; si alguno alcanza a la nave, pierde.
    pub(super) fn update_ataque(&mut self, dt: f32) {
        self.ataque.tiempo += dt;
        if self.ataque.en_gracia() {
            return;
        }
        let nave = self.ship.position;
        let vel = self.ataque.velocidad();
        let mut ovnis = std::mem::take(&mut self.ataque.ovnis);
        let mut atrapado = None;
        for o in ovnis.iter_mut() {
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
        self.ataque.ovnis = ovnis;

        if let Some(contacto) = atrapado {
            self.registrar_choque("OVNI", None, contacto);
        }
    }

    /// Cada disparo que pega en un ovni le quita un punto de vida; en 0 el ovni se destruye.
    pub(super) fn shots_vs_ovnis(&mut self, dt: f32) {
        if self.shots.is_empty() || self.ataque.ovnis.is_empty() {
            return;
        }
        let mut ovnis = std::mem::take(&mut self.ataque.ovnis);
        let mut impactos = Vec::new();
        self.shots.retain(|s| {
            let (a, b) = s.tramo(dt);
            match ovnis.iter_mut().find(|o| o.vida > 0 && shots::segment_hits_sphere(a, b, o.position, CAZADOR_RADIO)) {
                Some(o) => {
                    o.vida -= 1;
                    impactos.push((b, (b - o.position).normalize(), o.vida == 0));
                    false
                }
                None => true,
            }
        });
        for (pos, normal, destruido) in impactos {
            self.sparks.burst(pos, normal, Vec3::ZERO, if destruido { 60.0 } else { 6.0 });
            if destruido {
                self.ataque.destruidos += 1;
                self.notice("OVNI DESTRUIDO".into(), 0x66BB6A);
            }
        }
        ovnis.retain(|o| o.vida > 0);
        self.ataque.ovnis = ovnis;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::*;

    #[test]
    fn salen_los_cinco_y_a_disparos_se_destruye_el_de_enfrente() {
        let mut g = Game::new(1);
        g.update(0.05, &Input { chase: true, ..NADA });
        assert_eq!(g.ataque.ovnis.len(), OVNIS_ATAQUE);
        // Durante la gracia los ovnis no se mueven: el de enfrente recibe todos los disparos.
        avanzar_con(&mut g, 2.5, Input { fire: true, ..NADA });
        assert_eq!(g.ataque.destruidos, 1);
        assert_eq!(g.ataque.ovnis.len(), OVNIS_ATAQUE - 1);
    }
}
