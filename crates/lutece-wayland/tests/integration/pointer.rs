//! The pointer: focus following it, what clients receive, and drags of floating windows.

use crate::support::{TestClient, backend, drive, engine};
use lutece::{Action, Engine, Event, KeyPress, Keymap, Keysym, Modifiers, Rect, WinId};
use lutece_wayland::{ButtonAction, KeyAction, WaylandBackend};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
/// Evdev code plus 8, as Wayland keyboards number keys.
const KEY_LEFTALT: u32 = 56 + 8;
/// The size of the buffer the test window shows, smaller than its place on screen.
const SURFACE: (i32, i32) = (400, 300);

struct Setup {
    backend: WaylandBackend,
    engine: Engine,
    client: TestClient,
    id: WinId,
    rect: Rect,
}

/// One window, shown with a buffer, placed by the engine.
fn one_window() -> Setup {
    one_window_with(engine())
}

fn one_window_with(mut engine: Engine) -> Setup {
    let mut backend = backend();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let index = client.open_window("pointed", "app");
    events.extend(client.settle(&mut backend));
    // A buffer may only be attached once the first configure has been acknowledged.
    client.map_window(index, SURFACE.0, SURFACE.1);
    events.extend(client.settle(&mut backend));
    let opened = drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    let id = opened[0];
    let rect = engine
        .scene()
        .expect("a valid scene")
        .get(id)
        .expect("the window is shown")
        .rect;
    Setup {
        backend,
        engine,
        client,
        id,
        rect,
    }
}

fn at(rect: Rect, dx: i32, dy: i32) -> (f64, f64) {
    (f64::from(rect.x() + dx), f64::from(rect.y() + dy))
}

fn hold_alt(backend: &mut WaylandBackend) {
    backend.key_event(KEY_LEFTALT, KeyAction::Press, 0);
}

fn release_alt(backend: &mut WaylandBackend) {
    backend.key_event(KEY_LEFTALT, KeyAction::Release, 0);
}

#[test]
fn entering_a_window_asks_for_focus_and_the_client_sees_where() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);

    s.backend.pointer_motion(x, y, 1);
    let events = s.client.settle(&mut s.backend);

    assert!(events.contains(&Event::PointerEntered(s.id)));
    assert_eq!(s.client.pointer_enters(), 1);
    assert_eq!(s.client.pointer_position(), Some((30.0, 40.0)));
}

#[test]
fn moving_inside_a_window_does_not_ask_again() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    s.client.settle(&mut s.backend);

    s.backend.pointer_motion(x + 10.0, y + 5.0, 2);
    let events = s.client.settle(&mut s.backend);

    assert!(!events.iter().any(|e| matches!(e, Event::PointerEntered(_))));
    assert_eq!(s.client.pointer_enters(), 1);
    assert_eq!(s.client.pointer_position(), Some((40.0, 45.0)));
}

#[test]
fn empty_space_asks_for_nothing() {
    let mut s = one_window();
    // Past the 400x300 buffer, though inside the place the window was given.
    let (x, y) = at(s.rect, 600, 450);

    s.backend.pointer_motion(x, y, 1);
    let events = s.client.settle(&mut s.backend);

    assert!(!events.iter().any(|e| matches!(e, Event::PointerEntered(_))));
    assert_eq!(s.client.pointer_enters(), 0);
}

#[test]
fn buttons_and_the_wheel_reach_the_client() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);

    s.backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Release, 3);
    s.backend.pointer_axis(0.0, 15.0, 4);
    s.client.settle(&mut s.backend);

    assert_eq!(s.client.pointer_buttons(), 2);
    assert!(s.client.pointer_axes() >= 1);
}

#[test]
fn a_move_drag_is_reported_in_pixels_and_hidden_from_the_client() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    s.client.settle(&mut s.backend);

    hold_alt(&mut s.backend);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    s.backend.pointer_motion(x + 25.0, y + 10.0, 3);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Release, 4);
    release_alt(&mut s.backend);
    let events = s.client.settle(&mut s.backend);

    assert!(events.contains(&Event::FloatAdjusted {
        id: s.id,
        dx: 25,
        dy: 10,
        dw: 0,
        dh: 0,
    }));
    assert_eq!(s.client.pointer_buttons(), 0, "the clicks were the core's");
}

#[test]
fn a_resize_drag_grows_the_window() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    s.client.settle(&mut s.backend);

    hold_alt(&mut s.backend);
    s.backend.pointer_button(BTN_RIGHT, ButtonAction::Press, 2);
    s.backend.pointer_motion(x + 30.0, y + 20.0, 3);
    s.backend
        .pointer_button(BTN_RIGHT, ButtonAction::Release, 4);
    let events = s.client.settle(&mut s.backend);

    assert!(events.contains(&Event::FloatAdjusted {
        id: s.id,
        dx: 0,
        dy: 0,
        dw: 30,
        dh: 20,
    }));
}

#[test]
fn sub_pixel_motion_adds_up_instead_of_being_lost() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    s.client.settle(&mut s.backend);

    hold_alt(&mut s.backend);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    for step in 1..=10 {
        s.backend.pointer_motion(x + f64::from(step) * 0.4, y, 3);
    }
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Release, 4);
    let events = s.client.settle(&mut s.backend);

    let moved: i32 = events
        .iter()
        .filter_map(|event| match event {
            Event::FloatAdjusted { dx, .. } => Some(*dx),
            _ => None,
        })
        .sum();
    assert_eq!(moved, 4, "ten steps of 0.4 px make 4 px");
}

#[test]
fn without_the_modifier_a_click_is_just_a_click() {
    let mut s = one_window();
    let (x, y) = at(s.rect, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    s.backend.set_drag_modifier(Modifiers::NONE);

    hold_alt(&mut s.backend);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    let events = s.client.settle(&mut s.backend);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::FloatAdjusted { .. }))
    );
    assert_eq!(s.client.pointer_buttons(), 1);
}

#[test]
fn dragging_a_floating_window_moves_it_on_screen() {
    let toggle = KeyPress {
        keysym: Keysym::from_char('f').expect("f is printable"),
        modifiers: Modifiers::SUPER,
    };
    let mut keymap = Keymap::new();
    keymap
        .bind(toggle, Action::ToggleFloat)
        .expect("a free binding");
    let mut s = one_window_with(engine().with_keymap(keymap));
    // Float the window with a key, the way a user would.
    s.engine.handle(Event::KeyPressed(toggle));
    s.backend
        .apply_scene(&s.engine.scene().expect("a valid scene"))
        .expect("scene applies");
    s.client.settle(&mut s.backend);
    let before = rect_of(&s.engine, s.id);

    let (x, y) = at(before, 30, 40);
    s.backend.pointer_motion(x, y, 1);
    hold_alt(&mut s.backend);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    s.backend.pointer_motion(x + 25.0, y + 10.0, 3);
    s.backend.pointer_button(BTN_LEFT, ButtonAction::Release, 4);
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);

    let after = rect_of(&s.engine, s.id);
    assert_eq!((after.x(), after.y()), (before.x() + 25, before.y() + 10));
}

fn rect_of(engine: &Engine, id: WinId) -> Rect {
    engine
        .scene()
        .expect("a valid scene")
        .get(id)
        .expect("the window is shown")
        .rect
}
