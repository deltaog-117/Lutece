//! Scene changes and effects in, protocol messages out.
//!
//! Wayland has no map or unmap request: a client shows what it was told to show. Placing
//! a window means telling its client the size it should have; where it sits on screen
//! is the renderer's business, and a hidden window is simply not drawn.

use crate::compositor::{State, set_activated};
use crate::error::{Error, Result};
use crate::windows::Shell;
use lutece::{Change, Effect, Placement, SpawnCommand, WinId};
use smithay::utils::Rectangle;
use std::ffi::OsStr;
use std::process::Command;

/// Whether the window manager should keep running after effects were applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep going.
    Continue,
    /// An effect asked to stop the window manager.
    Quit,
}

/// Carries out the changes that turn the previous scene into the next one.
pub(crate) fn apply_changes(state: &mut State, changes: &[Change]) {
    for change in changes {
        match change {
            Change::Hide(id) => {
                if let Some(entry) = state.windows.get_mut(*id) {
                    entry.rect = None;
                }
                state.refresh_presence(*id);
            }
            Change::Show(placement) | Change::Move(placement) => place(state, placement),
            Change::Restack(order) => state.windows.restack(order),
            Change::Focus(id) => focus(state, *id),
            // `Change` may grow; a change this backend does not know is not carried out.
            _ => {}
        }
    }
}

fn place(state: &mut State, placement: &Placement) {
    let Some(entry) = state.windows.get_mut(placement.id) else {
        // The client is gone; the core learns of it from its own close event.
        return;
    };
    entry.rect = Some(placement.rect);
    let size = (
        to_protocol(placement.rect.width()),
        to_protocol(placement.rect.height()),
    );
    match &entry.shell {
        Shell::Xdg(toplevel) => {
            toplevel.with_pending_state(|pending| pending.size = Some(size.into()));
            toplevel.send_pending_configure();
        }
        // An X11 window is moved and sized in one request. If its program is gone, the
        // core learns of it from the close event.
        Shell::X11(window) => {
            let at = (placement.rect.x(), placement.rect.y());
            drop(window.configure(Rectangle::new(at.into(), size.into())));
        }
    }
    state.dismiss_misplaced_popups(placement.id);
    state.refresh_presence(placement.id);
}

/// Protocol sizes are signed; a size beyond `i32` is clamped, far past any real screen.
fn to_protocol(length: u32) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

fn focus(state: &mut State, focused: Option<WinId>) {
    for (id, entry) in state.windows.entries() {
        let active = Some(id) == focused;
        match &entry.shell {
            Shell::Xdg(toplevel) => {
                set_activated(toplevel, active);
                toplevel.send_pending_configure();
            }
            Shell::X11(window) => drop(window.set_activated(active)),
        }
    }
    let target = focused
        .and_then(|id| state.windows.get(id))
        .and_then(|entry| entry.shell.focus());
    state.set_keyboard_focus(target);
}

/// Carries out effects. `socket` is the display name spawned programs must connect to.
pub(crate) fn apply_effects(state: &mut State, socket: &OsStr, effects: &[Effect]) -> Result<Flow> {
    for effect in effects {
        match effect {
            Effect::Close(id) => {
                if let Some(entry) = state.windows.get(*id) {
                    entry.shell.close();
                }
            }
            Effect::Spawn(command) => spawn(command, socket, state.x_display)?,
            // There is no client to hand the window back to, so it is simply left out of
            // the scene from now on: never sized, never drawn.
            Effect::Release(id) => {
                state.windows.release(*id);
            }
            Effect::Quit => return Ok(Flow::Quit),
            // `Effect` may grow; an effect this backend does not know is not carried out.
            _ => {}
        }
    }
    Ok(Flow::Continue)
}

fn spawn(command: &SpawnCommand, socket: &OsStr, x_display: Option<u32>) -> Result<()> {
    let mut process = Command::new(command.program());
    process
        .args(command.args())
        // The program must reach this compositor, not the session it may be nested in.
        .env("WAYLAND_DISPLAY", socket);
    if let Some(number) = x_display {
        // Likewise for an X11 program: our Xwayland, not the session's X server.
        process.env("DISPLAY", format!(":{number}"));
    }
    let mut child = process.spawn().map_err(|source| Error::Spawn {
        program: command.program().to_owned(),
        source,
    })?;
    // Waiting on a thread reaps the child when it exits, so it never lingers as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
