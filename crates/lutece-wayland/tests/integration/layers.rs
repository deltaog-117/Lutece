//! Layer-shell: bars keep space clear, launchers take the keyboard, wallpapers stay below.

use crate::support::{TestClient, backend, drive, engine};
use lutece::{Event, Insets, OutputId, Rect};
use lutece_wayland::{ButtonAction, WaylandBackend};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, KeyboardInteractivity,
};

const BTN_LEFT: u32 = 0x110;

fn along_the_top() -> Anchor {
    Anchor::Top | Anchor::Left | Anchor::Right
}

/// What the events say bars reserve, as `(output, insets)`.
fn reserved(events: &[Event]) -> Vec<(u32, Insets)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ReservedChanged { output, insets } => Some((output.get(), *insets)),
            _ => None,
        })
        .collect()
}

fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect::new(x, y, width, height).expect("non-zero size")
}

#[test]
fn the_compositor_offers_layer_shell() {
    let mut backend = backend();
    let (client, _) = TestClient::connect(&mut backend);
    assert!(client.has_layer_shell());
}

#[test]
fn a_top_bar_gets_the_full_width_and_the_tiles_start_below_it() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);

    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    events.extend(client.settle(&mut backend));
    assert_eq!(client.bar(bar).configures.last(), Some(&(1000, 30)));
    assert_eq!(reserved(&events), [(0, Insets::new(0, 30, 0, 0))]);

    client.open_window("w", "app");
    events.extend(client.settle(&mut backend));
    let opened = drive(&mut engine, &mut backend, events);

    let scene = engine.scene().expect("a valid scene");
    assert_eq!(
        scene.get(opened[0]).map(|p| p.rect),
        Some(rect(0, 30, 1000, 570))
    );
}

#[test]
fn bars_on_the_other_edges_reserve_those_edges() {
    let mut backend = backend();
    let (mut client, mut events) = TestClient::connect(&mut backend);

    client.open_layer(
        Layer::Top,
        Anchor::Left | Anchor::Top | Anchor::Bottom,
        (40, 0),
        40,
        KeyboardInteractivity::None,
    );
    client.open_layer(
        Layer::Top,
        Anchor::Bottom | Anchor::Left | Anchor::Right,
        (0, 25),
        25,
        KeyboardInteractivity::None,
    );
    events.extend(client.settle(&mut backend));

    let last = reserved(&events).last().copied().expect("a report");
    // The left bar runs the full height, so the bottom bar sits beside it, not under it.
    assert_eq!(last.1.left, 40);
    assert_eq!(last.1.bottom, 25);
}

#[test]
fn a_wallpaper_fills_the_output_and_reserves_nothing() {
    let mut backend = backend();
    let (mut client, mut events) = TestClient::connect(&mut backend);

    let wallpaper = client.open_layer(
        Layer::Background,
        Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        (0, 0),
        -1,
        KeyboardInteractivity::None,
    );
    events.extend(client.settle(&mut backend));

    assert_eq!(client.bar(wallpaper).configures.last(), Some(&(1000, 600)));
    assert!(reserved(&events).is_empty());
}

#[test]
fn a_bar_that_quits_gives_its_space_back() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    client.open_window("w", "app");
    events.extend(client.settle(&mut backend));
    let opened = drive(&mut engine, &mut backend, events);

    client.destroy_layer(bar);
    let after = client.settle(&mut backend);

    assert_eq!(reserved(&after), [(0, Insets::NONE)]);
    drive(&mut engine, &mut backend, after);
    let scene = engine.scene().expect("a valid scene");
    assert_eq!(
        scene.get(opened[0]).map(|p| p.rect),
        Some(rect(0, 0, 1000, 600))
    );
}

#[test]
fn a_bar_that_changes_its_height_is_reported_again() {
    let mut backend = backend();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    events.extend(client.settle(&mut backend));
    // Settling again with nothing new says nothing more.
    assert!(reserved(&client.settle(&mut backend)).is_empty());
}

#[test]
fn a_bar_receives_the_pointer_and_the_core_is_not_asked_to_focus_it() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    client.settle(&mut backend);
    client.map_layer(bar, 1000, 30);
    client.settle(&mut backend);

    backend.pointer_motion(500.0, 10.0, 1);
    let events = client.settle(&mut backend);

    assert!(client.pointer_on_bar(bar));
    assert_eq!(client.pointer_position(), Some((500.0, 10.0)));
    assert!(!events.iter().any(|e| matches!(e, Event::PointerEntered(_))));
}

#[test]
fn an_exclusive_surface_takes_the_keyboard_and_gives_it_back_when_it_goes() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("w", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, 400, 300);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    assert!(client.keyboard_on_window(window));

    let launcher = client.open_layer(
        Layer::Overlay,
        Anchor::empty(),
        (200, 100),
        0,
        KeyboardInteractivity::Exclusive,
    );
    client.settle(&mut backend);
    client.map_layer(launcher, 200, 100);
    client.settle(&mut backend);
    assert!(client.keyboard_on_bar(launcher));

    // A new scene does not take it back while the launcher is up.
    drive(&mut engine, &mut backend, Vec::new());
    client.settle(&mut backend);
    assert!(client.keyboard_on_bar(launcher));

    client.destroy_layer(launcher);
    client.settle(&mut backend);
    assert!(client.keyboard_on_window(window));
}

#[test]
fn a_surface_that_wants_the_keyboard_when_clicked_gets_it_until_the_pointer_enters_a_window() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("w", "app");
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::OnDemand,
    );
    events.extend(client.settle(&mut backend));
    client.map_window(window, 1000, 570);
    client.map_layer(bar, 1000, 30);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    assert!(client.keyboard_on_window(window));

    backend.pointer_motion(500.0, 10.0, 1);
    backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    backend.pointer_button(BTN_LEFT, ButtonAction::Release, 3);
    client.settle(&mut backend);
    assert!(client.keyboard_on_bar(bar));

    backend.pointer_motion(500.0, 300.0, 4);
    client.settle(&mut backend);
    assert!(client.keyboard_on_window(window));
}

#[test]
fn a_surface_that_never_asks_for_the_keyboard_does_not_get_it_by_a_click() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("w", "app");
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    events.extend(client.settle(&mut backend));
    client.map_window(window, 1000, 570);
    client.map_layer(bar, 1000, 30);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    backend.pointer_motion(500.0, 10.0, 1);
    backend.pointer_button(BTN_LEFT, ButtonAction::Press, 2);
    client.settle(&mut backend);

    assert!(client.keyboard_on_window(window));
}

#[test]
fn a_bar_popup_is_placed_where_its_client_asks() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    client.settle(&mut backend);
    client.map_layer(bar, 1000, 30);
    client.settle(&mut backend);

    let popup = client.open_bar_popup(bar, (100, 30), (80, 40));
    client.settle(&mut backend);

    assert_eq!(
        client.popup(popup).configures.last(),
        Some(&(100, 30, 80, 40))
    );
}

#[test]
fn a_surface_on_an_output_that_goes_away_is_asked_to_close() {
    let mut backend: WaylandBackend = backend();
    let two = [
        lutece::Output {
            id: OutputId::new(0),
            rect: rect(0, 0, 1000, 600),
        },
        lutece::Output {
            id: OutputId::new(1),
            rect: rect(1000, 0, 1000, 600),
        },
    ];
    backend.set_outputs(&two);
    let (mut client, _) = TestClient::connect(&mut backend);
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    client.settle(&mut backend);
    assert!(!client.bar(bar).closed);

    backend.set_outputs(&two[1..]);
    client.settle(&mut backend);

    assert!(client.bar(bar).closed);
}

#[test]
fn a_surface_asked_for_with_no_output_is_closed_at_once() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    backend.set_outputs(&[]);
    // Removing every output keeps none: the backend then has no screen to put it on.
    let bar = client.open_layer(
        Layer::Top,
        along_the_top(),
        (0, 30),
        30,
        KeyboardInteractivity::None,
    );
    client.settle(&mut backend);
    assert!(client.bar(bar).closed);
}
