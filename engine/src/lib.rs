//! Núcleo del renderer: sin ventana, sin fs, sin relojes, sin hilos.
//! Recibe `dt` e input; los modelos entran como `&[u8]`.

pub mod assets;
pub mod camera;
pub mod framebuffer;
pub mod game;
pub mod input;
pub mod math;
pub mod obj;
pub mod raster;
pub mod rng;
pub mod ship;

pub use framebuffer::Framebuffer;
pub use game::Game;
pub use input::Input;
pub use math::{Mat4, Vec3, Vec4};
pub use obj::Mesh;
pub use ship::Ship;
