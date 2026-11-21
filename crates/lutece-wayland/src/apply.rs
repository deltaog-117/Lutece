//! Scene changes and effects in, protocol messages out.
//!
//! Wayland has no map or unmap request: a client shows what it was told to show. Placing
//! a window means telling its client the size it should have; where it sits on screen
//! is the renderer's business, and a hidden window is simply not drawn.

use crate::animation::Animation;
use crate::compositor::{State, set_activated};
use crate::error::{Error, Result};
use crate::glide::{on_appear, on_style_change};
use crate::slide::{Leaving, off_right};
use crate::windows::{Entry, Shell};
use lutece::{Change, Effect, Glide, Placement, SpawnCommand, WinId};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::utils::Rectangle;
use std::ffi::OsStr;
use std::process::Command;
use std::time::Instant;

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
    // Windows whose fullscreen state changed. The state rides on the configure that
    // places the window, when there is one; the rest get theirs after all the changes.
    let mut restated = Vec::new();
    // Set when the order the windows are drawn in may have changed, so Xwayland is told once.
    let mut restack_x11 = false;
    for change in changes {
        match change {
            Change::Hide(id) => {
                if let Some(entry) = state.windows.get_mut(*id) {
                    let now = Instant::now();
                    // A window that is on screen and slides leaves by gliding off it; one
                    // that is already hidden keeps the slide it has.
                    if let Some(target) = entry.rect {
                        entry.leaving = entry
                            .style
                            .glide()
                            .contains(Glide::SLIDES)
                            .then(|| entry.style.transition())
                            .filter(|transition| transition.duration_ms > 0)
                            .zip(entry.drawn_rect(now))
                            .map(|(transition, from)| {
                                Leaving::new(target, from, &state.screens, now, transition)
                            });
                    }
                    entry.rect = None;
                    // The core forgets a hidden window's fullscreen state, so the client
                    // is told, or it would come back believing it still has it.
                    if set_fullscreen(entry, false) {
                        restated.push(*id);
                    }
                }
                state.refresh_presence(*id);
            }
            Change::Fullscreen { id, on } => {
                if let Some(entry) = state.windows.get_mut(*id)
                    && set_fullscreen(entry, *on)
                {
                    restated.push(*id);
                    restack_x11 = true;
                }
            }
            Change::Style { id, style } => {
                if let Some(entry) = state.windows.get_mut(*id) {
                    // A window that is not on screen has nothing to glide from: it starts
                    // at its new look when it appears (see `place`).
                    let now = Instant::now();
                    entry.style_glide = entry
                        .rect
                        .and_then(|_| on_style_change(&entry.drawn_style(now), style, now));
                    entry.style = *style;
                }
            }
            // A window that appears is not animated in; one that moves glides.
            Change::Show(placement) => place(state, placement, false),
            Change::Move(placement) => place(state, placement, true),
            Change::Restack(order) => {
                state.windows.restack(order);
                restack_x11 = true;
            }
            Change::Focus(id) => focus(state, *id),
            // `Change` may grow; a change this backend does not know is not carried out.
            _ => {}
        }
    }
    if restack_x11 {
        state.restack_x11();
    }
    state.sync_active_window();
    for id in &restated {
        if let Some(on) = state.windows.get(*id).map(|entry| entry.fullscreen) {
            state.foreign.set_fullscreen(*id, on);
        }
    }
    for id in restated {
        if let Some(Entry {
            shell: Shell::Xdg(toplevel),
            ..
        }) = state.windows.get(id)
        {
            // Nothing is sent if the configure that placed the window already said it.
            toplevel.send_pending_configure();
        }
    }
}

/// Tells the window's client it is fullscreen (`on`) or not. Returns whether that was a
/// change; for a Wayland window the new state is sent with its next configure.
fn set_fullscreen(entry: &mut Entry, on: bool) -> bool {
    if entry.fullscreen == on {
        return false;
    }
    entry.fullscreen = on;
    match &entry.shell {
        Shell::Xdg(toplevel) => toplevel.with_pending_state(|pending| {
            if on {
                pending.states.set(xdg_toplevel::State::Fullscreen);
            } else {
                pending.states.unset(xdg_toplevel::State::Fullscreen);
            }
        }),
        // If the program is gone, the core learns of it from the close event.
        Shell::X11(window) => drop(window.set_fullscreen(on)),
    }
    true
}

fn place(state: &mut State, placement: &Placement, animate: bool) {
    let Some(entry) = state.windows.get_mut(placement.id) else {
        // The client is gone; the core learns of it from its own close event.
        return;
    };
    // A move that comes while another is still going starts from where the window is drawn
    // now, so the window never jumps.
    let now = Instant::now();
    let from = entry.drawn_rect(now);
    let was_hidden = entry.rect.is_none();
    // A window that comes back slides in from the right, unless it is still on its way out,
    // when it simply turns round from where it is drawn.
    let sliding_in =
        was_hidden && entry.shown_before && entry.style.glide().contains(Glide::SLIDES);
    let from = from.or_else(|| sliding_in.then(|| off_right(placement.rect, &state.screens)));
    entry.rect = Some(placement.rect);
    entry.leaving = None;
    entry.shown_before = true;
    // A window that comes on screen (a new one, or one back with its workspace) fades in if
    // its style says so; one that only moves keeps the glide it has.
    if was_hidden {
        entry.style_glide = on_appear(&entry.style, now);
    }
    entry.animation = from
        .filter(|from| (animate || sliding_in) && *from != placement.rect)
        .filter(|_| entry.style.transition().duration_ms > 0)
        .map(|from| Animation::new(from, placement.rect, now, entry.style.transition()));
    // Only the placement that starts the slide is one; a later move is not, even while the
    // window is still on its way in.
    entry.sliding_in = sliding_in && entry.animation.is_some();
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
    state.foreign.set_focus(focused);
    state.window_focus = focused
        .and_then(|id| state.windows.get(id))
        .and_then(|entry| entry.shell.focus());
    // A layer surface that holds the keyboard keeps it; otherwise this window gets it.
    state.refresh_keyboard_focus();
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
