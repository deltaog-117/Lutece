//! The first step towards running as the compositor of a login: take a screen from the
//! kernel and put a picture on it, with no clients and no input yet.
//!
//! Behind the `drm` feature, which is off by default, so nested use never pulls it in.
//! [`show_test_pattern`] opens a seat, the primary GPU and one connected screen, then
//! flips a colour that turns once around the wheel; it needs a real virtual terminal.
//! What it proves is that the seat, the mode setting, the buffers and the page flips work
//! on this machine, which a later session backend builds on.

mod cursor;
mod keys;
mod pattern;
mod pick;
mod session;

pub use pattern::{PatternReport, show_test_pattern};
pub use pick::{ModeInfo, best_mode, pattern_color, pick_crtc};
pub use session::{DrmBackend, Redraw};
