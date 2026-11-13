//! Lutece: a minimal core for window managers that run on both X11 and Wayland.
//!
//! The core never talks to a display. A backend feeds it [`Event`]s and applies the
//! [`Scene`] and [`Effect`]s it produces, so the same logic and the same recorded
//! event stream behave identically on every backend.

pub mod engine;
mod float;
pub mod input;
pub mod layout;
pub mod rules;
pub mod shared;
pub mod snapshot;
pub mod stack;

pub use engine::Engine;
pub use input::{Action, Direction, Keymap};
pub use layout::{Layout, MasterRatio};
pub use rules::{ManageRule, Match, Outcome, Rules};
pub use shared::contracts::effect::{Effect, SpawnCommand};
pub use shared::contracts::event::{
    Event, KeyPress, Keysym, ManageDecision, Modifiers, Output, PropertyTarget, WindowInfo,
};
pub use shared::contracts::kernel::Kernel;
pub use shared::contracts::scene::{Placement, Scene};
pub use shared::contracts::scene_diff::{Change, diff};
pub use shared::contracts::style::{Border, Curve, Rgba, Shadow, Style, TitleBar, Transition};
pub use shared::error::{Error, Result};
pub use shared::geometry::{Insets, Point, Rect, Size};
pub use shared::ids::{OutputId, Tag, WinId};
pub use shared::names::{
    CommandName, MAX_NAME_BYTES, MAX_PROPERTIES_PER_TARGET, MAX_PROPERTY_VALUE_BYTES, PropertyKey,
    PropertyValue,
};
pub use stack::stackset::StackSet;
pub use stack::workspace::Workspace;
