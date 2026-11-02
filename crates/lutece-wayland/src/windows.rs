//! The toplevel windows clients have created, and the ids the core knows them by.
//!
//! A toplevel starts out *pending*: its client has created it but not yet committed, so
//! its title and app id are not final. The first commit announces it to the core and
//! makes it *managed*.

use lutece::{Rect, WinId};
use smithay::desktop::PopupManager;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{SurfaceCachedState, ToplevelSurface};
use std::collections::HashMap;

/// A managed toplevel and where the core last put it.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) toplevel: ToplevelSurface,
    /// `None` while the window is hidden.
    pub(crate) rect: Option<Rect>,
}

/// One surface tree that is on screen: a window, or a popup that belongs to one.
pub(crate) struct Layer {
    /// The window it belongs to.
    pub(crate) id: WinId,
    pub(crate) surface: WlSurface,
    /// Where the surface's own origin is on the output.
    pub(crate) origin: Point<i32, Logical>,
    pub(crate) popup: bool,
}

/// Every toplevel this backend knows about.
///
/// Invariant: `order` lists exactly the keys of `managed`, each once.
#[derive(Debug, Default)]
pub(crate) struct Windows {
    next_id: u64,
    pending: Vec<ToplevelSurface>,
    managed: HashMap<WinId, Entry>,
    /// Stacking order, bottom to top.
    order: Vec<WinId>,
}

impl Windows {
    /// Remembers a toplevel until its first commit.
    pub(crate) fn track(&mut self, toplevel: ToplevelSurface) {
        self.pending.push(toplevel);
    }

    /// Moves the pending toplevel behind `surface`, if there is one, to the managed set.
    pub(crate) fn announce(&mut self, surface: &WlSurface) -> Option<(WinId, &ToplevelSurface)> {
        let at = self
            .pending
            .iter()
            .position(|toplevel| toplevel.wl_surface() == surface)?;
        let toplevel = self.pending.swap_remove(at);
        let id = WinId::new(self.next_id);
        self.next_id += 1;
        self.order.push(id);
        let entry = self.managed.entry(id).or_insert(Entry {
            toplevel,
            rect: None,
        });
        Some((id, &entry.toplevel))
    }

    /// The id of the managed toplevel behind `surface`.
    pub(crate) fn id_of(&self, surface: &WlSurface) -> Option<WinId> {
        self.managed
            .iter()
            .find_map(|(id, entry)| (entry.toplevel.wl_surface() == surface).then_some(*id))
    }

    /// The managed window with this id.
    pub(crate) fn get(&self, id: WinId) -> Option<&Entry> {
        self.managed.get(&id)
    }

    /// The managed window with this id, for changing.
    pub(crate) fn get_mut(&mut self, id: WinId) -> Option<&mut Entry> {
        self.managed.get_mut(&id)
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

    /// Everything on screen from front to back: for each placed window, from the top of
    /// the stack down, its popups (the newest first) and then the window itself. A
    /// hidden window has no rectangle and has no layers.
    pub(crate) fn layers(&self) -> Vec<Layer> {
        self.order
            .iter()
            .rev()
            .filter_map(|id| Some((*id, self.managed.get(id)?)))
            .filter_map(|(id, entry)| Some((id, entry, entry.rect?)))
            .flat_map(|(id, entry, rect)| {
                let origin = Point::from((rect.x(), rect.y()));
                // The rectangle is for the window proper; a client that draws a shadow or
                // a title bar around it declares where the window starts inside its surface.
                let surface_origin = origin - geometry_origin(&entry.toplevel);
                let root = entry.toplevel.wl_surface();
                let mut layers: Vec<Layer> = PopupManager::popups_for_surface(root)
                    .map(|(popup, location)| Layer {
                        id,
                        surface: popup.wl_surface().clone(),
                        origin: origin + location - popup.geometry().loc,
                        popup: true,
                    })
                    .collect();
                layers.reverse();
                layers.push(Layer {
                    id,
                    surface: root.clone(),
                    origin: surface_origin,
                    popup: false,
                });
                layers
            })
            .collect()
    }
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
