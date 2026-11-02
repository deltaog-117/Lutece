//! Scene changes and effects in, protocol messages out.
//!
//! Wayland has no map or unmap request: a client shows what it was told to show. Placing
//! a window means telling its client the size it should have; where it sits on screen
//! is the renderer's business, and a hidden window is simply not drawn.

use crate::compositor::{State, set_activated};
use crate::error::{Error, Result};
use lutece::{Change, Effect, Placement, SpawnCommand, WinId};
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
    entry
        .toplevel
        .with_pending_state(|pending| pending.size = Some(size.into()));
    entry.toplevel.send_pending_configure();
}

/// Protocol sizes are signed; a size beyond `i32` is clamped, far past any real screen.
fn to_protocol(length: u32) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

fn focus(state: &mut State, focused: Option<WinId>) {
    for (id, entry) in state.windows.entries() {
        set_activated(&entry.toplevel, Some(id) == focused);
        entry.toplevel.send_pending_configure();
    }
    let surface = focused
        .and_then(|id| state.windows.get(id))
        .map(|entry| entry.toplevel.wl_surface().clone());
    state.set_keyboard_focus(surface);
}

/// Carries out effects. `socket` is the display name spawned programs must connect to.
pub(crate) fn apply_effects(state: &mut State, socket: &OsStr, effects: &[Effect]) -> Result<Flow> {
    for effect in effects {
        match effect {
            Effect::Close(id) => {
                if let Some(entry) = state.windows.get(*id) {
                    entry.toplevel.send_close();
                }
            }
            Effect::Spawn(command) => spawn(command, socket)?,
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

fn spawn(command: &SpawnCommand, socket: &OsStr) -> Result<()> {
    let mut child = Command::new(command.program())
        .args(command.args())
        // The program must reach this compositor, not the session it may be nested in.
        .env("WAYLAND_DISPLAY", socket)
        .spawn()
        .map_err(|source| Error::Spawn {
            program: command.program().to_owned(),
            source,
        })?;
    // Waiting on a thread reaps the child when it exits, so it never lingers as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
