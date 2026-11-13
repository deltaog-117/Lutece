//! The keyboard's repeat rate and delay: what clients are told, now and after a change.

use crate::support::{TestClient, backend};
use lutece_wayland::InputSettings;

fn settings(delay_ms: u32, rate: u32) -> InputSettings {
    InputSettings::try_new(delay_ms, rate, None, None, None).expect("values in range")
}

#[test]
fn a_client_is_told_the_usual_repeat_when_nothing_was_set() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    client.settle(&mut backend);

    assert_eq!(client.repeat_info(), Some((25, 600)));
}

#[test]
fn a_client_that_is_already_connected_hears_of_a_change() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    client.settle(&mut backend);

    backend.set_repeat(&settings(300, 40));
    client.settle(&mut backend);

    assert_eq!(client.repeat_info(), Some((40, 300)));
}

#[test]
fn a_client_that_connects_later_is_told_the_new_repeat() {
    let mut backend = backend();
    backend.set_repeat(&settings(250, 0));
    let (mut client, _) = TestClient::connect(&mut backend);
    client.settle(&mut backend);

    // A rate of 0 is how the protocol says a held key does not repeat.
    assert_eq!(client.repeat_info(), Some((0, 250)));
}
