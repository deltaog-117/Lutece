//! The toplevel windows clients have created, and the ids the core knows them by.
//!
//! A toplevel starts out *pending*: its client has created it but not yet committed, so
//! its title and app id are not final. The first commit announces it to the core and
//! makes it *managed*.
//!
//! A window of an X11 program (through Xwayland) is the same to the core. It waits as
//! *pending* until its X11 map request has been granted and its surface has been
//! committed, and is then announced. X11 menus and tooltips (*override-redirect* windows)
//! place themselves and are never managed: they are drawn above everything, where their
//! program put them.

use crate::animation::Animation;
use lutece::{Rect, Style, WinId};
use smithay::desktop::PopupManager;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{SurfaceCachedState, ToplevelSurface};
use smithay::xwayland::X11Surface;
use std::collections::HashMap;
use std::time::Instant;

/// The client behind a window: a Wayland toplevel, or an X11 window of Xwayland.
#[derive(Debug, Clone)]
pub(crate) enum Shell {
    Xdg(ToplevelSurface),
    X11(X11Surface),
}

impl Shell {
    /// The window's surface. An X11 window has none until its surface is committed.
    pub(crate) fn wl_surface(&self) -> Option<WlSurface> {
        match self {
            Self::Xdg(toplevel) => Some(toplevel.wl_surface().clone()),
            Self::X11(window) => window.wl_surface(),
        }
    }

    /// Asks the client to close the window.
    pub(crate) fn close(&self) {
        match self {
            Self::Xdg(toplevel) => toplevel.send_close(),
            // The window may already be gone; the core hears of that from its own event.
            Self::X11(window) => drop(window.close()),
        }
    }
}

/// A managed window and where the core last put it.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) shell: Shell,
    /// `None` while the window is hidden.
    pub(crate) rect: Option<Rect>,
    /// Whether the client has been told it is fullscreen.
    pub(crate) fullscreen: bool,
    /// The look the core gave the window.
    pub(crate) style: Style,
    /// The window on its way to `rect`, while it moves with a transition.
    pub(crate) animation: Option<Animation>,
}

impl Entry {
    /// The rectangle to draw at `now`: where the window is on its way to, or the rectangle
    /// it has reached. `None` while it is hidden.
    pub(crate) fn drawn_rect(&self, now: Instant) -> Option<Rect> {
        let target = self.rect?;
        Some(match &self.animation {
            Some(animation) if !animation.finished(now) => animation.rect_at(now),
            _ => target,
        })
    }
}

/// One surface tree that is on screen: a window, or a popup that belongs to one.
pub(crate) struct Layer {
    /// The window it belongs to; `None` for an X11 menu, which belongs to no managed
    /// window.
    pub(crate) id: Option<WinId>,
    pub(crate) surface: WlSurface,
    /// Where the surface's own origin is on the output.
    pub(crate) origin: Point<i32, Logical>,
    pub(crate) popup: bool,
    /// How a window's own surface is drawn: its style, and where it is drawn compared with
    /// where the core placed it. `None` for popups, menus and layer surfaces.
    pub(crate) look: Option<Look>,
}

/// The look of a placed window's surface.
pub(crate) struct Look {
    pub(crate) style: Style,
    /// The rectangle the core gave the window, which its client sized itself to.
    pub(crate) target: Rectangle<i32, Logical>,
    /// The rectangle the window is drawn in now, which differs from `target` while it moves.
    pub(crate) drawn: Rectangle<i32, Logical>,
}

/// Every toplevel this backend knows about.
///
/// Invariant: `order` lists exactly the keys of `managed`, each once.
#[derive(Debug, Default)]
pub(crate) struct Windows {
    next_id: u64,
    pending: Vec<ToplevelSurface>,
    /// X11 windows whose map request was granted and whose surface is not committed yet.
    pending_x11: Vec<X11Surface>,
    /// Mapped X11 menus and tooltips, oldest first.
    override_redirect: Vec<X11Surface>,
    managed: HashMap<WinId, Entry>,
    /// Stacking order, bottom to top.
    order: Vec<WinId>,
    /// Toplevels that asked for fullscreen before they were announced, as a player that
    /// starts fullscreen does. The request is passed on when the window opens.
    early_fullscreen: Vec<WlSurface>,
    /// X11 windows (by their X window number) that set the fullscreen state before they
    /// mapped. Like `early_fullscreen`, passed on when the window opens.
    early_x11_fullscreen: Vec<u32>,
}

impl Windows {
    /// Remembers a toplevel until its first commit.
    pub(crate) fn track(&mut self, toplevel: ToplevelSurface) {
        self.pending.push(toplevel);
    }

    /// Moves the pending toplevel behind `surface`, if there is one, to the managed set.
    pub(crate) fn announce(&mut self, surface: &WlSurface) -> Option<(WinId, ToplevelSurface)> {
        let at = self
            .pending
            .iter()
            .position(|toplevel| toplevel.wl_surface() == surface)?;
        let toplevel = self.pending.swap_remove(at);
        let id = self.manage(Shell::Xdg(toplevel.clone()));
        Some((id, toplevel))
    }

    fn manage(&mut self, shell: Shell) -> WinId {
        let id = WinId::new(self.next_id);
        self.next_id += 1;
        self.order.push(id);
        self.managed.insert(
            id,
            Entry {
                shell,
                rect: None,
                fullscreen: false,
                style: Style::default(),
                animation: None,
            },
        );
        id
    }

    /// Remembers that a toplevel that is not announced yet asked for fullscreen (`on`),
    /// or changed its mind.
    pub(crate) fn remember_fullscreen(&mut self, surface: &WlSurface, on: bool) {
        self.early_fullscreen.retain(|asked| asked != surface);
        if on {
            self.early_fullscreen.push(surface.clone());
        }
    }

    /// Whether `surface` asked for fullscreen before it was announced; forgets the ask.
    pub(crate) fn take_early_fullscreen(&mut self, surface: &WlSurface) -> bool {
        let before = self.early_fullscreen.len();
        self.early_fullscreen.retain(|asked| asked != surface);
        self.early_fullscreen.len() != before
    }

    /// Remembers that the X11 window numbered `window` set the fullscreen state before it
    /// mapped.
    pub(crate) fn remember_x11_fullscreen(&mut self, window: u32) {
        if !self.early_x11_fullscreen.contains(&window) {
            self.early_x11_fullscreen.push(window);
        }
    }

    /// Whether the X11 window numbered `window` set the fullscreen state before it mapped;
    /// forgets the ask.
    pub(crate) fn take_early_x11_fullscreen(&mut self, window: u32) -> bool {
        let before = self.early_x11_fullscreen.len();
        self.early_x11_fullscreen.retain(|asked| *asked != window);
        self.early_x11_fullscreen.len() != before
    }

    /// Remembers an X11 window whose map request was granted, until its surface is
    /// committed. Asking twice for the same window changes nothing.
    pub(crate) fn track_x11(&mut self, window: X11Surface) {
        let known = self.id_of_x11(window.window_id()).is_some()
            || self
                .pending_x11
                .iter()
                .any(|pending| pending.window_id() == window.window_id());
        if !known {
            self.pending_x11.push(window);
        }
    }

    /// Moves every pending X11 window that has a committed surface to the managed set.
    pub(crate) fn announce_x11(&mut self) -> Vec<(WinId, X11Surface)> {
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_x11)
            .into_iter()
            .partition(|window| window.wl_surface().is_some());
        self.pending_x11 = waiting;
        ready
            .into_iter()
            .map(|window| (self.manage(Shell::X11(window.clone())), window))
            .collect()
    }

    /// Remembers a mapped X11 menu or tooltip, drawn above every window.
    pub(crate) fn show_override_redirect(&mut self, window: X11Surface) {
        if !self
            .override_redirect
            .iter()
            .any(|shown| shown.window_id() == window.window_id())
        {
            self.override_redirect.push(window);
        }
    }

    /// The id of the managed window behind `surface`.
    pub(crate) fn id_of(&self, surface: &WlSurface) -> Option<WinId> {
        self.managed.iter().find_map(|(id, entry)| {
            (entry.shell.wl_surface().as_ref() == Some(surface)).then_some(*id)
        })
    }

    /// The id of the managed X11 window numbered `window` on the X server.
    pub(crate) fn id_of_x11(&self, window: u32) -> Option<WinId> {
        self.managed
            .iter()
            .find_map(|(id, entry)| match &entry.shell {
                Shell::X11(managed) => (managed.window_id() == window).then_some(*id),
                Shell::Xdg(_) => None,
            })
    }

    /// Forgets an X11 window that was unmapped or destroyed, whatever it was. Returns its
    /// id if the core knew it.
    pub(crate) fn remove_x11(&mut self, window: &X11Surface) -> Option<WinId> {
        let same = |other: &X11Surface| other.window_id() == window.window_id();
        self.pending_x11.retain(|pending| !same(pending));
        self.override_redirect.retain(|shown| !same(shown));
        self.early_x11_fullscreen
            .retain(|asked| *asked != window.window_id());
        let id = self.id_of_x11(window.window_id())?;
        self.release(id);
        Some(id)
    }

    /// The managed window with this id.
    pub(crate) fn get(&self, id: WinId) -> Option<&Entry> {
        self.managed.get(&id)
    }

    /// The managed window with this id, for changing.
    pub(crate) fn get_mut(&mut self, id: WinId) -> Option<&mut Entry> {
        self.managed.get_mut(&id)
    }

    /// Every window surface, announced or not, X11 menus included. An X11 window whose
    /// surface is not committed yet has none.
    pub(crate) fn toplevel_surfaces(&self) -> Vec<WlSurface> {
        let xdg = self
            .pending
            .iter()
            .map(|toplevel| toplevel.wl_surface().clone());
        let managed = self
            .managed
            .values()
            .filter_map(|entry| entry.shell.wl_surface());
        let menus = self
            .override_redirect
            .iter()
            .filter_map(X11Surface::wl_surface);
        xdg.chain(managed).chain(menus).collect()
    }

    /// All managed windows.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (WinId, &Entry)> {
        self.managed.iter().map(|(id, entry)| (*id, entry))
    }

    /// Forgets a toplevel that was destroyed. Returns its id if the core knew it.
    pub(crate) fn remove(&mut self, toplevel: &ToplevelSurface) -> Option<WinId> {
        self.pending.retain(|pending| pending != toplevel);
        let id = self.id_of(toplevel.wl_surface())?;
        self.release(id);
        Some(id)
    }

    /// Stops managing a window without destroying it.
    pub(crate) fn release(&mut self, id: WinId) -> Option<Entry> {
        self.order.retain(|other| *other != id);
        self.managed.remove(&id)
    }

    /// Replaces the stacking order. Ids that are not managed are ignored, and managed
    /// windows missing from `order` keep their relative place below it.
    pub(crate) fn restack(&mut self, order: &[WinId]) {
        let mut next: Vec<WinId> = self
            .order
            .iter()
            .copied()
            .filter(|id| !order.contains(id))
            .collect();
        next.extend(
            order
                .iter()
                .copied()
                .filter(|id| self.managed.contains_key(id)),
        );
        self.order = next;
    }

    /// The managed X11 windows, bottom to top as they are drawn: a fullscreen window is
    /// above the others, whatever its place in the order.
    pub(crate) fn x11_in_order(&self) -> Vec<X11Surface> {
        let x11 = |fullscreen: bool| {
            self.order
                .iter()
                .filter_map(move |id| {
                    let entry = self.managed.get(id)?;
                    (entry.fullscreen == fullscreen).then_some(&entry.shell)
                })
                .filter_map(|shell| match shell {
                    Shell::X11(window) => Some(window.clone()),
                    Shell::Xdg(_) => None,
                })
        };
        x11(false).chain(x11(true)).collect()
    }

    /// The mapped X11 menus and tooltips, newest first. They are drawn above everything.
    pub(crate) fn menu_layers(&self) -> Vec<Layer> {
        self.override_redirect
            .iter()
            .rev()
            .filter_map(|window| {
                Some(Layer {
                    id: None,
                    surface: window.wl_surface()?,
                    origin: window.geometry().loc,
                    popup: true,
                    look: None,
                })
            })
            .collect()
    }

    /// The placed windows that are fullscreen (`fullscreen`) or are not, from the top of
    /// the stack down: for each, its popups (the newest first) and then the window itself.
    /// A hidden window has no rectangle and has no layers.
    pub(crate) fn window_layers(&self, fullscreen: bool) -> Vec<Layer> {
        let now = Instant::now();
        self.order
            .iter()
            .rev()
            .filter_map(|id| Some((*id, self.managed.get(id)?)))
            .filter(|(_, entry)| entry.fullscreen == fullscreen)
            .filter_map(|(id, entry)| Some((id, entry, entry.rect?)))
            .flat_map(|(id, entry, rect)| window_layers(id, entry, rect, now))
            .collect()
    }
}

/// The layers of one placed window, front to back.
fn window_layers(id: WinId, entry: &Entry, target: Rect, now: Instant) -> Vec<Layer> {
    let Some(root) = entry.shell.wl_surface() else {
        return Vec::new();
    };
    // While the window moves it is drawn between its old and new rectangles; its client has
    // already been told the new size and is drawn scaled to fit.
    let rect = entry.drawn_rect(now).unwrap_or(target);
    let origin = Point::from((rect.x(), rect.y()));
    // The rectangle is for the window proper; a client that draws a shadow or a title bar
    // around it declares where the window starts inside its surface. An X11 window is
    // sized to the rectangle and draws nothing around it.
    let surface_origin = match &entry.shell {
        Shell::Xdg(toplevel) => origin - geometry_origin(toplevel),
        Shell::X11(_) => origin,
    };
    let mut layers: Vec<Layer> = PopupManager::popups_for_surface(&root)
        .map(|(popup, location)| Layer {
            id: Some(id),
            surface: popup.wl_surface().clone(),
            origin: origin + location - popup.geometry().loc,
            popup: true,
            look: None,
        })
        .collect();
    layers.reverse();
    layers.push(Layer {
        id: Some(id),
        surface: root,
        origin: surface_origin,
        popup: false,
        look: Some(Look {
            style: entry.style,
            target: placed_area(target),
            drawn: placed_area(rect),
        }),
    });
    layers
}

/// The area the core gave a window, which is all its client may paint on. A size beyond
/// `i32` is clamped, far past any real screen.
fn placed_area(rect: Rect) -> Rectangle<i32, Logical> {
    let length = |pixels: u32| i32::try_from(pixels).unwrap_or(i32::MAX);
    Rectangle::new(
        (rect.x(), rect.y()).into(),
        (length(rect.width()), length(rect.height())).into(),
    )
}

/// Where the window geometry the client declared starts inside its surface; the surface's
/// own corner when it declared none.
fn geometry_origin(toplevel: &ToplevelSurface) -> Point<i32, Logical> {
    with_states(toplevel.wl_surface(), |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
    })
    .map(|geometry| geometry.loc)
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_may_paint_only_on_the_area_it_was_given() {
        let rect = Rect::new(-5, 6, 100, 50).expect("a non-empty rectangle");
        assert_eq!(
            placed_area(rect),
            Rectangle::new((-5, 6).into(), (100, 50).into())
        );
    }

    #[test]
    fn an_enormous_area_is_clamped_not_wrapped() {
        let rect = Rect::new(0, 0, u32::MAX, 1).expect("a non-empty rectangle");
        assert_eq!(placed_area(rect).size, (i32::MAX, 1).into());
    }
}
