//! X events in, lutece [`Event`]s out.

use crate::grabs::{Keyboard, modifiers_of};
use crate::windows::Windows;
use lutece::{Event, KeyPress, WinId, WindowInfo};
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{KeyPressEvent, MapState, NotifyDetail, NotifyMode, Window};

pub(crate) fn win_id(window: Window) -> WinId {
    WinId::new(u64::from(window))
}

/// Turns one X event into the lutece event it means, if any.
///
/// `adopt` runs once for each window that starts being managed: it subscribes to the
/// window's events and describes it. X events that carry no meaning for the core
/// (property changes, errors for windows that are already gone) produce nothing.
pub(crate) fn translate(
    windows: &mut Windows,
    event: &XEvent,
    adopt: impl FnOnce(Window) -> WindowInfo,
) -> Option<Event> {
    match event {
        XEvent::MapRequest(request) => {
            windows.manage(request.window).then(|| Event::WindowOpened {
                id: win_id(request.window),
                info: adopt(request.window),
            })
        }
        XEvent::UnmapNotify(unmap) => {
            if windows.take_expected_unmap(unmap.window) {
                // We hid it ourselves; the core already knows.
                return None;
            }
            windows
                .forget(unmap.window)
                .then(|| Event::WindowClosed(win_id(unmap.window)))
        }
        XEvent::DestroyNotify(destroy) => windows
            .forget(destroy.window)
            .then(|| Event::WindowClosed(win_id(destroy.window))),
        XEvent::EnterNotify(enter) => {
            // Crossing into a child, or a crossing caused by a grab, is not the pointer
            // arriving at the window.
            let arrived =
                enter.mode == NotifyMode::NORMAL && enter.detail != NotifyDetail::INFERIOR;
            (arrived && windows.is_managed(enter.event))
                .then(|| Event::FocusRequested(win_id(enter.event)))
        }
        _ => None,
    }
}

/// Turns a window that was already on screen when the backend started into the event
/// that opens it, if it is a client window we should manage.
///
/// Windows that are not showing (they send a map request when they want to be) and
/// override-redirect ones (menus and tooltips that opt out of management) produce
/// nothing. A window that is adopted is recorded as on screen, so hiding it later sends
/// the unmap it needs.
pub(crate) fn existing(
    windows: &mut Windows,
    window: Window,
    map_state: MapState,
    override_redirect: bool,
    adopt: impl FnOnce(Window) -> WindowInfo,
) -> Option<Event> {
    if map_state != MapState::VIEWABLE || override_redirect || !windows.manage(window) {
        return None;
    }
    windows.mark_mapped(window);
    Some(Event::WindowOpened {
        id: win_id(window),
        info: adopt(window),
    })
}

/// Turns a grabbed key press into the event it means. Keys with no keysym produce
/// nothing.
pub(crate) fn key_press(keyboard: &Keyboard, press: &KeyPressEvent) -> Option<Event> {
    Some(Event::KeyPressed(KeyPress {
        keysym: keyboard.keysym_of(press.detail)?,
        modifiers: modifiers_of(press.state),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lutece::{Keysym, Modifiers};
    use proptest::prelude::*;
    use x11rb::protocol::xproto::{
        DestroyNotifyEvent, EnterNotifyEvent, KeyButMask, MapRequestEvent, UnmapNotifyEvent,
    };

    fn map_request(window: Window) -> XEvent {
        XEvent::MapRequest(MapRequestEvent {
            response_type: 20,
            sequence: 0,
            parent: 1,
            window,
        })
    }

    fn unmap(window: Window) -> XEvent {
        XEvent::UnmapNotify(UnmapNotifyEvent {
            response_type: 18,
            sequence: 0,
            event: 1,
            window,
            from_configure: false,
        })
    }

    fn destroy(window: Window) -> XEvent {
        XEvent::DestroyNotify(DestroyNotifyEvent {
            response_type: 17,
            sequence: 0,
            event: 1,
            window,
        })
    }

    fn enter(window: Window, mode: NotifyMode, detail: NotifyDetail) -> XEvent {
        XEvent::EnterNotify(EnterNotifyEvent {
            response_type: 7,
            detail,
            sequence: 0,
            time: 0,
            root: 1,
            event: window,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state: KeyButMask::default(),
            mode,
            same_screen_focus: 0,
        })
    }

    fn key_event(keycode: u8, state: KeyButMask) -> KeyPressEvent {
        KeyPressEvent {
            response_type: 2,
            detail: keycode,
            sequence: 0,
            time: 0,
            root: 1,
            event: 1,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state,
            same_screen: true,
        }
    }

    fn run(windows: &mut Windows, event: &XEvent) -> Option<Event> {
        translate(windows, event, |_| WindowInfo::default())
    }

    #[test]
    fn a_map_request_opens_the_window_once() {
        let mut windows = Windows::default();
        assert_eq!(
            run(&mut windows, &map_request(5)),
            Some(Event::WindowOpened {
                id: WinId::new(5),
                info: WindowInfo::default()
            })
        );
        assert_eq!(run(&mut windows, &map_request(5)), None);
    }

    #[test]
    fn our_own_hide_is_not_a_close_but_the_clients_unmap_is() {
        let mut windows = Windows::default();
        run(&mut windows, &map_request(5));
        windows.mark_mapped(5);
        windows.mark_hidden(5);

        assert_eq!(run(&mut windows, &unmap(5)), None);
        assert_eq!(
            run(&mut windows, &unmap(5)),
            Some(Event::WindowClosed(WinId::new(5)))
        );
    }

    #[test]
    fn only_showing_client_windows_are_adopted_at_startup() {
        let mut windows = Windows::default();
        let adopt = |windows: &mut Windows, window, state, redirect| {
            existing(windows, window, state, redirect, |_| WindowInfo::default())
        };

        assert!(adopt(&mut windows, 5, MapState::VIEWABLE, false).is_some());
        assert!(adopt(&mut windows, 6, MapState::UNMAPPED, false).is_none());
        assert!(adopt(&mut windows, 7, MapState::UNVIEWABLE, false).is_none());
        assert!(adopt(&mut windows, 8, MapState::VIEWABLE, true).is_none());
        assert!(adopt(&mut windows, 5, MapState::VIEWABLE, false).is_none());
        assert!(windows.is_managed(5));
        assert!(!windows.is_managed(8));
    }

    #[test]
    fn an_adopted_window_is_on_screen_so_hiding_it_unmaps_it() {
        let mut windows = Windows::default();
        existing(&mut windows, 5, MapState::VIEWABLE, false, |_| {
            WindowInfo::default()
        });
        assert!(windows.mark_hidden(5));
    }

    #[test]
    fn the_pointer_focuses_only_managed_windows_it_really_entered() {
        let mut windows = Windows::default();
        run(&mut windows, &map_request(5));

        let normal = enter(5, NotifyMode::NORMAL, NotifyDetail::ANCESTOR);
        assert_eq!(
            run(&mut windows, &normal),
            Some(Event::FocusRequested(WinId::new(5)))
        );
        let inferior = enter(5, NotifyMode::NORMAL, NotifyDetail::INFERIOR);
        assert_eq!(run(&mut windows, &inferior), None);
        let grabbed = enter(5, NotifyMode::GRAB, NotifyDetail::ANCESTOR);
        assert_eq!(run(&mut windows, &grabbed), None);
        let stranger = enter(9, NotifyMode::NORMAL, NotifyDetail::ANCESTOR);
        assert_eq!(run(&mut windows, &stranger), None);
    }

    #[test]
    fn a_grabbed_press_carries_its_plain_keysym_and_modifiers_but_not_lock_keys() {
        // Keycodes 8 and 9 produce 'a' and 'q' (shifted: 'A' and 'Q').
        let keyboard = Keyboard::new(8, 2, vec![0x61, 0x41, 0x71, 0x51]);
        let state = KeyButMask::MOD4 | KeyButMask::SHIFT | KeyButMask::LOCK | KeyButMask::MOD2;
        assert_eq!(
            key_press(&keyboard, &key_event(9, state)),
            Some(Event::KeyPressed(KeyPress {
                keysym: Keysym(0x71),
                modifiers: Modifiers::SUPER | Modifiers::SHIFT,
            }))
        );
        assert_eq!(key_press(&keyboard, &key_event(30, state)), None);
    }

    proptest! {
        /// Whatever clients do, each window is reported opened, then closed, then opened
        /// again, and never closed twice in a row.
        #[test]
        fn opens_and_closes_alternate_for_every_window(
            steps in proptest::collection::vec((0u32..4, 0u8..3), 0..40),
        ) {
            let mut windows = Windows::default();
            let mut open = std::collections::HashSet::new();
            for (window, kind) in steps {
                let event = match kind {
                    0 => map_request(window),
                    1 => unmap(window),
                    _ => destroy(window),
                };
                match run(&mut windows, &event) {
                    Some(Event::WindowOpened { id, .. }) => {
                        prop_assert!(open.insert(id), "{id:?} opened twice");
                    }
                    Some(Event::WindowClosed(id)) => {
                        prop_assert!(open.remove(&id), "{id:?} closed while not open");
                    }
                    _ => {}
                }
            }
        }
    }
}
