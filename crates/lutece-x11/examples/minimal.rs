//! A window manager in one file: every window is tiled on a single workspace.
//!
//! Run it on a spare display, for example `Xephyr :2`, then start clients with
//! `DISPLAY=:2 xterm`. Stop it with Ctrl-C or by closing the Xephyr window.
//!
//! ```text
//! DISPLAY=:2 cargo run -p lutece-x11 --example minimal
//! ```

use lutece::{Engine, Layout, StackSet, Tag};
use lutece_x11::{Flow, X11Backend};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new(StackSet::new([Tag::try_new("1")?])?, Layout::default());
    let mut backend = X11Backend::connect()?;

    loop {
        for event in backend.next_events()? {
            if backend.apply_effects(&engine.handle(event))? == Flow::Quit {
                return Ok(());
            }
        }
        backend.apply_scene(&engine.scene()?)?;
    }
}
