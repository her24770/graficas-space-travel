//! Mensajes entre el navegador y el servidor. Binario, sin dependencias.
//! Cada cliente simula su propia nave; el servidor reparte posiciones y lleva el reloj de la carrera.

pub const MAX_PLAYERS: usize = 8;
/// Puntos de control del circuito. Debe coincidir con `engine::scenario::GATE_COUNT` (lo comprueba un test en `web`).
pub const GATE_COUNT: u8 = 10;

pub const PHASE_LOBBY: u8 = 0;
pub const PHASE_COUNTDOWN: u8 = 1;
pub const PHASE_RACING: u8 = 2;
pub const PHASE_RESULTS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub model: u8,
    /// Puntos de control cruzados.
    pub progress: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub id: u32,
    pub state: PlayerState,
    /// Milisegundos de carrera con los que terminó; 0 = aún no termina.
    pub finish_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Reloj del servidor (ms): de él dependen las órbitas de los planetas.
    pub clock_ms: u32,
    pub phase: u8,
    /// Milisegundos desde que empezó la fase actual.
    pub phase_ms: u32,
    pub players: Vec<Player>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    /// Primer mensaje: el id que el servidor asignó a este cliente.
    Welcome { id: u32 },
    Snapshot(Snapshot),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientMsg {
    /// Mi estado actual.
    State(PlayerState),
    /// Pedir que empiece la carrera (solo vale en espera o resultados).
    StartRace,
}

const C_STATE: u8 = 1;
const C_START: u8 = 2;
const S_WELCOME: u8 = 1;
const S_SNAPSHOT: u8 = 2;

fn put_state(out: &mut Vec<u8>, s: &PlayerState) {
    for v in s.position {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&s.yaw.to_le_bytes());
    out.extend_from_slice(&s.pitch.to_le_bytes());
    out.push(s.model);
    out.push(s.progress);
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take::<2>().map(u16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take::<4>().map(u32::from_le_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take::<4>().map(f32::from_le_bytes)
    }
    fn state(&mut self) -> Option<PlayerState> {
        Some(PlayerState {
            position: [self.f32()?, self.f32()?, self.f32()?],
            yaw: self.f32()?,
            pitch: self.f32()?,
            model: self.u8()?,
            progress: self.u8()?,
        })
    }
    fn done(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn encode_client(msg: &ClientMsg) -> Vec<u8> {
    match msg {
        ClientMsg::State(s) => {
            let mut out = vec![C_STATE];
            put_state(&mut out, s);
            out
        }
        ClientMsg::StartRace => vec![C_START],
    }
}

pub fn decode_client(bytes: &[u8]) -> Option<ClientMsg> {
    let mut r = Reader(bytes);
    let msg = match r.u8()? {
        C_STATE => ClientMsg::State(r.state()?),
        C_START => ClientMsg::StartRace,
        _ => return None,
    };
    r.done().then_some(msg)
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ServerMsg::Welcome { id } => {
                let mut out = vec![S_WELCOME];
                out.extend_from_slice(&id.to_le_bytes());
                out
            }
            ServerMsg::Snapshot(s) => {
                let mut out = vec![S_SNAPSHOT];
                out.extend_from_slice(&s.clock_ms.to_le_bytes());
                out.push(s.phase);
                out.extend_from_slice(&s.phase_ms.to_le_bytes());
                out.extend_from_slice(&(s.players.len() as u16).to_le_bytes());
                for p in &s.players {
                    out.extend_from_slice(&p.id.to_le_bytes());
                    put_state(&mut out, &p.state);
                    out.extend_from_slice(&p.finish_ms.to_le_bytes());
                }
                out
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<ServerMsg> {
        let mut r = Reader(bytes);
        let msg = match r.u8()? {
            S_WELCOME => ServerMsg::Welcome { id: r.u32()? },
            S_SNAPSHOT => {
                let (clock_ms, phase, phase_ms) = (r.u32()?, r.u8()?, r.u32()?);
                let n = r.u16()? as usize;
                let mut players = Vec::with_capacity(n.min(MAX_PLAYERS));
                for _ in 0..n {
                    players.push(Player { id: r.u32()?, state: r.state()?, finish_ms: r.u32()? });
                }
                ServerMsg::Snapshot(Snapshot { clock_ms, phase, phase_ms, players })
            }
            _ => return None,
        };
        r.done().then_some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado() -> PlayerState {
        PlayerState { position: [1.5, -2.0, 30.25], yaw: 0.5, pitch: -0.25, model: 2, progress: 4 }
    }

    fn snapshot() -> ServerMsg {
        ServerMsg::Snapshot(Snapshot {
            clock_ms: 123_456,
            phase: PHASE_RACING,
            phase_ms: 7_000,
            players: vec![Player { id: 1, state: estado(), finish_ms: 0 }, Player { id: 9, state: estado(), finish_ms: 42_000 }],
        })
    }

    #[test]
    fn los_mensajes_del_cliente_ida_y_vuelta() {
        for m in [ClientMsg::State(estado()), ClientMsg::StartRace] {
            assert_eq!(decode_client(&encode_client(&m)), Some(m));
        }
    }

    #[test]
    fn welcome_y_snapshot_ida_y_vuelta() {
        for m in [ServerMsg::Welcome { id: 7 }, snapshot()] {
            assert_eq!(ServerMsg::decode(&m.encode()), Some(m));
        }
    }

    #[test]
    fn rechaza_mensajes_truncados_vacios_o_con_basura() {
        let ok = encode_client(&ClientMsg::State(estado()));
        assert_eq!(decode_client(&ok[..ok.len() - 1]), None);
        assert_eq!(decode_client(&[]), None);
        assert_eq!(decode_client(&[9, 9, 9]), None);
        assert_eq!(decode_client(&[C_START, 0]), None, "StartRace no lleva datos");
        let mut largo = ok.clone();
        largo.push(0);
        assert_eq!(decode_client(&largo), None);
        let snap = snapshot().encode();
        assert_eq!(ServerMsg::decode(&snap[..snap.len() - 3]), None);
    }

    #[test]
    fn un_snapshot_que_miente_sobre_su_tamano_no_revienta() {
        let mut m = snapshot().encode();
        m[10] = 0xFF;
        m[11] = 0xFF; // cantidad de jugadores absurda
        assert_eq!(ServerMsg::decode(&m), None);
    }

    #[test]
    fn el_tamano_de_un_snapshot_completo_es_pequeno() {
        let m = ServerMsg::Snapshot(Snapshot {
            clock_ms: 0,
            phase: 0,
            phase_ms: 0,
            players: vec![Player { id: 1, state: estado(), finish_ms: 0 }; MAX_PLAYERS],
        });
        assert!(m.encode().len() < 300);
    }
}
