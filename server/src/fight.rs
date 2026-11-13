//! Pelea entre jugadores, último en pie: sala → cuenta regresiva → pelea → resultados.
//! Cada cliente lleva su propia vida; el servidor valida los impactos, se los reenvía a la
//! víctima, cuenta quién sigue vivo, los derribos y decide el ganador.
//! No usa el reloj del sistema: recibe `now` (ms), así se prueba sin esperar.
use protocol::{Hit, PlayerState, MAX_HITS, PELEA, PELEA_ESPERA, PELEA_FUERA, PHASE_COUNTDOWN, PHASE_LOBBY, PHASE_RACING, PHASE_RESULTS};
use std::collections::{HashMap, HashSet};

pub const CUENTA_ATRAS_MS: u32 = 3000;
/// Si nadie gana, la pelea se cierra sola.
pub const MAX_PELEA_MS: u32 = 300_000;
/// Jugadores en la sala de pelea que hacen falta para empezar.
pub const MIN_JUGADORES: usize = 2;
/// Un mismo tirador no puede pegar más seguido que esto (el arma dispara cada 180 ms).
pub const ENTRE_IMPACTOS_MS: u32 = 120;
/// Si te derriban (o chocas) hasta este tiempo después de un disparo, el derribo es de quien disparó.
pub const CREDITO_MS: u32 = 10_000;

pub struct FightState {
    pub phase: u8,
    phase_start: u32,
    /// Los que entraron a esta pelea en la cuenta regresiva.
    participantes: HashSet<u32>,
    fuera: HashSet<u32>,
    kills: HashMap<u32, u8>,
    /// Último que le pegó a cada uno: (tirador, cuándo).
    ultimo_golpe: HashMap<u32, (u32, u32)>,
    /// Último impacto aceptado de cada tirador.
    ultimo_disparo: HashMap<u32, u32>,
    pendientes: Vec<Hit>,
    pub winner: u32,
}

/// ¿Está en la sala de pelea, listo para entrar a la siguiente?
fn en_sala(s: &PlayerState) -> bool {
    s.fight & PELEA != 0 && s.fight & PELEA_ESPERA == 0
}

impl FightState {
    pub fn new() -> Self {
        Self {
            phase: PHASE_LOBBY,
            phase_start: 0,
            participantes: HashSet::new(),
            fuera: HashSet::new(),
            kills: HashMap::new(),
            ultimo_golpe: HashMap::new(),
            ultimo_disparo: HashMap::new(),
            pendientes: Vec::new(),
            winner: 0,
        }
    }

    /// Alguien pidió empezar. Solo en la sala o en resultados, y con al menos dos en la sala de pelea.
    pub fn start(&mut self, now: u32, players: &HashMap<u32, PlayerState>) -> bool {
        let sala: HashSet<u32> = players.iter().filter(|(_, s)| en_sala(s)).map(|(id, _)| *id).collect();
        if !(self.phase == PHASE_LOBBY || self.phase == PHASE_RESULTS) || sala.len() < MIN_JUGADORES {
            return false;
        }
        *self = FightState { phase: PHASE_COUNTDOWN, phase_start: now, participantes: sala, ..FightState::new() };
        true
    }

    fn vivos(&self) -> impl Iterator<Item = &u32> {
        self.participantes.iter().filter(|id| !self.fuera.contains(id))
    }

    pub fn tick(&mut self, now: u32, players: &HashMap<u32, PlayerState>) {
        let elapsed = now.wrapping_sub(self.phase_start);
        if matches!(self.phase, PHASE_COUNTDOWN | PHASE_RACING) {
            // Queda fuera quien se fue del modo pelea, quien se desconectó y, ya en plena pelea, quien
            // avisa que lo derribaron. En la cuenta regresiva el aviso puede ser de la pelea anterior.
            let racing = self.phase == PHASE_RACING;
            let caidos: Vec<u32> = self
                .vivos()
                .copied()
                .filter(|id| players.get(id).is_none_or(|s| s.fight & PELEA == 0 || (racing && s.fight & PELEA_FUERA != 0)))
                .collect();
            for id in caidos {
                self.eliminar(id, now, players.get(&id).is_some_and(|s| s.fight & PELEA_FUERA != 0));
            }
        }
        match self.phase {
            PHASE_COUNTDOWN if elapsed >= CUENTA_ATRAS_MS => {
                self.phase = PHASE_RACING;
                self.phase_start = now;
            }
            PHASE_RACING => {
                let vivos: Vec<u32> = self.vivos().copied().collect();
                if vivos.len() <= 1 || elapsed >= MAX_PELEA_MS {
                    // Último en pie; si se acaba el tiempo, el vivo con más derribos.
                    self.winner = vivos.iter().copied().max_by_key(|id| (self.kills(*id), u32::MAX - id)).unwrap_or(0);
                    self.phase = PHASE_RESULTS;
                    self.phase_start = now;
                }
            }
            _ => {}
        }
        // Si ya nadie está en el modo pelea, se vuelve a la sala.
        if self.phase != PHASE_LOBBY && !players.values().any(|s| s.fight & PELEA != 0) {
            *self = FightState::new();
        }
    }

    fn eliminar(&mut self, id: u32, now: u32, derribado: bool) {
        self.fuera.insert(id);
        if !derribado {
            return; // se fue o se desconectó: no es un derribo
        }
        if let Some((tirador, cuando)) = self.ultimo_golpe.get(&id).copied() {
            if now.wrapping_sub(cuando) <= CREDITO_MS && tirador != id {
                *self.kills.entry(tirador).or_insert(0) += 1;
            }
        }
    }

    /// Un disparo de `shooter` le pegó a `victim`. Se acepta si los dos siguen vivos en plena pelea
    /// y el tirador no pega más rápido de lo que dispara su arma.
    pub fn hit(&mut self, now: u32, shooter: u32, victim: u32) -> bool {
        let vivo = |id: &u32| self.participantes.contains(id) && !self.fuera.contains(id);
        let rapido = self.ultimo_disparo.get(&shooter).is_some_and(|&t| now.wrapping_sub(t) < ENTRE_IMPACTOS_MS);
        if self.phase != PHASE_RACING || shooter == victim || !vivo(&shooter) || !vivo(&victim) || rapido || self.pendientes.len() >= MAX_HITS {
            return false;
        }
        self.ultimo_disparo.insert(shooter, now);
        self.ultimo_golpe.insert(victim, (shooter, now));
        self.pendientes.push(Hit { victim, shooter });
        true
    }

    /// Impactos aceptados desde la última vez (van en el siguiente snapshot).
    pub fn take_hits(&mut self) -> Vec<Hit> {
        std::mem::take(&mut self.pendientes)
    }

    pub fn kills(&self, id: u32) -> u8 {
        self.kills.get(&id).copied().unwrap_or(0)
    }

    pub fn phase_ms(&self, now: u32) -> u32 {
        if self.phase == PHASE_LOBBY { 0 } else { now.wrapping_sub(self.phase_start) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_pelea(ids: &[u32]) -> HashMap<u32, PlayerState> {
        ids.iter().map(|&id| (id, PlayerState { fight: PELEA, ..PlayerState::default() })).collect()
    }

    /// Pelea ya empezada entre `ids` (pasó la cuenta regresiva en t = 3000).
    fn peleando(ids: &[u32]) -> (FightState, HashMap<u32, PlayerState>) {
        let (mut f, p) = (FightState::new(), en_pelea(ids));
        assert!(f.start(0, &p));
        f.tick(CUENTA_ATRAS_MS, &p);
        assert_eq!(f.phase, PHASE_RACING);
        (f, p)
    }

    fn derribar(p: &mut HashMap<u32, PlayerState>, id: u32) {
        p.get_mut(&id).unwrap().fight |= PELEA_FUERA;
    }

    #[test]
    fn hacen_falta_dos_en_la_sala_de_pelea() {
        let mut f = FightState::new();
        assert!(!f.start(0, &en_pelea(&[1])));
        let mut p = en_pelea(&[1]);
        p.insert(2, PlayerState::default()); // está conectado pero no en el modo pelea
        assert!(!f.start(0, &p));
        assert!(f.start(0, &en_pelea(&[1, 2])));
        assert_eq!(f.phase, PHASE_COUNTDOWN);
    }

    #[test]
    fn el_ultimo_en_pie_gana_y_los_derribos_son_de_quien_disparo() {
        let (mut f, mut p) = peleando(&[1, 2, 3]);
        assert!(f.hit(4000, 1, 2));
        derribar(&mut p, 2);
        f.tick(4100, &p);
        assert_eq!((f.phase, f.kills(1)), (PHASE_RACING, 1), "quedan dos");

        assert!(f.hit(5000, 3, 1));
        derribar(&mut p, 1);
        f.tick(5100, &p);
        assert_eq!(f.phase, PHASE_RESULTS);
        assert_eq!((f.winner, f.kills(3)), (3, 1));
    }

    #[test]
    fn solo_valen_impactos_entre_vivos_en_plena_pelea_y_al_ritmo_del_arma() {
        let mut f = FightState::new();
        assert!(!f.hit(100, 1, 2), "en la sala no hay daño");
        let (mut f, mut p) = peleando(&[1, 2, 3]);
        assert!(!f.hit(4000, 1, 1), "a sí mismo no");
        assert!(!f.hit(4000, 1, 9), "a alguien que no pelea no");
        assert!(f.hit(4000, 1, 2));
        assert!(!f.hit(4050, 1, 2), "más rápido que el arma no");
        assert!(f.hit(4200, 1, 2));
        assert_eq!(f.take_hits(), vec![Hit { victim: 2, shooter: 1 }; 2]);
        assert!(f.take_hits().is_empty(), "se entregan una vez");

        derribar(&mut p, 2);
        f.tick(4300, &p);
        assert!(!f.hit(4500, 2, 3), "un derribado ya no pega");
        assert!(!f.hit(4500, 3, 2), "ni le pegan");
    }

    #[test]
    fn irse_o_desconectarse_deja_fuera_sin_dar_derribo() {
        let (mut f, mut p) = peleando(&[1, 2, 3]);
        f.hit(4000, 1, 2);
        p.get_mut(&2).unwrap().fight = 0; // volvió al menú
        p.remove(&3); // se desconectó
        f.tick(4100, &p);
        assert_eq!((f.phase, f.winner, f.kills(1)), (PHASE_RESULTS, 1, 0));
    }

    #[test]
    fn chocar_poco_despues_de_recibir_un_disparo_cuenta_como_derribo() {
        let (mut f, mut p) = peleando(&[1, 2, 3]);
        f.hit(4000, 1, 2);
        derribar(&mut p, 2);
        f.tick(4000 + CREDITO_MS, &p);
        assert_eq!(f.kills(1), 1);

        f.hit(5000, 1, 3);
        derribar(&mut p, 3);
        f.tick(5000 + CREDITO_MS + 1, &p);
        assert_eq!(f.kills(1), 1, "mucho después ya no es suyo");
    }

    #[test]
    fn quien_llega_tarde_no_entra_y_la_revancha_reinicia_todo() {
        let (mut f, mut p) = peleando(&[1, 2]);
        p.insert(5, PlayerState { fight: PELEA | PELEA_ESPERA, ..PlayerState::default() });
        assert!(!f.hit(4000, 5, 1), "el que espera no participa");
        f.hit(4000, 1, 2);
        derribar(&mut p, 2);
        f.tick(4100, &p);
        assert_eq!((f.phase, f.winner), (PHASE_RESULTS, 1));

        let mut p = en_pelea(&[1, 2, 5]);
        assert!(f.start(9000, &p));
        assert_eq!((f.kills(1), f.winner), (0, 0));
        f.tick(9000 + CUENTA_ATRAS_MS, &p);
        derribar(&mut p, 1);
        derribar(&mut p, 2);
        f.tick(13_000, &p);
        assert_eq!(f.winner, 5, "en la revancha ya participa");
    }

    #[test]
    fn en_la_revancha_el_aviso_viejo_de_derribado_no_saca_a_nadie() {
        let (mut f, mut p) = peleando(&[1, 2]);
        derribar(&mut p, 2);
        f.tick(4000, &p);
        assert_eq!(f.phase, PHASE_RESULTS);
        // El 2 sigue mandando "derribado" un momento después de que empieza la revancha.
        assert!(f.start(5000, &p));
        f.tick(5050, &p);
        assert_eq!(f.phase, PHASE_COUNTDOWN);
        p.get_mut(&2).unwrap().fight = PELEA; // ya recibió la cuenta regresiva
        f.tick(5000 + CUENTA_ATRAS_MS, &p);
        assert_eq!(f.phase, PHASE_RACING, "los dos siguen en pie");
    }

    #[test]
    fn si_se_acaba_el_tiempo_gana_el_vivo_con_mas_derribos() {
        let (mut f, mut p) = peleando(&[1, 2, 3, 4]);
        f.hit(4000, 2, 4);
        derribar(&mut p, 4);
        f.tick(4100, &p);
        f.tick(CUENTA_ATRAS_MS + MAX_PELEA_MS, &p);
        assert_eq!((f.phase, f.winner), (PHASE_RESULTS, 2));
    }

    #[test]
    fn sin_nadie_en_el_modo_pelea_vuelve_a_la_sala() {
        let (mut f, _) = peleando(&[1, 2]);
        f.tick(5000, &HashMap::new());
        assert_eq!(f.phase, PHASE_LOBBY);
    }
}
