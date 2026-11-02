//! Window decorations and window geometry: clients are told to draw no frame, and one that
//! does keep a margin around its window still lines up with its place.

use crate::support::{TestClient, backend, drive, engine};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

#[test]
fn a_client_asking_for_decoration_is_told_the_compositor_draws_it() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let window = client.open_window("framed", "app");
    client.settle(&mut backend);

    client.request_decoration(window);
    client.settle(&mut backend);

    assert_eq!(
        client.window(window).decoration_mode,
        Some(Mode::ServerSide)
    );
}

#[test]
fn a_client_that_prefers_its_own_frame_is_still_told_server_side() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let window = client.open_window("framed", "app");
    client.settle(&mut backend);
    client.request_decoration(window);
    client.settle(&mut backend);

    client.prefer_client_side_decoration(window);
    client.settle(&mut backend);

    assert_eq!(
        client.window(window).decoration_mode,
        Some(Mode::ServerSide)
    );
}

#[test]
fn a_window_is_placed_by_its_declared_geometry_not_its_surface() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("shadowed", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, 400, 300);
    events.extend(client.settle(&mut backend));
    // A 20 pixel shadow on every side of the window proper.
    client.set_window_geometry(window, (20, 20), (360, 260));
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    // The window fills the output from its corner, so the corner of its geometry is the
    // output's, and the surface starts 20 pixels before it.
    backend.pointer_motion(5.0, 5.0, 1);
    client.settle(&mut backend);

    assert_eq!(client.pointer_position(), Some((25.0, 25.0)));
}
