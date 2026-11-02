//! The host scale: what clients are told, and that the logical area of an output stays put.

use crate::support::{TestClient, backend};
use lutece_wayland::Scale;

fn scale(factor: f64) -> Scale {
    Scale::from_factor(factor).expect("a usable scale")
}

#[test]
fn a_client_is_told_the_preferred_scale_when_it_asks() {
    let mut backend = backend();
    backend.set_scale(scale(1.5));
    let (mut client, _) = TestClient::connect(&mut backend);
    let window = client.open_window("scaled", "app");
    client.watch_fractional_scale(window);
    client.settle(&mut backend);

    assert_eq!(client.window(window).preferred_scale, Some(180));
}

#[test]
fn without_a_set_scale_a_client_is_told_one() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let window = client.open_window("plain", "app");
    client.watch_fractional_scale(window);
    client.settle(&mut backend);

    assert_eq!(client.window(window).preferred_scale, Some(120));
}

#[test]
fn a_change_of_scale_reaches_windows_that_are_already_open() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let window = client.open_window("scaled", "app");
    client.watch_fractional_scale(window);
    client.settle(&mut backend);

    backend.set_scale(scale(1.25));
    client.settle(&mut backend);

    assert_eq!(client.window(window).preferred_scale, Some(150));
}

#[test]
fn the_output_announces_the_scale_rounded_up_and_the_physical_mode() {
    let mut backend = backend();
    backend.set_scale(scale(1.5));
    let (client, _) = TestClient::connect(&mut backend);

    // The output is 1000x600 logical pixels, which is 1500x900 on the screen.
    assert_eq!(client.output_scale(), Some(2));
    assert_eq!(client.output_mode(), Some((1500, 900)));
}

#[test]
fn a_change_of_scale_updates_a_bound_output() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    assert_eq!(client.output_mode(), Some((1000, 600)));

    backend.set_scale(scale(2.0));
    client.settle(&mut backend);

    assert_eq!(client.output_scale(), Some(2));
    assert_eq!(client.output_mode(), Some((2000, 1200)));
}

#[test]
fn a_client_may_size_its_window_with_a_viewport() {
    let mut backend = backend();
    backend.set_scale(scale(1.5));
    let (mut client, _) = TestClient::connect(&mut backend);
    assert!(client.has_viewporter());
    let window = client.open_window("scaled", "app");
    client.settle(&mut backend);

    // A buffer of 600x450 pixels that is 400x300 logical pixels at scale 1.5. A protocol
    // error would end the connection, which `settle` does not survive.
    client.map_window(window, 600, 450);
    client.set_viewport_destination(window, 400, 300);
    client.settle(&mut backend);
}
