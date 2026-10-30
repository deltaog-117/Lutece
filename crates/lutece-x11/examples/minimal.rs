//! A window manager in one file: windows are tiled on nine workspaces and driven by keys.
//!
//! Run it on a spare display, for example `Xephyr :2`, then start clients with
//! `DISPLAY=:2 xterm`. Keys (Super is the logo key):
//!
//! | Keys | Action |
//! |---|---|
//! | Super+Return | start `xterm` |
//! | Super+j / Super+k | focus the next / previous window |
//! | Super+Shift+j / Super+Shift+k | swap the focused window with the next / previous |
//! | Super+1..9 | show that workspace |
//! | Super+space | switch the workspace to its next layout |
//! | Super+h / Super+l | narrow / widen the master column |
//! | Super+Shift+1..9 | send the focused window to that workspace |
//! | Super+Shift+c | close the focused window |
//! | Super+Shift+q | quit |
//!
//! ```text
//! DISPLAY=:2 cargo run -p lutece-x11 --example minimal
//! ```

use lutece::{
    Action, Engine, KeyPress, Keymap, Keysym, Layout, Modifiers, SpawnCommand, StackSet, Tag,
};
use lutece_x11::{Flow, X11Backend};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn keymap(tags: &[Tag]) -> Result<Keymap> {
    let shifted = Modifiers::SUPER | Modifiers::SHIFT;
    let key = |c: char, modifiers| -> Result<KeyPress> {
        let keysym = Keysym::from_char(c).ok_or("key is not a printable character")?;
        Ok(KeyPress { keysym, modifiers })
    };

    let mut keymap = Keymap::new();
    keymap.bind(
        KeyPress {
            keysym: Keysym::RETURN,
            modifiers: Modifiers::SUPER,
        },
        Action::Spawn(SpawnCommand::try_new("xterm", Vec::<String>::new())?),
    )?;
    keymap.bind(key('j', Modifiers::SUPER)?, Action::FocusNext)?;
    keymap.bind(key('k', Modifiers::SUPER)?, Action::FocusPrev)?;
    keymap.bind(key('j', shifted)?, Action::SwapNext)?;
    keymap.bind(key('k', shifted)?, Action::SwapPrev)?;
    keymap.bind(key(' ', Modifiers::SUPER)?, Action::NextLayout)?;
    keymap.bind(key('h', Modifiers::SUPER)?, Action::ShrinkMaster)?;
    keymap.bind(key('l', Modifiers::SUPER)?, Action::GrowMaster)?;
    keymap.bind(key('c', shifted)?, Action::Close)?;
    keymap.bind(key('q', shifted)?, Action::Quit)?;
    for (digit, tag) in ('1'..='9').zip(tags) {
        keymap.bind(key(digit, Modifiers::SUPER)?, Action::ViewTag(tag.clone()))?;
        keymap.bind(key(digit, shifted)?, Action::MoveToTag(tag.clone()))?;
    }
    Ok(keymap)
}

fn main() -> Result<()> {
    let tags = ('1'..='9')
        .map(|digit| Tag::try_new(digit.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut engine =
        Engine::new(StackSet::new(tags.clone())?, Layout::default()).with_keymap(keymap(&tags)?);
    let mut backend = X11Backend::connect()?;
    backend.grab_keys(engine.grabs())?;

    loop {
        for event in backend.next_events()? {
            if backend.apply_effects(&engine.handle(event))? == Flow::Quit {
                return Ok(());
            }
        }
        backend.apply_scene(&engine.scene()?)?;
    }
}
