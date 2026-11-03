//! Fullscreen windows: asked for by their clients, decided by the core.

use crate::support::{TestClient, backend, drive, engine};
use lutece::{Event, Rect};
use lutece_wayland::WaylandBackend;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, KeyboardInteractivity,
};

fn along_the_top() -> Anchor {
    Anchor::Top | Anchor::Left | Anchor::Right
}

struct Setup {
    backend: WaylandBackend,
    engine: lutece::Engine,
    client: TestClient,
    window: usize,
}

/// A 30-pixel bar along the top and one mapped window below it.
fn bar_and_window() -> Setup {
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
    let window = client.open_window("w", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, 1000, 570);
    client.map_layer(bar, 1000, 30);
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    Setup {
        backend,
        engine,
        client,
        window,
    }
}

fn size_of(client: &TestClient, window: usize) -> Option<(i32, i32)> {
    client.window(window).configures.last().copied()
}

#[test]
fn a_fullscreen_request_covers_the_bar_and_leaving_it_puts_the_window_back() {
    let mut s = bar_and_window();
    assert_eq!(size_of(&s.client, s.window), Some((1000, 570)));
    assert!(!s.client.window(s.window).fullscreen);

    s.client.request_fullscreen(s.window, true);
    let events = s.client.settle(&mut s.backend);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FullscreenRequested { on: true, .. }))
    );
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    assert!(s.client.window(s.window).fullscreen);
    assert_eq!(size_of(&s.client, s.window), Some((1000, 600)));

    s.client.request_fullscreen(s.window, false);
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    assert!(!s.client.window(s.window).fullscreen);
    assert_eq!(size_of(&s.client, s.window), Some((1000, 570)));
}

#[test]
fn a_window_that_asks_before_it_opens_is_fullscreen_from_the_start() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window_fullscreen("video", "player");
    events.extend(client.settle(&mut backend));

    let opened = events
        .iter()
        .position(|e| matches!(e, Event::WindowOpened { .. }))
        .expect("the window opened");
    assert!(matches!(
        events[opened + 1],
        Event::FullscreenRequested { on: true, .. }
    ));

    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);
    assert!(client.window(window).fullscreen);
    assert_eq!(size_of(&client, window), Some((1000, 600)));
}

#[test]
fn a_fullscreen_window_is_in_front_of_the_bar_for_the_pointer() {
    let mut s = bar_and_window();
    s.backend.pointer_motion(500.0, 10.0, 1);
    s.client.settle(&mut s.backend);
    assert!(
        s.client.pointer_on_bar(0),
        "the bar is on top of the output's edge"
    );

    s.client.request_fullscreen(s.window, true);
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    // The window now covers the screen, and the buffer it has is as large as its place.
    s.client.map_window(s.window, 1000, 600);
    s.client.settle(&mut s.backend);

    s.backend.pointer_motion(500.0, 11.0, 2);
    s.client.settle(&mut s.backend);
    assert!(s.client.pointer_on_window(s.window));
    assert!(!s.client.pointer_on_bar(0));
}

#[test]
fn a_new_window_ends_the_fullscreen_of_the_one_before() {
    let mut s = bar_and_window();
    s.client.request_fullscreen(s.window, true);
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    assert!(s.client.window(s.window).fullscreen);

    // A second window takes the focus, which ends the first one's fullscreen.
    let other = s.client.open_window("other", "app");
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);

    assert!(!s.client.window(s.window).fullscreen);
    assert!(!s.client.window(other).fullscreen);
    // Both share the area below the bar; neither covers it.
    let scene = s.engine.scene().expect("a valid scene");
    let below_the_bar = Rect::new(0, 30, 1000, 570).expect("non-zero size");
    let covered: u64 = scene
        .placements()
        .iter()
        .map(|p| u64::from(p.rect.width()) * u64::from(p.rect.height()))
        .sum();
    assert_eq!(
        covered,
        u64::from(below_the_bar.width()) * u64::from(below_the_bar.height())
    );
}
