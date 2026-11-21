//! The window list for bars (`zwlr_foreign_toplevel_manager_v1`): what a client is told about
//! each toplevel, and what it may ask for.

use crate::support::{TestClient, backend, drive, engine};
use lutece::{Event, WinId};
use lutece_wayland::WaylandBackend;

/// `zwlr_foreign_toplevel_handle_v1.state.activated`.
const ACTIVATED: u32 = 2;
/// `zwlr_foreign_toplevel_handle_v1.state.fullscreen`.
const FULLSCREEN: u32 = 3;

struct Setup {
    backend: WaylandBackend,
    engine: lutece::Engine,
    client: TestClient,
}

fn setup() -> Setup {
    let mut backend = backend();
    let mut engine = engine();
    let (client, events) = TestClient::connect(&mut backend);
    // The backend's first events tell the core about the screen; with none, nothing is placed
    // or focused.
    drive(&mut engine, &mut backend, events);
    Setup {
        backend,
        engine,
        client,
    }
}

impl Setup {
    /// Opens a window, lets the core place and focus it, and returns its index on the client
    /// and the core's id for it.
    fn open(&mut self, title: &str, app_id: &str) -> (usize, WinId) {
        let index = self.client.open_window(title, app_id);
        let events = self.client.settle(&mut self.backend);
        let opened = drive(&mut self.engine, &mut self.backend, events);
        self.client.settle(&mut self.backend);
        (index, *opened.first().expect("the window opened"))
    }
}

#[test]
fn the_compositor_offers_the_window_list() {
    let mut backend = backend();
    let (client, _) = TestClient::connect(&mut backend);
    assert!(client.has_toplevel_list());
}

#[test]
fn a_window_is_listed_with_its_title_and_app_id() {
    let mut s = setup();
    s.open("hello", "term");

    let listed = s.client.listed();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "hello");
    assert_eq!(listed[0].app_id, "term");
    assert!(listed[0].dones >= 1, "the changes were made to take effect");
    assert!(!listed[0].closed);
}

#[test]
fn a_new_title_is_sent_and_the_same_one_is_not() {
    let mut s = setup();
    let (index, _) = s.open("first", "term");
    let before = s.client.listed()[0].dones;

    s.client.set_title(index, "second");
    s.client.settle(&mut s.backend);
    assert_eq!(s.client.listed()[0].title, "second");
    assert_eq!(s.client.listed()[0].app_id, "term", "the app id is kept");
    assert!(s.client.listed()[0].dones > before);

    let after = s.client.listed()[0].dones;
    s.client.set_title(index, "second");
    s.client.settle(&mut s.backend);
    assert_eq!(
        s.client.listed()[0].dones,
        after,
        "nothing changed, nothing sent"
    );
}

#[test]
fn the_focused_window_is_the_active_one() {
    let mut s = setup();
    s.open("a", "app");
    assert_eq!(s.client.listed()[0].states, [ACTIVATED]);

    s.open("b", "app");
    assert_eq!(s.client.listed().len(), 2);
    assert!(
        s.client.listed()[0].states.is_empty(),
        "the first is no longer active"
    );
    assert_eq!(s.client.listed()[1].states, [ACTIVATED]);
}

#[test]
fn a_closed_window_is_reported_closed() {
    let mut s = setup();
    let (index, _) = s.open("a", "app");

    s.client.destroy_window(index);
    let events = s.client.settle(&mut s.backend);
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);

    assert!(s.client.listed()[0].closed);
}

#[test]
fn a_client_that_binds_late_is_told_of_the_windows_already_open() {
    let mut s = setup();
    s.open("early", "term");

    let (mut late, _) = TestClient::connect(&mut s.backend);
    late.settle(&mut s.backend);

    assert_eq!(late.listed().len(), 1);
    assert_eq!(late.listed()[0].title, "early");
    assert_eq!(late.listed()[0].app_id, "term");
    assert_eq!(
        late.listed()[0].states,
        [ACTIVATED],
        "it is told the state too"
    );
}

#[test]
fn asking_to_activate_a_window_asks_the_core_for_its_focus() {
    let mut s = setup();
    let (_, first) = s.open("a", "app");
    s.open("b", "app");

    s.client.activate_listed(0);
    let events = s.client.settle(&mut s.backend);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::FocusRequested(id) if *id == first)),
        "{events:?}"
    );
}

#[test]
fn asking_to_close_a_window_closes_it() {
    let mut s = setup();
    let (index, _) = s.open("a", "app");

    s.client.close_listed(0);
    s.client.settle(&mut s.backend);

    assert!(s.client.window(index).closed);
}

#[test]
fn fullscreen_is_asked_of_the_core_and_listed_once_it_is_granted() {
    let mut s = setup();
    let (_, id) = s.open("a", "app");

    s.client.fullscreen_listed(0, true);
    let events = s.client.settle(&mut s.backend);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::FullscreenRequested { id: asked, on: true } if *asked == id)),
        "{events:?}"
    );
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    assert_eq!(s.client.listed()[0].states, [ACTIVATED, FULLSCREEN]);

    s.client.fullscreen_listed(0, false);
    let events = s.client.settle(&mut s.backend);
    assert!(events.iter().any(
        |event| matches!(event, Event::FullscreenRequested { id: asked, on: false } if *asked == id)
    ));
    drive(&mut s.engine, &mut s.backend, events);
    s.client.settle(&mut s.backend);
    assert_eq!(s.client.listed()[0].states, [ACTIVATED]);
}

#[test]
fn a_client_that_stops_hears_no_more() {
    let mut s = setup();
    let (index, _) = s.open("first", "term");

    s.client.stop_listing();
    s.client.settle(&mut s.backend);
    assert!(s.client.listing_finished());

    s.client.set_title(index, "second");
    s.client.settle(&mut s.backend);
    assert_eq!(s.client.listed()[0].title, "first");
}
