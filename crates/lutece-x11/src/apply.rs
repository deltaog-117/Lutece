//! Scene changes and effects in, X requests out.
//!
//! Applying a scene is split in two: [`plan`] decides which requests a list of changes
//! needs, which can be checked without a server, and [`execute`] sends them.

use crate::connection::Atoms;
use crate::error::{Error, Result};
use crate::windows::Windows;
use lutece::{Change, Effect, Rect, SpawnCommand, WinId};
use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, InputFocus,
    StackMode, Window,
};
use x11rb::{CURRENT_TIME, NONE};

/// Whether the window manager should keep running after effects were applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep going.
    Continue,
    /// An effect asked to stop the window manager.
    Quit,
}

/// One X request, described without a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Place { window: Window, rect: Rect },
    Map(Window),
    Unmap(Window),
    Raise(Window),
    Focus(Option<Window>),
}

fn window_of(id: WinId) -> Result<Window> {
    Window::try_from(id.get()).map_err(|_| Error::ForeignWindow(id))
}

/// Works out the requests that carry out `changes`, and records in `windows` what they
/// will put on screen.
///
/// Changes for windows that are no longer managed are skipped: the client is gone, so
/// there is nothing left to place, and the core learns of it from its own close event.
pub(crate) fn plan(windows: &mut Windows, changes: &[Change]) -> Result<Vec<Op>> {
    let mut ops = Vec::with_capacity(changes.len());
    for change in changes {
        match change {
            Change::Hide(id) => {
                let window = window_of(*id)?;
                if windows.mark_hidden(window) {
                    ops.push(Op::Unmap(window));
                }
            }
            Change::Show(placement) => {
                let window = window_of(placement.id)?;
                if windows.is_managed(window) {
                    // Placed before it is mapped, so it never flashes at a stale spot.
                    ops.push(Op::Place {
                        window,
                        rect: placement.rect,
                    });
                    ops.push(Op::Map(window));
                    windows.mark_mapped(window);
                }
            }
            Change::Move(placement) => {
                let window = window_of(placement.id)?;
                if windows.is_managed(window) {
                    ops.push(Op::Place {
                        window,
                        rect: placement.rect,
                    });
                }
            }
            Change::Restack(order) => {
                // Raising each window in turn, bottom first, leaves the last on top.
                for id in order {
                    let window = window_of(*id)?;
                    if windows.is_managed(window) {
                        ops.push(Op::Raise(window));
                    }
                }
            }
            Change::Focus(id) => {
                let window = id.map(window_of).transpose()?;
                ops.push(Op::Focus(window.filter(|w| windows.is_managed(*w))));
            }
            // `Change` may grow; a change this backend does not know needs no request.
            _ => {}
        }
    }
    Ok(ops)
}

/// Sends `ops` to the X server. Requests for windows that vanished meanwhile fail on the
/// server side and arrive later as error events, which the translator ignores.
pub(crate) fn execute(conn: &impl Connection, root: Window, ops: &[Op]) -> Result<()> {
    for op in ops {
        match *op {
            Op::Place { window, rect } => {
                let geometry = ConfigureWindowAux::new()
                    .x(rect.x())
                    .y(rect.y())
                    .width(rect.width())
                    .height(rect.height())
                    .border_width(0);
                conn.configure_window(window, &geometry)?;
            }
            Op::Map(window) => {
                conn.map_window(window)?;
            }
            Op::Unmap(window) => {
                conn.unmap_window(window)?;
            }
            Op::Raise(window) => {
                conn.configure_window(
                    window,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
            }
            Op::Focus(Some(window)) => {
                conn.set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?;
            }
            // Nothing focused: hand keyboard input back to the root window.
            Op::Focus(None) => {
                conn.set_input_focus(InputFocus::POINTER_ROOT, root, CURRENT_TIME)?;
            }
        }
    }
    conn.flush()?;
    Ok(())
}

/// Carries out `effects` in order, stopping at the first [`Effect::Quit`].
pub(crate) fn run_effects(
    conn: &impl Connection,
    atoms: &Atoms,
    windows: &mut Windows,
    effects: &[Effect],
) -> Result<Flow> {
    for effect in effects {
        match effect {
            Effect::Close(id) => close(conn, atoms, window_of(*id)?)?,
            Effect::Spawn(command) => spawn(command)?,
            Effect::Release(id) => release(conn, windows, window_of(*id)?)?,
            Effect::Quit => return Ok(Flow::Quit),
            // `Effect` may grow; an effect this backend does not know is not carried out.
            _ => {}
        }
    }
    conn.flush()?;
    Ok(Flow::Continue)
}

/// Hands `window` back to its client. It was held back at its map request, so it is
/// shown now, and everything else we knew about it is dropped.
fn release(conn: &impl Connection, windows: &mut Windows, window: Window) -> Result<()> {
    windows.forget(window);
    conn.map_window(window)?;
    Ok(())
}

/// Asks the client to close itself, and kills its connection only if it cannot be asked.
fn close(conn: &impl Connection, atoms: &Atoms, window: Window) -> Result<()> {
    let protocols = conn
        .get_property(false, window, atoms.WM_PROTOCOLS, AtomEnum::ATOM, 0, 32)?
        .reply();
    // A refused request means the window is already gone; killing it then is a harmless no-op.
    let supports_delete = protocols
        .ok()
        .and_then(|reply| {
            reply
                .value32()
                .map(|mut atoms_set| atoms_set.any(|a| a == atoms.WM_DELETE_WINDOW))
        })
        .unwrap_or(false);

    if supports_delete {
        let message = ClientMessageEvent::new(
            32,
            window,
            atoms.WM_PROTOCOLS,
            [atoms.WM_DELETE_WINDOW, CURRENT_TIME, NONE, NONE, NONE],
        );
        conn.send_event(false, window, EventMask::NO_EVENT, message)?;
    } else {
        conn.kill_client(window)?;
    }
    Ok(())
}

fn spawn(command: &SpawnCommand) -> Result<()> {
    let mut child = Command::new(command.program())
        .args(command.args())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::win_id;
    use lutece::{Placement, Scene, diff};
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};

    /// The state a real X server would end up in, as far as the scene is concerned.
    #[derive(Default)]
    struct FakeServer {
        mapped: HashSet<Window>,
        rects: HashMap<Window, Rect>,
        /// Every window we ever mapped, bottom to top, whether or not it is mapped now.
        stacking: Vec<Window>,
        focus: Option<Window>,
    }

    impl FakeServer {
        fn run(&mut self, ops: &[Op]) {
            for op in ops {
                match *op {
                    Op::Place { window, rect } => {
                        self.rects.insert(window, rect);
                    }
                    Op::Map(window) => {
                        self.mapped.insert(window);
                        if !self.stacking.contains(&window) {
                            self.stacking.push(window);
                        }
                    }
                    Op::Unmap(window) => {
                        self.mapped.remove(&window);
                    }
                    Op::Raise(window) => {
                        self.stacking.retain(|w| *w != window);
                        self.stacking.push(window);
                    }
                    Op::Focus(window) => self.focus = window,
                }
            }
        }

        fn visible_order(&self) -> Vec<Window> {
            self.stacking
                .iter()
                .copied()
                .filter(|w| self.mapped.contains(w))
                .collect()
        }
    }

    fn scene_strategy() -> impl Strategy<Value = Scene> {
        (
            proptest::collection::vec(
                (
                    0u32..8,
                    (-500i32..500, -500i32..500, 1u32..2000, 1u32..2000),
                ),
                0..8,
            ),
            proptest::option::of(any::<usize>()),
        )
            .prop_map(|(raw, pick)| {
                let mut seen = HashSet::new();
                let placements: Vec<Placement> = raw
                    .into_iter()
                    .filter(|(id, _)| seen.insert(*id))
                    .map(|(id, (x, y, w, h))| Placement {
                        id: win_id(id),
                        rect: Rect::new(x, y, w, h).expect("generated non-zero size"),
                    })
                    .collect();
                let focus = pick
                    .filter(|_| !placements.is_empty())
                    .map(|pick| placements[pick % placements.len()].id);
                Scene::try_new(placements, focus).expect("ids are deduplicated, focus is listed")
            })
    }

    proptest! {
        /// After every scene in a sequence, the server shows exactly that scene.
        #[test]
        fn the_planned_requests_reach_every_scene(
            scenes in proptest::collection::vec(scene_strategy(), 1..6),
        ) {
            let mut windows = Windows::default();
            for id in 0..8 {
                windows.manage(id);
            }
            let mut server = FakeServer::default();
            let mut previous = Scene::empty();

            for scene in scenes {
                let ops = plan(&mut windows, &diff(&previous, &scene)).expect("ids fit in u32");
                server.run(&ops);

                let shown: HashSet<Window> =
                    scene.placements().iter().map(|p| window_of(p.id).expect("small id")).collect();
                prop_assert_eq!(&server.mapped, &shown);
                for p in scene.placements() {
                    prop_assert_eq!(server.rects[&window_of(p.id).expect("small id")], p.rect);
                }
                let order: Vec<Window> =
                    scene.placements().iter().map(|p| window_of(p.id).expect("small id")).collect();
                prop_assert_eq!(server.visible_order(), order);
                prop_assert_eq!(server.focus, scene.focus().map(|id| window_of(id).expect("small id")));
                previous = scene;
            }
        }
    }

    #[test]
    fn an_id_too_large_for_x_is_rejected() {
        let mut windows = Windows::default();
        let err = plan(&mut windows, &[Change::Hide(WinId::new(u64::MAX))]).expect_err("too large");
        assert!(matches!(err, Error::ForeignWindow(id) if id == WinId::new(u64::MAX)));
    }

    #[test]
    fn a_window_that_is_gone_is_not_shown() {
        let mut windows = Windows::default();
        let rect = Rect::new(0, 0, 10, 10).expect("non-zero size");
        let show = Change::Show(Placement {
            id: win_id(3),
            rect,
        });
        assert_eq!(plan(&mut windows, &[show]).expect("fits"), []);
    }
}
