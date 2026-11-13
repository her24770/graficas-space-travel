//! Modo pelea entre jugadores (solo online): todos salen en círculo en la arena, se disparan y
//! gana el último en pie. Cada cliente lleva su propia vida; cuando uno de mis rayos le pega a un
//! rival, aviso al servidor, que lo valida y se lo reenvía a la víctima para que se quite vida.
use super::*;
use crate::remote::{DISPARANDO, PELEA, PELEA_ESPERA, PELEA_FUERA};

/// Vida que quita cada rayo que pega (la nave tiene 100).
pub(super) const DANO_DISPARO: f32 = 10.0;
/// Jugadores en la sala de pelea que hacen falta para empezar (lo mismo que pide el servidor).
pub const MIN_PELEA: usize = 2;
/// Los rayos de los rivales se dibujan de este color, distinto de los míos.
const COLOR_RAYO_RIVAL: u32 = 0xFF6E6E;

/// Un renglón de la tabla de la pelea.
pub(super) struct Peleador {
    pub(super) label: String,
    pub(super) color: u32,
    pub(super) kills: u8,
    pub(super) vivo: bool,
}

impl Game {
    pub(super) fn iniciar_pelea(&mut self) {
        self.modo = Modo::Pelea;
        self.derrota = None;
        self.fuera = false;
        self.en_pelea = false;
        self.place_on_grid();
    }

    /// Estoy vivo en plena pelea: mis rayos dañan y me pueden dañar.
    pub(super) fn peleando(&self) -> bool {
        self.modo == Modo::Pelea && self.en_pelea && !self.fuera && self.pelea.phase == Phase::Racing
    }

    /// Entré al modo pelea con una pelea ya empezada: miro sin participar.
    pub(super) fn esperando_pelea(&self) -> bool {
        self.modo == Modo::Pelea && matches!(self.pelea.phase, Phase::Countdown | Phase::Racing) && !self.en_pelea
    }

    /// En la cuenta regresiva o vivo en la pelea: no se puede salir al menú.
    pub(super) fn atado_a_la_pelea(&self) -> bool {
        self.modo == Modo::Pelea && self.en_pelea && !self.fuera && matches!(self.pelea.phase, Phase::Countdown | Phase::Racing)
    }

    /// Bits de la pelea que van en mi estado.
    pub(super) fn bits_pelea(&self) -> u8 {
        if self.modo != Modo::Pelea {
            return 0;
        }
        PELEA
            | if self.fuera { PELEA_FUERA } else { 0 }
            | if self.esperando_pelea() { PELEA_ESPERA } else { 0 }
            | if self.disparando { DISPARANDO } else { 0 }
    }

    /// ¿Veo a este rival? Mientras peleo, solo a los que siguen vivos en la pelea; si me derribaron,
    /// llegué tarde o estoy en la sala, a todos los del modo pelea.
    pub(super) fn ve_en_pelea(&self, r: &RemotePlayer) -> bool {
        let f = r.target.fight;
        f & PELEA != 0 && (!self.atado_a_la_pelea() || f & (PELEA_FUERA | PELEA_ESPERA) == 0)
    }

    /// Cuántos están en la sala de pelea, contándome.
    pub(super) fn en_sala_de_pelea(&self) -> usize {
        1 + self.remotes.iter().filter(|r| r.target.fight & PELEA != 0 && r.target.fight & PELEA_ESPERA == 0).count()
    }

    /// Enter en el modo pelea: pide empezar si se puede, o explica por qué no.
    pub(super) fn pedir_pelea(&mut self) {
        if !self.online {
            self.notice("LA PELEA ES ONLINE: ABRE EL JUEGO EN LA WEB".into(), 0xEF9A9A);
        } else if !matches!(self.pelea.phase, Phase::Lobby | Phase::Results) {
            // Hay una pelea en marcha: hay que esperar a que termine.
        } else if self.en_sala_de_pelea() < MIN_PELEA {
            self.notice("FALTA OTRO JUGADOR EN LA SALA DE PELEA".into(), 0xEF9A9A);
        } else {
            self.fight_request = true;
        }
    }

    /// Cambios de fase de la pelea (las dicta el servidor).
    pub(super) fn update_fases_pelea(&mut self) {
        if self.pelea.phase == self.last_pelea {
            return;
        }
        match self.pelea.phase {
            // Quien está jugando en el modo pelea cuando empieza la cuenta, entra.
            Phase::Countdown if self.screen == Screen::Playing && self.modo == Modo::Pelea => {
                self.en_pelea = true;
                self.fuera = false;
                self.derrota = None;
                self.derribado_por = None;
                self.place_on_grid();
            }
            Phase::Lobby => {
                self.en_pelea = false;
                self.fuera = false;
            }
            _ => {}
        }
        self.last_pelea = self.pelea.phase;
    }

    /// Mis rayos contra los rivales vivos. Cada impacto se avisa al servidor.
    pub(super) fn shots_vs_rivales(&mut self, dt: f32) {
        if !self.peleando() || self.shots.is_empty() {
            return;
        }
        let n = self.models.len();
        let blancos: Vec<(u32, Vec3, f32)> = self
            .rivales()
            .map(|r| (r.target.id, r.shown.position, self.models[r.shown.model as usize % n].scale.max(1.5)))
            .collect();
        let mut impactos = Vec::new();
        self.shots.retain(|s| {
            let (a, b) = s.tramo(dt);
            match blancos.iter().find(|(_, c, r)| shots::segment_hits_sphere(a, b, *c, *r)) {
                Some(&(id, c, _)) => {
                    impactos.push((id, b, (b - c).normalize()));
                    false
                }
                None => true,
            }
        });
        for (id, pos, normal) in impactos {
            self.sparks.burst(pos, normal, Vec3::ZERO, 8.0);
            self.golpes.push(id);
        }
    }

    /// Los rivales que disparan sueltan rayos (solo se dibujan; el daño lo decide quien dispara).
    pub(super) fn update_rayos_rivales(&mut self, dt: f32) {
        let disparan: Vec<u32> = self.rivales().filter(|r| r.target.fight & DISPARANDO != 0).map(|r| r.target.id).collect();
        for r in &mut self.remotes {
            if !disparan.contains(&r.target.id) {
                r.recarga = 0.0;
                continue;
            }
            r.recarga -= dt;
            if r.recarga <= 0.0 {
                r.recarga = shots::CADENCIA_S;
                let rot = Mat4::rotation_y(r.shown.yaw) * Mat4::rotation_x(r.shown.pitch);
                let f = rot.transform_dir(Vec3::new(0.0, 0.0, -1.0)).normalize();
                self.rayos_rivales.spawn(r.shown.position + f * DISPARO_SALIDA, f, Vec3::ZERO);
            }
        }
        self.rayos_rivales.update(dt);
    }

    /// Impactos que mandó el servidor: los que me pegaron me quitan vida.
    pub fn apply_hits(&mut self, hits: &[(u32, u32)]) {
        let Some(yo) = self.my_id else { return };
        for &(victima, tirador) in hits {
            if victima != yo || !self.peleando() || self.derrota.is_some() {
                continue;
            }
            self.vida = (self.vida - DANO_DISPARO).max(0.0);
            let normal = self.remotes.iter().find(|r| r.target.id == tirador).map_or(Vec3::Y, |r| (r.shown.position - self.ship.position).normalize());
            self.sparks.burst(self.ship.position + normal * NAVE_RADIO, normal, self.ship.velocity, 12.0);
            let nombre = self.nombre_de(tirador);
            if self.vida <= 0.0 {
                self.derribado_por = Some(nombre);
                self.derrotar(Causa::Derribada);
            } else {
                self.notice(format!("{nombre} TE DIO  -{DANO_DISPARO:.0}%"), 0xEF5350);
            }
        }
    }

    /// El servidor manda la fase de la pelea, el ganador y mis derribos.
    pub fn set_fight_server(&mut self, phase: u8, ms: u32, winner: u32, my_kills: u8) {
        if let Some(p) = Phase::from_u8(phase) {
            self.pelea.set_server(p, ms as f32 / 1000.0);
        }
        self.ganador = winner;
        self.mis_derribos = my_kills;
    }

    /// ¿Se pidió empezar la pelea? Devuelve true una sola vez por pulsación.
    pub fn take_fight_request(&mut self) -> bool {
        std::mem::take(&mut self.fight_request)
    }

    /// Ids de los rivales a los que les pegué desde la última llamada (para avisar al servidor).
    pub fn take_hits_out(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.golpes)
    }

    pub fn fight_phase(&self) -> Phase {
        self.pelea.phase
    }

    pub(super) fn nombre_de(&self, id: u32) -> String {
        if Some(id) == self.my_id {
            return nombre_mostrado(&self.name, "TU".into());
        }
        self.remotes
            .iter()
            .find(|r| r.target.id == id)
            .map_or(format!("J{id}"), |r| nombre_mostrado(&r.target.name, format!("J{id}")))
    }

    /// Tabla de la pelea: yo y los del modo pelea que veo, primero los vivos y luego por derribos.
    pub(super) fn peleadores(&self) -> Vec<Peleador> {
        let mut list = vec![Peleador {
            label: nombre_mostrado(&self.name, "TU".into()),
            color: self.my_id.map_or(0xFFFFFF, color_jugador),
            kills: self.mis_derribos,
            vivo: !self.fuera,
        }];
        let en_marcha = matches!(self.pelea.phase, Phase::Countdown | Phase::Racing | Phase::Results);
        for r in self.remotes.iter().filter(|r| r.target.fight & PELEA != 0) {
            // En plena pelea la tabla es de los que pelean: los que llegaron tarde no salen.
            if en_marcha && !self.esperando_pelea() && r.target.fight & PELEA_ESPERA != 0 {
                continue;
            }
            list.push(Peleador {
                label: nombre_mostrado(&r.target.name, format!("J{}", r.target.id)),
                color: color_jugador(r.target.id),
                kills: r.target.kills,
                vivo: r.target.fight & PELEA_FUERA == 0,
            });
        }
        list.sort_by_key(|p| (!p.vivo, u8::MAX - p.kills));
        list
    }

    pub(super) fn draw_rayos_rivales(&self, fb: &mut Framebuffer, vp: &Mat4) {
        self.rayos_rivales.draw_color(fb, vp, COLOR_RAYO_RIVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::*;

    fn pelear() -> Input {
        Input { fight: true, ..NADA }
    }

    /// Un rival en el modo pelea, `distancia` delante de mi nave.
    fn rival_delante(g: &Game, id: u32, distancia: f32) -> RemoteShip {
        RemoteShip { position: g.ship.position + g.ship.forward() * distancia, fight: PELEA, ..rival(id, 0.0) }
    }

    /// Online, en la pelea ya empezada contra el jugador 2, que está delante de mí.
    fn en_plena_pelea() -> Game {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_my_id(Some(1));
        g.update(0.05, &pelear());
        g.set_fight_server(Phase::Countdown as u8, 0, 0, 0);
        g.update(0.05, &NADA);
        assert!(g.en_pelea, "estaba en la sala de pelea: entra");
        g.set_fight_server(Phase::Racing as u8, 0, 0, 0);
        g.update(0.05, &NADA);
        let r = rival_delante(&g, 2, 40.0);
        g.set_remote_players(&[r]);
        avanzar(&mut g, 0.5); // el rival suavizado llega a su lugar
        g
    }

    #[test]
    fn b_en_el_menu_entra_a_la_sala_de_pelea_en_la_arena() {
        let mut g = Game::new(1);
        g.update(0.05, &pelear());
        assert_eq!((g.screen(), g.mode()), (Screen::Playing, Modo::Pelea));
        assert!(g.ship.position.distance(scenario::FIGHT_CENTER) < scenario::FIGHT_RING + 1.0);
        let s = g.local_state();
        assert!(s.fight & PELEA != 0 && s.progress & SOLO != 0, "avisa que pelea y la carrera no lo cuenta");
        assert_eq!(s.fight & (PELEA_FUERA | PELEA_ESPERA), 0);
    }

    #[test]
    fn sin_conexion_o_solo_no_se_puede_empezar() {
        let mut g = Game::new(1);
        g.update(0.05, &pelear());
        g.update(0.05, &empezar());
        assert!(!g.take_fight_request());
        assert!(g.notice.as_ref().is_some_and(|n| n.0.contains("ONLINE")));

        g.set_online(true);
        g.update(0.05, &NADA);
        g.update(0.05, &empezar());
        assert!(!g.take_fight_request());
        assert!(g.notice.as_ref().is_some_and(|n| n.0.contains("FALTA OTRO")));

        g.set_remote_players(&[RemoteShip { fight: PELEA, ..rival(2, 0.0) }]);
        g.update(0.05, &NADA);
        g.update(0.05, &empezar());
        assert!(g.take_fight_request());
        assert!(!g.take_fight_request(), "se entrega una sola vez");
    }

    #[test]
    fn en_la_cuenta_regresiva_todos_quedan_quietos_en_su_casilla() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.update(0.05, &pelear());
        g.set_slot(3);
        g.ship.position = Vec3::new(5.0, 5.0, 5.0);
        g.set_fight_server(Phase::Countdown as u8, 0, 0, 0);
        g.update(0.05, &NADA);
        let casilla = scenario::fight_slot(3).0;
        assert!(g.ship.position.distance(casilla) < 1e-3);
        avanzar_con(&mut g, 1.0, Input { forward: true, ..NADA });
        assert!(g.ship.position.distance(casilla) < 1e-3);
        g.update(0.05, &Input { menu: true, ..NADA });
        assert_eq!(g.screen(), Screen::Playing, "en la cuenta no se puede salir");
    }

    #[test]
    fn mis_rayos_pegan_al_rival_y_se_avisa_al_servidor() {
        let mut g = en_plena_pelea();
        avanzar_con(&mut g, 0.6, Input { fire: true, ..NADA });
        let golpes = g.take_hits_out();
        assert!(!golpes.is_empty() && golpes.iter().all(|&id| id == 2), "{golpes:?}");
        assert!(g.take_hits_out().is_empty(), "se entregan una vez");
        assert!(g.local_state().fight & DISPARANDO != 0, "los demás saben que disparo");
    }

    #[test]
    fn en_la_sala_los_rayos_no_hacen_dano() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.update(0.05, &pelear());
        let r = rival_delante(&g, 2, 40.0);
        g.set_remote_players(&[r]);
        avanzar_con(&mut g, 0.6, Input { fire: true, ..NADA });
        assert!(g.take_hits_out().is_empty());
    }

    #[test]
    fn los_impactos_del_servidor_quitan_vida_y_en_cero_quedo_fuera() {
        let mut g = en_plena_pelea();
        g.apply_hits(&[(2, 1)]); // yo le pegué a otro: no me quita nada
        assert_eq!(g.health(), VIDA_MAX);
        g.apply_hits(&[(1, 2)]);
        assert_eq!(g.health(), VIDA_MAX - DANO_DISPARO);
        let tiros = vec![(1, 2); (VIDA_MAX / DANO_DISPARO) as usize];
        g.apply_hits(&tiros);
        assert_eq!(g.health(), 0.0);
        let d = g.derrota.expect("me derribaron");
        assert_eq!(d.causa, Causa::Derribada);
        assert!(g.fuera && g.local_state().fight & PELEA_FUERA != 0, "avisa al servidor");
        assert_eq!(g.derribado_por.as_deref(), Some("J2"));

        // Pasado el cartel vuelo libre para mirar, pero ya no peleo.
        avanzar(&mut g, DERROTA_S + 0.2);
        assert!(g.derrota.is_none() && !g.peleando());
        let antes = g.ship.position;
        avanzar_con(&mut g, 1.0, Input { forward: true, ..NADA });
        assert!(g.ship.position.distance(antes) > 5.0);
    }

    #[test]
    fn mientras_peleo_no_veo_a_los_derribados_ni_a_los_que_esperan() {
        let mut g = en_plena_pelea();
        let fuera = RemoteShip { fight: PELEA | PELEA_FUERA, ..rival(3, 0.0) };
        let espera = RemoteShip { fight: PELEA | PELEA_ESPERA, ..rival(4, 0.0) };
        let corre = RemoteShip { progress: 2, ..rival(5, 0.0) };
        let vivo = rival_delante(&g, 2, 40.0);
        g.set_remote_players(&[vivo, fuera, espera, corre]);
        let ids = |g: &Game| g.rivales().map(|r| r.target.id).collect::<Vec<_>>();
        assert_eq!(ids(&g), [2]);
        assert_eq!(g.peleadores().len(), 3, "la tabla: yo, el vivo y el derribado");

        g.apply_hits(&[(1, 2); 10]);
        assert_eq!(ids(&g), [2, 3, 4], "derribado veo a todos los del modo pelea");
    }

    #[test]
    fn quien_llega_con_la_pelea_empezada_mira_y_no_participa() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_my_id(Some(1));
        g.set_fight_server(Phase::Racing as u8, 5000, 0, 0);
        g.update(0.05, &pelear());
        assert!(g.esperando_pelea() && !g.peleando());
        assert!(g.local_state().fight & PELEA_ESPERA != 0);
        g.apply_hits(&[(1, 2)]);
        assert_eq!(g.health(), VIDA_MAX, "no le pueden pegar");

        g.set_fight_server(Phase::Results as u8, 0, 2, 0);
        g.update(0.05, &NADA);
        assert!(!g.esperando_pelea(), "cuando termina ya es uno más de la sala");
        g.render(&mut Framebuffer::new(320, 180)); // con el cartel del ganador
    }

    #[test]
    fn los_rayos_de_un_rival_que_dispara_se_dibujan() {
        let mut g = en_plena_pelea();
        let r = RemoteShip { fight: PELEA | DISPARANDO, ..rival_delante(&g, 2, 40.0) };
        g.set_remote_players(&[r]);
        avanzar(&mut g, 0.3);
        assert!(!g.rayos_rivales.is_empty());
    }
}
