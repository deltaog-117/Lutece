//! Lutece: a minimal core for window managers that run on both X11 and Wayland.
//!
//! The core never talks to a display. A backend feeds it [`Event`]s and applies the
//! [`Scene`] and [`Effect`]s it produces, so the same logic and the same recorded
//! event stream behave identically on every backend.

pub mod shared;
pub mod stack;

pub use shared::contracts::effect::{Effect, SpawnCommand};
pub use shared::contracts::event::{Event, KeyPress, Keysym, Modifiers, Output, WindowInfo};
pub use shared::contracts::scene::{Placement, Scene};
pub use shared::contracts::scene_diff::{Change, diff};
pub use shared::error::{Error, Result};
pub use shared::geometry::{Point, Rect, Size};
pub use shared::ids::{OutputId, Tag, WinId};
pub use stack::stackset::StackSet;
pub use stack::workspace::Workspace;
