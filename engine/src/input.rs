//! Entrada abstracta: native y web la traducen desde teclado a estos bits.

pub mod bits {
    pub const FORWARD: u32 = 1;
    pub const BACKWARD: u32 = 2;
    pub const YAW_LEFT: u32 = 4;
    pub const YAW_RIGHT: u32 = 8;
    pub const PITCH_UP: u32 = 16;
    pub const PITCH_DOWN: u32 = 32;
    pub const BRAKE: u32 = 64;
    /// Se envía solo en el cuadro en que se presiona la tecla.
    pub const CYCLE_MODEL: u32 = 128;
    /// Se envía solo en el cuadro en que se presiona la tecla (Enter).
    pub const START: u32 = 256;
    /// Se envía solo en el cuadro en que se presiona la tecla (M).
    pub const MENU: u32 = 512;
    /// Teclas de una pulsación: G gravedad, O órbita, V vista, R semilla de los objetos.
    pub const GRAVITY: u32 = 1024;
    pub const ORBIT: u32 = 2048;
    pub const VIEW: u32 = 4096;
    pub const RESEED: u32 = 8192;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub forward: bool,
    pub backward: bool,
    pub yaw_left: bool,
    pub yaw_right: bool,
    pub pitch_up: bool,
    pub pitch_down: bool,
    pub brake: bool,
    pub cycle_model: bool,
    pub start: bool,
    pub menu: bool,
    pub toggle_gravity: bool,
    pub toggle_orbit: bool,
    pub toggle_view: bool,
    pub reseed: bool,
}

impl Input {
    pub fn from_bits(b: u32) -> Self {
        Self {
            forward: b & bits::FORWARD != 0,
            backward: b & bits::BACKWARD != 0,
            yaw_left: b & bits::YAW_LEFT != 0,
            yaw_right: b & bits::YAW_RIGHT != 0,
            pitch_up: b & bits::PITCH_UP != 0,
            pitch_down: b & bits::PITCH_DOWN != 0,
            brake: b & bits::BRAKE != 0,
            cycle_model: b & bits::CYCLE_MODEL != 0,
            start: b & bits::START != 0,
            menu: b & bits::MENU != 0,
            toggle_gravity: b & bits::GRAVITY != 0,
            toggle_orbit: b & bits::ORBIT != 0,
            toggle_view: b & bits::VIEW != 0,
            reseed: b & bits::RESEED != 0,
        }
    }
}
