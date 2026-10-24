//! Mensajes entre el navegador y el servidor. Binario, sin dependencias.
//! El servidor solo reenvía posiciones: cada cliente simula su propia nave.

pub const MAX_PLAYERS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub model: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub id: u32,
    pub state: PlayerState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    /// Primer mensaje: el id que el servidor asignó a este cliente.
    Welcome { id: u32 },
    /// Estado de todos los jugadores (incluido el propio).
    Snapshot(Vec<Player>),
}

const TAG_STATE: u8 = 1;
const TAG_WELCOME: u8 = 1;
const TAG_SNAPSHOT: u8 = 2;

fn put_state(out: &mut Vec<u8>, s: &PlayerState) {
    for v in s.position {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&s.yaw.to_le_bytes());
    out.extend_from_slice(&s.pitch.to_le_bytes());
    out.push(s.model);
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take::<4>().map(f32::from_le_bytes)
    }
    fn state(&mut self) -> Option<PlayerState> {
        Some(PlayerState {
            position: [self.f32()?, self.f32()?, self.f32()?],
            yaw: self.f32()?,
            pitch: self.f32()?,
            model: self.take::<1>()?[0],
        })
    }
}

/// Cliente → servidor: mi estado actual.
pub fn encode_client(state: &PlayerState) -> Vec<u8> {
    let mut out = vec![TAG_STATE];
    put_state(&mut out, state);
    out
}

pub fn decode_client(bytes: &[u8]) -> Option<PlayerState> {
    let mut r = Reader(bytes);
    if r.take::<1>()?[0] != TAG_STATE {
        return None;
    }
    let s = r.state()?;
    r.0.is_empty().then_some(s)
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ServerMsg::Welcome { id } => {
                let mut out = vec![TAG_WELCOME];
                out.extend_from_slice(&id.to_le_bytes());
                out
            }
            ServerMsg::Snapshot(players) => {
                let mut out = vec![TAG_SNAPSHOT];
                out.extend_from_slice(&(players.len() as u16).to_le_bytes());
                for p in players {
                    out.extend_from_slice(&p.id.to_le_bytes());
                    put_state(&mut out, &p.state);
                }
                out
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<ServerMsg> {
        let mut r = Reader(bytes);
        let msg = match r.take::<1>()?[0] {
            TAG_WELCOME => ServerMsg::Welcome { id: u32::from_le_bytes(r.take::<4>()?) },
            TAG_SNAPSHOT => {
                let n = u16::from_le_bytes(r.take::<2>()?) as usize;
                let mut players = Vec::with_capacity(n.min(MAX_PLAYERS));
                for _ in 0..n {
                    let id = u32::from_le_bytes(r.take::<4>()?);
                    players.push(Player { id, state: r.state()? });
                }
                ServerMsg::Snapshot(players)
            }
            _ => return None,
        };
        r.0.is_empty().then_some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado() -> PlayerState {
        PlayerState { position: [1.5, -2.0, 30.25], yaw: 0.5, pitch: -0.25, model: 2 }
    }

    #[test]
    fn el_estado_del_cliente_ida_y_vuelta() {
        assert_eq!(decode_client(&encode_client(&estado())), Some(estado()));
    }

    #[test]
    fn welcome_ida_y_vuelta() {
        let m = ServerMsg::Welcome { id: 7 };
        assert_eq!(ServerMsg::decode(&m.encode()), Some(m));
    }

    #[test]
    fn snapshot_ida_y_vuelta() {
        let m = ServerMsg::Snapshot(vec![Player { id: 1, state: estado() }, Player { id: 9, state: estado() }]);
        assert_eq!(ServerMsg::decode(&m.encode()), Some(m));
    }

    #[test]
    fn rechaza_mensajes_truncados_vacios_o_con_basura() {
        let ok = encode_client(&estado());
        assert_eq!(decode_client(&ok[..ok.len() - 1]), None);
        assert_eq!(decode_client(&[]), None);
        assert_eq!(decode_client(&[9, 9, 9]), None);
        let mut largo = ok.clone();
        largo.push(0);
        assert_eq!(decode_client(&largo), None);
        let snap = ServerMsg::Snapshot(vec![Player { id: 1, state: estado() }]).encode();
        assert_eq!(ServerMsg::decode(&snap[..snap.len() - 3]), None);
    }

    #[test]
    fn un_snapshot_que_miente_sobre_su_tamano_no_revienta() {
        assert_eq!(ServerMsg::decode(&[TAG_SNAPSHOT, 0xFF, 0xFF]), None);
    }
}
