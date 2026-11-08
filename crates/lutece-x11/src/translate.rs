//! X events in, lutece [`Event`]s out.

use crate::grabs::{Keyboard, modifiers_of};
use crate::windows::Windows;
use lutece::{Event, KeyPress, WinId, WindowInfo};
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, KeyPressEvent, MapState, NotifyDetail, NotifyMode, Window,
};

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
        XEvent::MapRequest(request) => windows.manage(request.window).then(|| {
            let info = adopt(request.window);
            windows.note_info(request.window, info.clone());
            Event::WindowOpened {
                id: win_id(request.window),
                info,
            }
        }),
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
                .then(|| Event::PointerEntered(win_id(enter.event)))
        }
        _ => None,
    }
}

/// Whether a change of the property `atom` can change a window's title, class or parent
/// window, which is all [`WindowInfo`] holds.
pub(crate) fn changes_description(net_wm_name: Atom, atom: Atom) -> bool {
    [
        net_wm_name,
        AtomEnum::WM_NAME.into(),
        AtomEnum::WM_CLASS.into(),
        AtomEnum::WM_TRANSIENT_FOR.into(),
    ]
    .contains(&atom)
}

/// `_NET_WM_STATE` action: take the listed states away.
const STATE_REMOVE: u32 = 0;
/// `_NET_WM_STATE` action: add them.
const STATE_ADD: u32 = 1;
/// `_NET_WM_STATE` action: switch them.
const STATE_TOGGLE: u32 = 2;

/// Turns a `_NET_WM_STATE` client message that names the fullscreen state into the
/// request it means. `state` and `fullscreen` are the two atoms. Messages about other
/// states, about windows we do not manage, or in the wrong format produce nothing.
///
/// A toggle is worked out from what we last showed, since the message does not say.
pub(crate) fn fullscreen_request(
    windows: &Windows,
    state: Atom,
    fullscreen: Atom,
    message: &ClientMessageEvent,
) -> Option<Event> {
    if message.type_ != state || message.format != 32 || !windows.is_managed(message.window) {
        return None;
    }
    let [action, first, second, ..] = message.data.as_data32();
    if first != fullscreen && second != fullscreen {
        return None;
    }
    let on = match action {
        STATE_REMOVE => false,
        STATE_ADD => true,
        STATE_TOGGLE => !windows.is_fullscreen(message.window),
        _ => return None,
    };
    Some(Event::FullscreenRequested {
        id: win_id(message.window),
        on,
    })
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
    let info = adopt(window);
    windows.note_info(window, info.clone());
    Some(Event::WindowOpened {
        id: win_id(window),
        info,
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

    const STATE: Atom = 300;
    const FULLSCREEN: Atom = 301;

    fn state_message(window: Window, action: u32, atoms: [Atom; 2]) -> ClientMessageEvent {
        ClientMessageEvent::new(32, window, STATE, [action, atoms[0], atoms[1], 1, 0])
    }

    #[test]
    fn a_state_message_adds_removes_and_toggles_fullscreen() {
        let mut windows = Windows::default();
        windows.manage(7);
        let ask = |windows: &Windows, action, atoms| {
            fullscreen_request(windows, STATE, FULLSCREEN, &state_message(7, action, atoms))
        };
        let want = |on| Some(Event::FullscreenRequested { id: win_id(7), on });

        assert_eq!(ask(&windows, STATE_ADD, [FULLSCREEN, 0]), want(true));
        assert_eq!(ask(&windows, STATE_REMOVE, [FULLSCREEN, 0]), want(false));
        // The atom may sit in either slot, as GTK and Qt send it differently.
        assert_eq!(ask(&windows, STATE_ADD, [5, FULLSCREEN]), want(true));

        assert_eq!(ask(&windows, STATE_TOGGLE, [FULLSCREEN, 0]), want(true));
        windows.set_fullscreen(7, true);
        assert_eq!(ask(&windows, STATE_TOGGLE, [FULLSCREEN, 0]), want(false));
    }

    #[test]
    fn other_states_other_windows_and_odd_messages_are_ignored() {
        let mut windows = Windows::default();
        windows.manage(7);
        let ask =
            |message: &ClientMessageEvent| fullscreen_request(&windows, STATE, FULLSCREEN, message);

        assert_eq!(ask(&state_message(7, STATE_ADD, [5, 6])), None);
        assert_eq!(ask(&state_message(8, STATE_ADD, [FULLSCREEN, 0])), None);
        assert_eq!(ask(&state_message(7, 9, [FULLSCREEN, 0])), None);
        let wrong_type = ClientMessageEvent::new(32, 7, 999_u32, [STATE_ADD, FULLSCREEN, 0, 0, 0]);
        assert_eq!(ask(&wrong_type), None);
        let bytes = ClientMessageEvent::new(8, 7, STATE, [0u8; 20]);
        assert_eq!(ask(&bytes), None);
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
    fn a_released_window_is_never_reported_again() {
        let mut windows = Windows::default();
        run(&mut windows, &map_request(5));
        windows.forget(5);

        assert_eq!(run(&mut windows, &unmap(5)), None);
        assert_eq!(run(&mut windows, &destroy(5)), None);
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
    fn only_the_properties_a_description_holds_count_as_changing_it() {
        let net_wm_name: Atom = 400;
        for atom in [
            net_wm_name,
            AtomEnum::WM_NAME.into(),
            AtomEnum::WM_CLASS.into(),
            AtomEnum::WM_TRANSIENT_FOR.into(),
        ] {
            assert!(changes_description(net_wm_name, atom));
        }
        for atom in [
            AtomEnum::WM_HINTS.into(),
            AtomEnum::WM_ICON_NAME.into(),
            401,
        ] {
            assert!(!changes_description(net_wm_name, atom));
        }
    }

    #[test]
    fn the_pointer_is_reported_only_for_managed_windows_it_really_entered() {
        let mut windows = Windows::default();
        run(&mut windows, &map_request(5));

        let normal = enter(5, NotifyMode::NORMAL, NotifyDetail::ANCESTOR);
        assert_eq!(
            run(&mut windows, &normal),
            Some(Event::PointerEntered(WinId::new(5)))
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
