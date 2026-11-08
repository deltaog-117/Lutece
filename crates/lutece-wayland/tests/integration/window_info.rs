//! A window that changes its title or class after it opened is reported to the core.

use crate::support::{TestClient, backend};
use lutece::{Event, WindowInfo};
use lutece_wayland::WaylandBackend;

fn changes(events: &[Event]) -> Vec<&WindowInfo> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::WindowInfoChanged { info, .. } => Some(info),
            _ => None,
        })
        .collect()
}

/// A client with one announced window, and the events up to then.
fn announced() -> (WaylandBackend, TestClient, usize) {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let index = client.open_window("first", "term");
    let events = client.settle(&mut backend);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::WindowOpened { .. })),
        "the window opens on its first commit"
    );
    (backend, client, index)
}

#[test]
fn a_new_title_is_reported_with_the_class_it_keeps() {
    let (mut backend, mut client, index) = announced();

    client.set_title(index, "second");
    let events = client.settle(&mut backend);

    let info = changes(&events);
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].title, "second");
    assert_eq!(info[0].class, "term");
}

#[test]
fn a_new_class_is_reported() {
    let (mut backend, mut client, index) = announced();

    client.set_app_id(index, "editor");
    let events = client.settle(&mut backend);

    let info = changes(&events);
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].class, "editor");
}

#[test]
fn setting_what_is_already_set_reports_nothing() {
    let (mut backend, mut client, index) = announced();

    client.set_title(index, "first");
    client.set_app_id(index, "term");
    let events = client.settle(&mut backend);

    assert!(changes(&events).is_empty());
}
