//! Popups: where clients are told to put them, that the pointer reaches them, and that
//! a click elsewhere dismisses the ones that grabbed it.

use crate::support::{TestClient, backend, drive, engine};
use lutece::Event;
use lutece_wayland::{ButtonAction, WaylandBackend};

const BTN_LEFT: u32 = 0x110;
const WINDOW: (i32, i32) = (400, 300);
const POPUP_AT: (i32, i32) = (10, 20);
const POPUP_SIZE: (i32, i32) = (80, 40);

/// A shown window with a popup opened on it.
fn window_with_popup(grab: bool) -> (WaylandBackend, TestClient, usize) {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("parent", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, WINDOW.0, WINDOW.1);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    let popup = client.open_popup(window, POPUP_AT, POPUP_SIZE, grab);
    client.settle(&mut backend);
    client.map_popup(popup, POPUP_SIZE.0, POPUP_SIZE.1);
    client.settle(&mut backend);
    (backend, client, popup)
}

#[test]
fn a_popup_is_configured_where_its_client_asked() {
    let (_backend, client, popup) = window_with_popup(false);

    assert_eq!(
        client.popup(popup).configures.last().copied(),
        Some((POPUP_AT.0, POPUP_AT.1, POPUP_SIZE.0, POPUP_SIZE.1))
    );
}

#[test]
fn the_pointer_reaches_a_popup_above_its_window() {
    let (mut backend, mut client, _popup) = window_with_popup(false);

    // Inside the popup, which is drawn on top of the window.
    backend.pointer_motion(f64::from(POPUP_AT.0 + 5), f64::from(POPUP_AT.1 + 5), 1);
    client.settle(&mut backend);

    assert!(client.pointer_entered_popup());
}

#[test]
fn a_click_outside_a_grabbing_popup_dismisses_it() {
    let (mut backend, mut client, popup) = window_with_popup(true);
    // Over the window, away from the popup.
    backend.pointer_motion(300.0, 250.0, 1);
    client.settle(&mut backend);
    assert!(!client.popup(popup).done);

    backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    let events = client.settle(&mut backend);

    assert!(client.popup(popup).done);
    assert!(!events.iter().any(|e| matches!(e, Event::WindowClosed(_))));
}

#[test]
fn a_click_inside_a_grabbing_popup_keeps_it() {
    let (mut backend, mut client, popup) = window_with_popup(true);
    backend.pointer_motion(f64::from(POPUP_AT.0 + 5), f64::from(POPUP_AT.1 + 5), 1);
    client.settle(&mut backend);

    backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    client.settle(&mut backend);

    assert!(!client.popup(popup).done);
}

#[test]
fn a_popup_near_the_edge_slides_back_onto_the_output() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("parent", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, WINDOW.0, WINDOW.1);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    // The output is 1000 wide and the window fills it; this popup would end at 1030.
    let popup = client.open_popup(window, (950, 20), POPUP_SIZE, false);
    client.settle(&mut backend);

    assert_eq!(
        client.popup(popup).configures.last().copied(),
        Some((1000 - POPUP_SIZE.0, 20, POPUP_SIZE.0, POPUP_SIZE.1))
    );
}

/// Two tiled windows side by side, the one on the right mapped. Returns the backend, the
/// engine, the client, the window on the right, the window on the left and the width of
/// the one on the right.
fn side_by_side() -> (
    WaylandBackend,
    lutece::Engine,
    TestClient,
    usize,
    usize,
    i32,
) {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let first = client.open_window("one", "app");
    let second = client.open_window("two", "app");
    events.extend(client.settle(&mut backend));
    let opened = drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    let scene = engine.scene().expect("a valid scene");
    let rect = |id| scene.get(id).expect("the window is on screen").rect;
    // A new window becomes the master, so the older one is in the stack on the right.
    let (right, left) = if rect(opened[0]).x() > rect(opened[1]).x() {
        (first, second)
    } else {
        (second, first)
    };
    let place = rect(opened[usize::from(right == second)]);
    let width = i32::try_from(place.width()).expect("fits");
    assert!(place.x() > 0, "one window sits beside the other");
    client.map_window(right, width, 300);
    client.settle(&mut backend);
    (backend, engine, client, right, left, width)
}

/// Opens and maps a popup asked for at `(x, 20)` in `window`.
fn open_popup_at(
    backend: &mut WaylandBackend,
    client: &mut TestClient,
    window: usize,
    x: i32,
) -> usize {
    let popup = client.open_popup(window, (x, 20), POPUP_SIZE, false);
    client.settle(backend);
    client.map_popup(popup, POPUP_SIZE.0, POPUP_SIZE.1);
    client.settle(backend);
    popup
}

/// Closes `window` and lets the engine re-tile what is left.
fn close_window(
    backend: &mut WaylandBackend,
    engine: &mut lutece::Engine,
    client: &mut TestClient,
    window: usize,
) {
    client.destroy_window(window);
    let events = client.settle(backend);
    drive(engine, backend, events);
    client.settle(backend);
}

#[test]
fn a_popup_put_in_the_wrong_place_by_a_window_move_is_dismissed() {
    let (mut backend, mut engine, mut client, right, left, width) = side_by_side();
    // Asked for 20 pixels before the window's right edge, which is the output's right
    // edge too, so it slides back by 60.
    let ask = width - 20;
    let popup = open_popup_at(&mut backend, &mut client, right, ask);
    assert_eq!(
        client.popup(popup).configures.last().map(|c| c.0),
        Some(ask - 60)
    );
    assert!(!client.popup(popup).done);

    // This window now fills the output, and the slide no longer fits.
    close_window(&mut backend, &mut engine, &mut client, left);

    assert!(client.popup(popup).done);
}

#[test]
fn a_popup_that_still_fits_after_its_window_moves_stays() {
    let (mut backend, mut engine, mut client, right, left, _) = side_by_side();
    let popup = open_popup_at(&mut backend, &mut client, right, 10);

    close_window(&mut backend, &mut engine, &mut client, left);

    assert!(!client.popup(popup).done);
}
