//! X11 backend for lutece: turns X events into lutece events and applies scene changes
//! as X requests.
//!
//! The backend does not run a loop of its own. A window manager connects with
//! [`X11Backend::connect`] and repeats three steps:
//!
//! ```no_run
//! use lutece::{Engine, Layout, StackSet, Tag};
//! use lutece_x11::{Flow, X11Backend};
//!
//! let mut engine = Engine::new(StackSet::new([Tag::try_new("1")?])?, Layout::default());
//! let mut backend = X11Backend::connect()?;
//! backend.grab_keys(engine.grabs())?;
//!
//! 'run: loop {
//!     for event in backend.next_events()? {
//!         if backend.apply_effects(&engine.handle(event))? == Flow::Quit {
//!             break 'run;
//!         }
//!     }
//!     backend.apply_scene(&engine.scene()?)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod apply;
mod connection;
mod error;
mod grabs;
mod translate;
mod windows;

pub use apply::Flow;
pub use connection::X11Backend;
pub use error::{Error, Result};
