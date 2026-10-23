//! Núcleo del renderer: sin ventana, sin fs, sin relojes, sin hilos.
//! Recibe `dt` e input; los modelos entran como `&[u8]`.

pub mod framebuffer;
pub mod game;

pub use framebuffer::Framebuffer;
pub use game::Game;
