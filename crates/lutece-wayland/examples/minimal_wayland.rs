//! A window manager in one file: Wayland clients are tiled on nine workspaces inside a
//! window of the session you run it in, and driven by keys.
//!
//! The keys use Alt, because the session around the window usually keeps Super for
//! itself. Start clients against the socket the example prints, for example
//! `WAYLAND_DISPLAY=lutece-0 alacritty`; Alt+Return does that for you.
//!
//! | Keys | Action |
//! |---|---|
//! | Alt+Return | start `$LUTECE_TERMINAL`, or `alacritty` |
//! | Alt+j / Alt+k | focus the next / previous window |
//! | Alt+Shift+j / Alt+Shift+k | swap the focused window with the next / previous |
//! | Alt+1..9 | show that workspace |
//! | Alt+space | switch the workspace to its next layout |
//! | Alt+h / Alt+l | narrow / widen the master column |
//! | Alt+Shift+1..9 | send the focused window to that workspace |
//! | Alt+f | float the focused window, or tile it again |
//! | Alt+Shift+f | make the focused window fullscreen, or put it back |
//! | Alt+, / Alt+. | focus the previous / next output |
//! | Alt+Shift+, / Alt+Shift+. | send the focused window to the previous / next output |
//! | Alt+left-drag | move the window under the pointer, if it floats |
//! | Alt+right-drag | resize the window under the pointer, if it floats |
//! | Alt+Shift+c | close the focused window |
//! | Alt+Shift+q | quit |
//!
//! Focus follows the pointer. Copy and paste work between clients, and popups such as
//! menus are shown above their window and kept on its output. Clients are asked to draw no
//! title bar of their own.
//!
//! X11 programs run too, through Xwayland, and their windows are tiled like the others.
//! Programs started with Alt+Return find it through `DISPLAY`.
//!
//! The window can stand for several monitors: `LUTECE_OUTPUTS=2` cuts it into two outputs
//! side by side.
//!
//! ```text
//! cargo run -p lutece-wayland --example minimal_wayland
//! ```

use lutece::{
    Action, Engine, KeyPress, Keymap, Keysym, Layout, Modifiers, SpawnCommand, StackSet, Tag,
};
use lutece_wayland::{Error, Flow, NestedBackend};
use std::num::NonZeroU32;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const MOD: Modifiers = Modifiers::ALT;
const IDLE_WAIT: Duration = Duration::from_millis(100);

fn keymap(tags: &[Tag]) -> Result<Keymap> {
    let shifted = MOD | Modifiers::SHIFT;
    let key = |c: char, modifiers| -> Result<KeyPress> {
        let keysym = Keysym::from_char(c).ok_or("key is not a printable character")?;
        Ok(KeyPress { keysym, modifiers })
    };
    let terminal = std::env::var("LUTECE_TERMINAL").unwrap_or_else(|_| "alacritty".to_owned());

    let mut keymap = Keymap::new();
    keymap.bind(
        KeyPress {
            keysym: Keysym::RETURN,
            modifiers: MOD,
        },
        Action::Spawn(SpawnCommand::try_new(terminal, Vec::<String>::new())?),
    )?;
    keymap.bind(key('j', MOD)?, Action::FocusNext)?;
    keymap.bind(key('k', MOD)?, Action::FocusPrev)?;
    keymap.bind(key('j', shifted)?, Action::SwapNext)?;
    keymap.bind(key('k', shifted)?, Action::SwapPrev)?;
    keymap.bind(key(' ', MOD)?, Action::NextLayout)?;
    keymap.bind(key('h', MOD)?, Action::ShrinkMaster)?;
    keymap.bind(key('l', MOD)?, Action::GrowMaster)?;
    keymap.bind(key('f', MOD)?, Action::ToggleFloat)?;
    keymap.bind(key('f', shifted)?, Action::ToggleFullscreen)?;
    keymap.bind(key(',', MOD)?, Action::FocusPrevOutput)?;
    keymap.bind(key('.', MOD)?, Action::FocusNextOutput)?;
    keymap.bind(key(',', shifted)?, Action::SendToPrevOutput)?;
    keymap.bind(key('.', shifted)?, Action::SendToNextOutput)?;
    keymap.bind(key('c', shifted)?, Action::Close)?;
    keymap.bind(key('q', shifted)?, Action::Quit)?;
    for (digit, tag) in ('1'..='9').zip(tags) {
        keymap.bind(key(digit, MOD)?, Action::ViewTag(tag.clone()))?;
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
    let outputs = std::env::var("LUTECE_OUTPUTS")
        .ok()
        .and_then(|count| count.parse::<NonZeroU32>().ok())
        .unwrap_or(NonZeroU32::MIN);
    let mut backend = NestedBackend::open_with_outputs(outputs)?;
    backend.grab_keys(engine.grabs());
    backend.set_drag_modifier(MOD);
    eprintln!(
        "lutece: clients connect with WAYLAND_DISPLAY={}",
        backend.socket_name().to_string_lossy()
    );

    if let Err(error) = backend.enable_xwayland() {
        eprintln!("lutece: no X11 programs: {error}");
    }
    let mut said_display = false;

    while !backend.is_closed() {
        if !said_display && let Some(number) = backend.xwayland_display() {
            eprintln!("lutece: X11 clients connect with DISPLAY=:{number}");
            said_display = true;
        }
        let events = match backend.next_events(IDLE_WAIT) {
            // Wayland programs still work without X11 ones.
            Err(error @ Error::XWayland(_)) => {
                eprintln!("lutece: no X11 programs: {error}");
                continue;
            }
            other => other?,
        };
        for event in events {
            if backend.apply_effects(&engine.handle(event))? == Flow::Quit {
                return Ok(());
            }
        }
        backend.apply_scene(&engine.scene()?)?;
        backend.render()?;
    }
    Ok(())
}
