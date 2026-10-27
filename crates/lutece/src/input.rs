//! Key bindings: which key combination triggers which [`Action`].
//!
//! A [`Keymap`] is plain data. The engine owns one, asks the backend to grab its key
//! combinations, and turns each press back into an action, so a recorded stream of key
//! presses replays the same way on every backend.

mod binding;
mod keymap;

pub use binding::Action;
pub use keymap::Keymap;
