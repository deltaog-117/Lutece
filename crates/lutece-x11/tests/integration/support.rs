//! Shared scaffolding: a private X server, the window manager under test, and a client.

mod client;
mod server;
mod session;

pub use client::{Client, Geometry, Spec};
pub use session::Session;

/// Screen the private server starts with.
pub const SCREEN_WIDTH: u16 = 1280;
pub const SCREEN_HEIGHT: u16 = 720;

/// Keysyms for the keys the tests press. Printable keys use their character code.
pub const SUPER: u32 = 0xffeb;
pub const SHIFT: u32 = 0xffe1;
pub const CTRL: u32 = 0xffe3;
pub const SPACE: u32 = 0x20;

/// The keysym of a printable ASCII key.
pub fn key(c: char) -> u32 {
    u32::from(c)
}
