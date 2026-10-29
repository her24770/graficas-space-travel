//! Reloj de la carrera en el servidor: espera → cuenta regresiva → carrera → resultados.
//! No usa el reloj del sistema: recibe `now` (ms), así se prueba sin esperar.
use protocol::{PlayerState, GATE_COUNT, PHASE_COUNTDOWN, PHASE_LOBBY, PHASE_RACING, PHASE_RESULTS};
use std::collections::HashMap;

pub const CUENTA_ATRAS_MS: u32 = 3000;
/// Si alguien nunca termina, la carrera se cierra sola.
pub const MAX_CARRERA_MS: u32 = 300_000;

pub struct RaceState {
    pub phase: u8,
    phase_start: u32,
    finishes: HashMap<u32, u32>,
}

impl RaceState {
    pub fn new() -> Self {
        Self { phase: PHASE_LOBBY, phase_start: 0, finishes: HashMap::new() }
    }

    /// Alguien pidió empezar. Solo vale en espera o resultados.
    pub fn start(&mut self, now: u32) -> bool {
        if self.phase == PHASE_LOBBY || self.phase == PHASE_RESULTS {
            self.phase = PHASE_COUNTDOWN;
            self.phase_start = now;
            self.finishes.clear();
            return true;
        }
        false
    }

    pub fn tick(&mut self, now: u32, players: &HashMap<u32, PlayerState>) {
        let elapsed = now.wrapping_sub(self.phase_start);
        match self.phase {
            PHASE_COUNTDOWN if elapsed >= CUENTA_ATRAS_MS => {
                self.phase = PHASE_RACING;
                self.phase_start = now;
            }
            PHASE_RACING => {
                let todos = !players.is_empty() && players.keys().all(|id| self.finishes.contains_key(id));
                if todos || elapsed >= MAX_CARRERA_MS {
                    self.phase = PHASE_RESULTS;
                    self.phase_start = now;
                }
            }
            _ => {}
        }
        if players.is_empty() && self.phase != PHASE_LOBBY {
            self.phase = PHASE_LOBBY;
            self.phase_start = now;
            self.finishes.clear();
        }
    }

    /// Un jugador reporta cuántos puntos lleva. Al completar todos se registra su tiempo, una sola vez.
    pub fn record_progress(&mut self, id: u32, progress: u8, now: u32) {
        if self.phase == PHASE_RACING && progress >= GATE_COUNT && !self.finishes.contains_key(&id) {
            self.finishes.insert(id, now.wrapping_sub(self.phase_start).max(1));
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.finishes.remove(&id);
    }

    pub fn finish_ms(&self, id: u32) -> u32 {
        self.finishes.get(&id).copied().unwrap_or(0)
    }

    pub fn phase_ms(&self, now: u32) -> u32 {
        if self.phase == PHASE_LOBBY { 0 } else { now.wrapping_sub(self.phase_start) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jugadores(ids: &[u32]) -> HashMap<u32, PlayerState> {
        ids.iter().map(|&id| (id, PlayerState::default())).collect()
    }

    #[test]
    fn la_cuenta_regresiva_dura_tres_segundos_y_pasa_a_carrera() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1]));
        assert!(r.start(1000));
        r.tick(3999, &p);
        assert_eq!(r.phase, PHASE_COUNTDOWN);
        r.tick(4000, &p);
        assert_eq!(r.phase, PHASE_RACING);
        assert_eq!(r.phase_ms(5500), 1500);
    }

    #[test]
    fn no_se_puede_empezar_en_plena_carrera() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1]));
        r.start(0);
        r.tick(3000, &p);
        assert!(!r.start(4000));
        assert_eq!(r.phase, PHASE_RACING);
    }

    #[test]
    fn el_tiempo_final_se_registra_una_sola_vez_y_solo_al_completar() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1, 2]));
        r.start(0);
        r.tick(3000, &p);
        r.record_progress(1, GATE_COUNT - 1, 20_000);
        assert_eq!(r.finish_ms(1), 0);
        r.record_progress(1, GATE_COUNT, 23_000);
        r.record_progress(1, GATE_COUNT, 30_000);
        assert_eq!(r.finish_ms(1), 20_000, "tiempo desde que arrancó la carrera (3000)");
    }

    #[test]
    fn no_cuenta_terminar_antes_de_la_largada() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1]));
        r.start(0);
        r.tick(1000, &p);
        r.record_progress(1, GATE_COUNT, 1500);
        assert_eq!(r.finish_ms(1), 0);
    }

    #[test]
    fn cuando_terminan_todos_hay_resultados_y_se_puede_revancha() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1, 2]));
        r.start(0);
        r.tick(3000, &p);
        r.record_progress(1, GATE_COUNT, 10_000);
        r.tick(10_000, &p);
        assert_eq!(r.phase, PHASE_RACING, "falta el 2");
        r.record_progress(2, GATE_COUNT, 12_000);
        r.tick(12_000, &p);
        assert_eq!(r.phase, PHASE_RESULTS);
        assert!(r.start(13_000));
        assert_eq!(r.finish_ms(1), 0, "la revancha borra los tiempos");
    }

    #[test]
    fn si_alguien_nunca_termina_la_carrera_se_cierra() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1]));
        r.start(0);
        r.tick(3000, &p);
        r.tick(3000 + MAX_CARRERA_MS, &p);
        assert_eq!(r.phase, PHASE_RESULTS);
    }

    #[test]
    fn sin_jugadores_vuelve_a_la_espera() {
        let (mut r, p) = (RaceState::new(), jugadores(&[1]));
        r.start(0);
        r.tick(3000, &p);
        r.tick(4000, &jugadores(&[]));
        assert_eq!(r.phase, PHASE_LOBBY);
    }
}
