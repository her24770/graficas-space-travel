//! Estado de la carrera: espera, cuenta regresiva, carrera y resultados.
//! Offline lo maneja el propio juego; online lo dicta el servidor.

/// Los números coinciden con `protocol::PHASE_*` (lo comprueba un test en `web`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Lobby = 0,
    Countdown = 1,
    Racing = 2,
    Results = 3,
}

impl Phase {
    pub fn from_u8(v: u8) -> Option<Phase> {
        match v {
            0 => Some(Phase::Lobby),
            1 => Some(Phase::Countdown),
            2 => Some(Phase::Racing),
            3 => Some(Phase::Results),
            _ => None,
        }
    }
}

pub const COUNTDOWN_S: f32 = 3.0;

pub struct Race {
    pub phase: Phase,
    /// Segundos desde que empezó la fase actual.
    pub time: f32,
}

impl Race {
    pub fn new() -> Self {
        Self { phase: Phase::Lobby, time: 0.0 }
    }

    pub fn start_local(&mut self) {
        if matches!(self.phase, Phase::Lobby | Phase::Results) {
            *self = Race { phase: Phase::Countdown, time: 0.0 };
        }
    }

    pub fn finish_local(&mut self) {
        *self = Race { phase: Phase::Results, time: 0.0 };
    }

    pub fn reset(&mut self) {
        *self = Race::new();
    }

    /// Avanza el reloj. Sin servidor (`online = false`) también hace las transiciones.
    pub fn update(&mut self, dt: f32, online: bool) {
        self.time += dt;
        if !online && self.phase == Phase::Countdown && self.time >= COUNTDOWN_S {
            *self = Race { phase: Phase::Racing, time: self.time - COUNTDOWN_S };
        }
    }

    /// El servidor manda la fase y el tiempo; se corrige sin saltos bruscos.
    pub fn set_server(&mut self, phase: Phase, secs: f32) {
        if self.phase != phase || (self.time - secs).abs() > 0.2 {
            *self = Race { phase, time: secs };
        } else {
            self.time += (secs - self.time) * 0.2;
        }
    }

    /// 3, 2, 1 durante la cuenta regresiva.
    pub fn countdown_number(&self) -> Option<u32> {
        (self.phase == Phase::Countdown).then(|| (COUNTDOWN_S - self.time).ceil().max(1.0) as u32)
    }
}

impl Default for Race {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_cuenta_regresiva_y_luego_carrera() {
        let mut r = Race::new();
        r.start_local();
        assert_eq!((r.phase, r.countdown_number()), (Phase::Countdown, Some(3)));
        r.update(1.1, false);
        assert_eq!(r.countdown_number(), Some(2));
        r.update(2.0, false);
        assert_eq!(r.phase, Phase::Racing);
        assert!(r.time > 0.0 && r.time < 0.2);
    }

    #[test]
    fn no_se_puede_reiniciar_en_plena_carrera() {
        let mut r = Race { phase: Phase::Racing, time: 5.0 };
        r.start_local();
        assert_eq!(r.phase, Phase::Racing);
    }

    #[test]
    fn online_el_reloj_local_no_cambia_de_fase_solo() {
        let mut r = Race::new();
        r.set_server(Phase::Countdown, 0.0);
        r.update(10.0, true);
        assert_eq!(r.phase, Phase::Countdown);
    }

    #[test]
    fn el_servidor_corrige_el_tiempo() {
        let mut r = Race { phase: Phase::Racing, time: 10.0 };
        r.set_server(Phase::Racing, 14.0);
        assert_eq!(r.time, 14.0);
        r.set_server(Phase::Racing, 14.05);
        assert!(r.time > 14.0 && r.time < 14.05);
    }

    #[test]
    fn los_numeros_de_fase_ida_y_vuelta() {
        for p in [Phase::Lobby, Phase::Countdown, Phase::Racing, Phase::Results] {
            assert_eq!(Phase::from_u8(p as u8), Some(p));
        }
        assert_eq!(Phase::from_u8(9), None);
    }
}
