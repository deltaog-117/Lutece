//! Wayland backend for lutece, built on Smithay.
//!
//! The backend is a compositor the caller drives, as with the X11 backend: it does not
//! run a loop of its own. A window manager binds it and repeats three steps:
//!
//! ```no_run
//! use lutece::{Engine, Event, Layout, Size, StackSet, Tag};
//! use lutece_wayland::{Flow, WaylandBackend};
//! use std::time::Duration;
//!
//! let mut engine = Engine::new(StackSet::new([Tag::try_new("1")?])?, Layout::default());
//! let mut backend = WaylandBackend::bind(Size::new(1280, 800)?)?;
//! backend.grab_keys(engine.grabs());
//!
//! 'run: loop {
//!     for event in backend.next_events(Duration::from_millis(100))? {
//!         if backend.apply_effects(&engine.handle(event))? == Flow::Quit {
//!             break 'run;
//!         }
//!     }
//!     backend.apply_scene(&engine.scene()?)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod animation;
mod apply;
mod backend;
mod compositor;
mod dmabuf;
#[cfg(feature = "drm")]
mod drm;
mod error;
mod focus;
mod input;
mod layers;
mod look;
mod nested;
mod outline;
mod pointer;
mod popup;
mod presence;
mod render;
mod scale;
mod selection;
mod title;
mod translate;
mod windows;
mod xwayland;

pub use apply::Flow;
pub use backend::{ButtonAction, KeyAction, WaylandBackend};
pub use dmabuf::DmabufSupport;
#[cfg(feature = "drm")]
pub use drm::{
    DrmBackend, ModeInfo, PatternReport, best_mode, pattern_color, pick_crtc, show_test_pattern,
};
pub use error::{Error, Result};
pub use nested::NestedBackend;
pub use outline::Outline;
pub use scale::Scale;
