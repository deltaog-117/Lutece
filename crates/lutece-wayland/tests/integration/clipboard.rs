//! Copy and paste between two clients.

use crate::support::{TestClient, backend, drive, engine, settle_all};
use lutece::{Engine, Event, WinId};
use lutece_wayland::WaylandBackend;
use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

/// How long a paste may take before the test calls it hung.
const PASTE_DEADLINE: Duration = Duration::from_secs(2);

struct Pair {
    backend: WaylandBackend,
    engine: Engine,
    first: TestClient,
    second: TestClient,
    first_id: WinId,
    second_id: WinId,
}

/// Two clients with a shown window each. The second window was opened last, so it has
/// the focus.
fn two_clients() -> Pair {
    let mut backend = backend();
    let mut engine = engine();
    let (mut first, mut events) = TestClient::connect(&mut backend);
    let (mut second, more) = TestClient::connect(&mut backend);
    events.extend(more);

    let a = first.open_window("first", "app");
    events.extend(settle_all(&mut backend, &mut [&mut first, &mut second]));
    first.map_window(a, 100, 100);
    events.extend(settle_all(&mut backend, &mut [&mut first, &mut second]));
    let b = second.open_window("second", "app");
    events.extend(settle_all(&mut backend, &mut [&mut first, &mut second]));
    second.map_window(b, 100, 100);
    events.extend(settle_all(&mut backend, &mut [&mut first, &mut second]));

    let opened = drive(&mut engine, &mut backend, events);
    settle_all(&mut backend, &mut [&mut first, &mut second]);
    Pair {
        backend,
        engine,
        first,
        second,
        first_id: opened[0],
        second_id: opened[1],
    }
}

impl Pair {
    fn settle(&mut self) {
        settle_all(&mut self.backend, &mut [&mut self.first, &mut self.second]);
    }

    /// Gives `id` the keyboard focus, as the pointer entering it would.
    fn focus(&mut self, id: WinId) {
        self.engine.handle(Event::FocusRequested(id));
        self.backend
            .apply_scene(&self.engine.scene().expect("a valid scene"))
            .expect("scene applies");
        self.settle();
    }
}

fn read_all(mut reader: std::io::PipeReader) -> String {
    let (sender, receiver) = mpsc::channel();
    // The read ends only when every writer is closed; a thread lets a hang fail the test.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    receiver
        .recv_timeout(PASTE_DEADLINE)
        .expect("the sender closed its end")
}

#[test]
fn what_one_client_copies_another_can_paste() {
    let mut pair = two_clients();
    let (first, second) = (pair.first_id, pair.second_id);
    pair.focus(first);

    pair.first.copy(b"hello from the first client");
    pair.settle();
    pair.focus(second);

    assert_eq!(
        pair.second.offered().map(<[String]>::to_vec),
        Some(vec!["text/plain".to_owned()])
    );
    let reader = pair.second.paste();
    pair.settle();
    assert_eq!(read_all(reader), "hello from the first client");
}

#[test]
fn a_client_without_focus_is_not_offered_the_selection() {
    let mut pair = two_clients();
    let first = pair.first_id;
    pair.focus(first);

    pair.first.copy(b"private");
    pair.settle();

    assert!(pair.second.offered().is_none());
}
