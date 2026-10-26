//! The messages that cross the line between the core and a backend.
//!
//! A backend sends [`event::Event`]s in and applies the [`scene::Scene`] (through
//! [`scene_diff::diff`]) and the [`effect::Effect`]s that come back.

pub mod effect;
pub mod event;
pub mod scene;
pub mod scene_diff;
