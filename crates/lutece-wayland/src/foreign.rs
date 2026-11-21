//! The window list for bars and taskbars: `zwlr_foreign_toplevel_manager_v1`.
//!
//! A bar binds the manager and is told about every toplevel: its title, its app id and whether
//! it is the active or a fullscreen one. It is told again whenever one of those changes, and
//! when the window closes. Quickshell, waybar and most docks read this protocol. Smithay has a
//! server for the `ext` list, which carries no state and cannot activate or close a window, so
//! this is written here on the generated bindings.
//!
//! What a client is told is kept apart from the protocol: a [`Snapshot`] is what one window
//! looks like, and [`Snapshot::delta`] says which events a change needs, so that part is plain
//! data and is tested on its own.
//!
//! Left out on purpose: `output_enter` and `output_leave` (a bar asks nothing of them),
//! `parent`, and the requests to maximize, minimize or set a rectangle, which a tiling manager
//! has no meaning for. The protocol lets a compositor ignore them.

use crate::compositor::State;
use lutece::{Event, WinId, WindowInfo};
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::{
    self, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::{
    self, ZwlrForeignToplevelManagerV1,
};

/// The version of the protocol offered. Version 2 adds the fullscreen state and its requests;
/// version 3 adds `parent`, which is not sent.
const VERSION: u32 = 3;

/// `zwlr_foreign_toplevel_handle_v1.state.activated`.
const ACTIVATED: u32 = 2;
/// `zwlr_foreign_toplevel_handle_v1.state.fullscreen`, which exists from version 2.
const FULLSCREEN: u32 = 3;

/// What a client of the protocol is told about one toplevel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Snapshot {
    pub(crate) title: String,
    pub(crate) app_id: String,
    pub(crate) activated: bool,
    pub(crate) fullscreen: bool,
}

/// Which parts of a toplevel changed, and so which events a handle must send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Delta {
    pub(crate) title: bool,
    pub(crate) app_id: bool,
    pub(crate) state: bool,
}

impl Delta {
    /// Whether nothing changed, so nothing is sent (not even `done`).
    pub(crate) const fn is_empty(self) -> bool {
        !(self.title || self.app_id || self.state)
    }
}

impl Snapshot {
    /// A window that just opened: its title and app id, not active and not fullscreen. The
    /// core's focus and fullscreen changes come after, and update it.
    pub(crate) fn of(info: &WindowInfo) -> Self {
        Self {
            title: info.title.clone(),
            app_id: info.class.clone(),
            activated: false,
            fullscreen: false,
        }
    }

    /// What differs between `self` and `next`.
    pub(crate) fn delta(&self, next: &Self) -> Delta {
        Delta {
            title: self.title != next.title,
            app_id: self.app_id != next.app_id,
            state: self.activated != next.activated || self.fullscreen != next.fullscreen,
        }
    }

    /// The `state` event's array: each state that holds, as a native-endian `u32`, in the order
    /// of their values. A client of version 1 does not know fullscreen, so it is left out.
    pub(crate) fn state_array(&self, version: u32) -> Vec<u8> {
        let mut states = Vec::new();
        if self.activated {
            states.push(ACTIVATED);
        }
        if self.fullscreen && version >= 2 {
            states.push(FULLSCREEN);
        }
        states
            .iter()
            .flat_map(|state| state.to_ne_bytes())
            .collect()
    }
}

/// One window as the protocol knows it: what it was last told, and the handle each client
/// holds for it.
#[derive(Debug)]
struct Tracked {
    snapshot: Snapshot,
    handles: Vec<ZwlrForeignToplevelHandleV1>,
}

/// The manager global, the clients bound to it, and every window it lists.
///
/// Invariant: `windows` lists each [`WinId`] once, in the order the windows opened.
#[derive(Debug, Default)]
pub(crate) struct ForeignToplevels {
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    windows: Vec<(WinId, Tracked)>,
}

impl ForeignToplevels {
    /// Creates the global on `display`.
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        // Only the global matters: it stays up for as long as the display does.
        let _ = display.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(VERSION, ());
        Self::default()
    }

    /// Lists a window that just opened, to every client that is bound.
    pub(crate) fn opened(&mut self, display: &DisplayHandle, id: WinId, snapshot: Snapshot) {
        let handles = self
            .managers
            .iter()
            .filter_map(|manager| introduce(display, manager, id, &snapshot))
            .collect();
        self.windows.push((id, Tracked { snapshot, handles }));
    }

    /// Tells each client that the window `id` closed, and forgets it.
    pub(crate) fn closed(&mut self, id: WinId) {
        let Some(at) = self.windows.iter().position(|(known, _)| *known == id) else {
            return;
        };
        let (_, tracked) = self.windows.remove(at);
        for handle in tracked.handles {
            handle.closed();
        }
    }

    /// Changes the title and app id of the window `id`.
    pub(crate) fn set_info(&mut self, id: WinId, title: &str, app_id: &str) {
        self.update(id, |snapshot| {
            title.clone_into(&mut snapshot.title);
            app_id.clone_into(&mut snapshot.app_id);
        });
    }

    /// Marks `focused` as the active window and every other as not.
    pub(crate) fn set_focus(&mut self, focused: Option<WinId>) {
        let ids: Vec<WinId> = self.windows.iter().map(|(id, _)| *id).collect();
        for id in ids {
            self.update(id, |snapshot| snapshot.activated = Some(id) == focused);
        }
    }

    /// Marks the window `id` as fullscreen, or not.
    pub(crate) fn set_fullscreen(&mut self, id: WinId, on: bool) {
        self.update(id, |snapshot| snapshot.fullscreen = on);
    }

    /// Applies `change` to the window `id` and sends what it changed.
    fn update(&mut self, id: WinId, change: impl FnOnce(&mut Snapshot)) {
        let Some((_, tracked)) = self.windows.iter_mut().find(|(known, _)| *known == id) else {
            return;
        };
        let before = tracked.snapshot.clone();
        change(&mut tracked.snapshot);
        let delta = before.delta(&tracked.snapshot);
        if delta.is_empty() {
            return;
        }
        for handle in &tracked.handles {
            send_changes(handle, &tracked.snapshot, delta);
        }
    }

    /// Stops sending to the client behind `manager`: its manager is dropped, and so are its
    /// handles, since the protocol allows no event after the manager's `finished`.
    fn stop_client(&mut self, manager: &ZwlrForeignToplevelManagerV1) {
        self.managers.retain(|known| known != manager);
        for (_, tracked) in &mut self.windows {
            tracked
                .handles
                .retain(|handle| !handle.id().same_client_as(&manager.id()));
        }
    }

    /// Drops a handle its client destroyed.
    fn forget_handle(&mut self, handle: &ZwlrForeignToplevelHandleV1) {
        for (_, tracked) in &mut self.windows {
            tracked.handles.retain(|known| known != handle);
        }
    }
}

/// Makes the handle for window `id` on `manager`'s client and tells it everything about the
/// window. `None` when the client is already gone.
fn introduce(
    display: &DisplayHandle,
    manager: &ZwlrForeignToplevelManagerV1,
    id: WinId,
    snapshot: &Snapshot,
) -> Option<ZwlrForeignToplevelHandleV1> {
    let client = manager.client()?;
    let handle = client
        .create_resource::<ZwlrForeignToplevelHandleV1, WinId, State>(
            display,
            manager.version(),
            id,
        )
        .ok()?;
    manager.toplevel(&handle);
    send_changes(
        &handle,
        snapshot,
        Delta {
            title: true,
            app_id: true,
            state: true,
        },
    );
    Some(handle)
}

/// Sends the events for `delta` on `handle`, and the `done` that makes them take effect.
fn send_changes(handle: &ZwlrForeignToplevelHandleV1, snapshot: &Snapshot, delta: Delta) {
    if delta.title {
        handle.title(snapshot.title.clone());
    }
    if delta.app_id {
        handle.app_id(snapshot.app_id.clone());
    }
    if delta.state {
        handle.state(snapshot.state_array(handle.version()));
    }
    handle.done();
}

impl GlobalDispatch<ZwlrForeignToplevelManagerV1, (), State> for State {
    fn bind(
        state: &mut State,
        display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        (): &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, ());
        // A client that binds late is told about every window already open.
        for (id, tracked) in &mut state.foreign.windows {
            if let Some(handle) = introduce(display, &manager, *id, &tracked.snapshot) {
                tracked.handles.push(handle);
            }
        }
        state.foreign.managers.push(manager);
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, (), State> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        (): &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // `stop` is the only request: the client wants no more events, and is told it is over.
        if matches!(request, zwlr_foreign_toplevel_manager_v1::Request::Stop) {
            state.foreign.stop_client(manager);
            manager.finished();
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        manager: &ZwlrForeignToplevelManagerV1,
        (): &(),
    ) {
        state.foreign.managers.retain(|known| known != manager);
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, WinId, State> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        _handle: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        id: &WinId,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Request;
        match request {
            // A bar asking for a window is asking for the focus, as a client of the window
            // manager would; the core decides.
            Request::Activate { .. } => state.events.push(Event::FocusRequested(*id)),
            Request::Close => {
                if let Some(entry) = state.windows.get(*id) {
                    entry.shell.close();
                }
            }
            Request::SetFullscreen { .. } => state
                .events
                .push(Event::FullscreenRequested { id: *id, on: true }),
            Request::UnsetFullscreen => state
                .events
                .push(Event::FullscreenRequested { id: *id, on: false }),
            // Maximize, minimize, a rectangle for an animation, and a destroyed handle (cleaned
            // up in `destroyed`) need nothing here.
            _ => {}
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        handle: &ZwlrForeignToplevelHandleV1,
        _id: &WinId,
    ) {
        state.foreign.forget_handle(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn snapshot() -> impl Strategy<Value = Snapshot> {
        (".{0,8}", ".{0,8}", any::<bool>(), any::<bool>()).prop_map(
            |(title, app_id, activated, fullscreen)| Snapshot {
                title,
                app_id,
                activated,
                fullscreen,
            },
        )
    }

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().expect("four bytes")))
            .collect()
    }

    #[test]
    fn equal_snapshots_have_no_delta_and_each_part_shows_alone() {
        let base = Snapshot::default();
        assert!(base.delta(&base).is_empty());
        let titled = Snapshot {
            title: "a".into(),
            ..Snapshot::default()
        };
        assert_eq!(
            base.delta(&titled),
            Delta {
                title: true,
                ..Delta::default()
            }
        );
        let active = Snapshot {
            activated: true,
            ..Snapshot::default()
        };
        assert!(base.delta(&active).state && !base.delta(&active).title);
    }

    #[test]
    fn the_state_array_holds_the_states_that_hold_in_order() {
        let both = Snapshot {
            activated: true,
            fullscreen: true,
            ..Snapshot::default()
        };
        assert_eq!(words(&both.state_array(3)), [ACTIVATED, FULLSCREEN]);
        assert_eq!(
            words(&Snapshot::default().state_array(3)),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn a_client_of_version_one_is_not_told_of_fullscreen() {
        let both = Snapshot {
            activated: true,
            fullscreen: true,
            ..Snapshot::default()
        };
        assert_eq!(words(&both.state_array(1)), [ACTIVATED]);
    }

    proptest! {
        #[test]
        fn a_delta_is_empty_exactly_when_nothing_a_client_sees_changed(a in snapshot(), b in snapshot()) {
            prop_assert_eq!(a.delta(&b).is_empty(), a == b);
            prop_assert_eq!(a.delta(&b).is_empty(), b.delta(&a).is_empty());
        }

        #[test]
        fn the_state_array_is_whole_words_of_known_states(s in snapshot(), version in 1u32..=3) {
            let bytes = s.state_array(version);
            prop_assert_eq!(bytes.len() % 4, 0);
            prop_assert!(words(&bytes).iter().all(|w| *w == ACTIVATED || *w == FULLSCREEN));
            prop_assert!(words(&bytes).len() <= 2);
        }
    }
}
