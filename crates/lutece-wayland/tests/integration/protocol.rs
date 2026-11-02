//! What clients see when the backend is driven by a real engine.

use crate::support::{TestClient, backend, drive, engine};
use lutece::{Action, Effect, Event, KeyPress, Keymap, Keysym, Modifiers};
use lutece_wayland::{KeyAction, WaylandBackend};

/// Evdev codes plus 8, which is how Wayland keyboards number their keys.
const KEY_LEFTMETA: u32 = 125 + 8;
const KEY_Q: u32 = 16 + 8;
const KEY_W: u32 = 17 + 8;

fn key(backend: &mut WaylandBackend, code: u32, action: KeyAction) -> Option<KeyPress> {
    backend.key_event(code, action, 0)
}

#[test]
fn the_first_events_report_the_output() {
    let mut backend = backend();
    let events = backend
        .next_events(std::time::Duration::ZERO)
        .expect("backend serves");
    let [Event::OutputsChanged(outputs)] = events.as_slice() else {
        panic!("expected one output report, got {events:?}");
    };
    assert_eq!(
        (outputs[0].rect.width(), outputs[0].rect.height()),
        (1000, 600)
    );
}

#[test]
fn a_window_is_reported_on_its_first_commit_with_title_and_class() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    client.open_window("notes", "editor");

    let events = client.settle(&mut backend);

    let opened: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::WindowOpened { info, .. } => Some((info.title.as_str(), info.class.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(opened, [("notes", "editor")]);
}

#[test]
fn the_scene_decides_the_size_each_client_is_told() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let first = client.open_window("one", "app");
    let second = client.open_window("two", "app");
    events.extend(client.settle(&mut backend));

    let opened = drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    assert_eq!(opened.len(), 2);
    let scene = engine.scene().expect("a valid scene");
    let told = |index| client.window(index).configures.last().copied();
    let wanted = |id| {
        let rect = scene.get(id).expect("the window is on screen").rect;
        (
            i32::try_from(rect.width()).expect("fits"),
            i32::try_from(rect.height()).expect("fits"),
        )
    };
    assert_eq!(told(first), Some(wanted(opened[0])));
    assert_eq!(told(second), Some(wanted(opened[1])));
}

#[test]
fn the_focused_window_is_activated_and_gets_the_keyboard() {
    let mut backend = backend();
    let mut engine = engine();
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let first = client.open_window("one", "app");
    let second = client.open_window("two", "app");
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    let activated: Vec<_> = [first, second]
        .into_iter()
        .filter(|index| client.window(*index).activated)
        .collect();
    assert_eq!(activated.len(), 1, "exactly one window is active");
    assert!(
        client.keyboard_enters() >= 1,
        "the keyboard entered a surface"
    );
}

#[test]
fn a_destroyed_window_is_reported_closed() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let index = client.open_window("short lived", "app");
    let events = client.settle(&mut backend);
    let Some(Event::WindowOpened { id, .. }) = events
        .iter()
        .find(|event| matches!(event, Event::WindowOpened { .. }))
    else {
        panic!("the window was never reported");
    };

    client.destroy_window(index);
    let events = client.settle(&mut backend);

    assert!(events.contains(&Event::WindowClosed(*id)));
}

#[test]
fn a_close_effect_asks_the_client_to_close() {
    let mut backend = backend();
    let (mut client, _) = TestClient::connect(&mut backend);
    let index = client.open_window("closing", "app");
    let events = client.settle(&mut backend);
    let Some(Event::WindowOpened { id, .. }) = events
        .iter()
        .find(|event| matches!(event, Event::WindowOpened { .. }))
    else {
        panic!("the window was never reported");
    };

    backend
        .apply_effects(&[Effect::Close(*id)])
        .expect("effects apply");
    client.settle(&mut backend);

    assert!(client.window(index).closed);
}

#[test]
fn a_grabbed_key_goes_to_the_core_and_never_to_the_client() {
    let mut backend = backend();
    let mut engine = engine();
    let mut keymap = Keymap::new();
    let super_q = KeyPress {
        keysym: Keysym::from_char('q').expect("q is printable"),
        modifiers: Modifiers::SUPER,
    };
    keymap.bind(super_q, Action::Quit).expect("a free binding");
    engine = engine.with_keymap(keymap);
    backend.grab_keys(engine.grabs());

    let (mut client, mut events) = TestClient::connect(&mut backend);
    client.open_window("typing", "app");
    events.extend(client.settle(&mut backend));
    drive(&mut engine, &mut backend, events);
    client.settle(&mut backend);

    // The Super key itself is the client's to see; only the combination is taken.
    assert_eq!(key(&mut backend, KEY_LEFTMETA, KeyAction::Press), None);
    let after_modifier = {
        client.settle(&mut backend);
        client.keys()
    };
    assert_eq!(key(&mut backend, KEY_Q, KeyAction::Press), Some(super_q));
    assert_eq!(key(&mut backend, KEY_Q, KeyAction::Release), None);
    client.settle(&mut backend);
    assert_eq!(client.keys(), after_modifier, "the grabbed key was hidden");

    assert_eq!(key(&mut backend, KEY_W, KeyAction::Press), None);
    client.settle(&mut backend);
    assert_eq!(
        client.keys(),
        after_modifier + 1,
        "an ungrabbed key got through"
    );
}
