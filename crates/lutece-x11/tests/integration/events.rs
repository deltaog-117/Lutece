//! What the backend reports besides windows and keys: a window's title and class changing,
//! the pointer entering a window, and waiting for events with a timeout.
//!
//! The backend runs inside the test, on a private X server, with an engine to place the
//! windows; the test acts as the clients.

use crate::support::Session;
use lutece::{Engine, Event, Layout, StackSet, Tag, WinId};
use lutece_x11::X11Backend;
use std::time::{Duration, Instant};
use x11rb::protocol::xproto::Window;

use crate::support::Spec;

/// A backend managing a private display, and what it has reported so far.
struct Rig {
    backend: X11Backend,
    engine: Engine,
    seen: Vec<Event>,
}

impl Rig {
    fn new(session: &Session) -> Self {
        let tags = ["1"].map(|name| Tag::try_new(name).expect("has text"));
        Self {
            backend: X11Backend::connect_to(Some(session.display())).expect("takes over"),
            engine: Engine::new(StackSet::new(tags).expect("distinct"), Layout::default()),
            seen: Vec::new(),
        }
    }

    /// Takes the events that are ready, feeds them to the engine and shows its scene.
    fn pump(&mut self) {
        let events = self
            .backend
            .next_events_timeout(Duration::from_millis(20))
            .expect("events");
        for event in events {
            self.seen.push(event.clone());
            self.engine.handle(event);
        }
        let scene = self.engine.scene().expect("a scene");
        self.backend.apply_scene(&scene).expect("applies");
    }
}

fn id(window: Window) -> WinId {
    WinId::new(u64::from(window))
}

#[test]
fn a_change_of_title_or_class_is_reported_once_per_real_change() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut rig = Rig::new(&session);
    let window = session.client.open(Spec::default().class("Term"));
    session.client.wait_for("the window to be reported", || {
        rig.pump();
        rig.seen
            .iter()
            .any(|event| matches!(event, Event::WindowOpened { id: opened, .. } if *opened == id(window)))
            .then_some(())
    });
    rig.seen.clear();

    let reported = |rig: &mut Rig, title: &str| {
        session.client.wait_for("a title to be reported", || {
            rig.pump();
            rig.seen
                .iter()
                .any(|event| {
                    matches!(event, Event::WindowInfoChanged { info, .. } if info.title == title)
                })
                .then_some(())
        });
    };
    session.client.set_title(window, "one");
    reported(&mut rig, "one");
    // The same text again is not a change; the next one is. The backend reads the title when
    // it handles the event, so changes made before it looks are one change.
    session.client.set_title(window, "one");
    session.client.set_title(window, "two");
    reported(&mut rig, "two");
    rig.pump();
    let titles: Vec<&str> = rig
        .seen
        .iter()
        .filter_map(|event| match event {
            Event::WindowInfoChanged { id: changed, info } if *changed == id(window) => {
                assert_eq!(info.class, "Term", "the report is the complete description");
                Some(info.title.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["one", "two"]);
}

#[test]
fn the_pointer_entering_a_window_is_reported_as_such_and_not_as_a_focus_request() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut rig = Rig::new(&session);
    let left = session.client.open(Spec::default());
    let right = session.client.open(Spec::default());
    session.client.wait_for("both windows to be placed", || {
        rig.pump();
        let (a, b) = (
            session.client.geometry(left)?,
            session.client.geometry(right)?,
        );
        (session.client.is_shown(left) && session.client.is_shown(right) && a != b)
            .then_some((a, b))
    });
    let centre = |window| {
        let g = session.client.geometry(window).expect("placed");
        (
            g.x + i16::try_from(g.width / 2).expect("fits"),
            g.y + i16::try_from(g.height / 2).expect("fits"),
        )
    };
    session.client.move_pointer(centre(left));
    rig.seen.clear();

    session.client.move_pointer(centre(right));

    session.client.wait_for("the pointer to be reported", || {
        rig.pump();
        rig.seen
            .contains(&Event::PointerEntered(id(right)))
            .then_some(())
    });
    assert!(
        !rig.seen
            .iter()
            .any(|event| matches!(event, Event::FocusRequested(_)))
    );
}

#[test]
fn waiting_with_a_timeout_returns_nothing_when_nothing_happens() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut rig = Rig::new(&session);
    // The monitors are reported first.
    rig.pump();
    let started = Instant::now();

    let events = rig
        .backend
        .next_events_timeout(Duration::from_millis(150))
        .expect("waits");

    assert!(events.is_empty());
    assert!(started.elapsed() >= Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn set_grabs_lets_go_of_the_keys_that_are_no_longer_wanted() {
    use crate::support::{SUPER, key};
    use lutece::{KeyPress, Keysym, Modifiers};

    let Some(session) = Session::bare() else {
        return;
    };
    let mut rig = Rig::new(&session);
    let press = |c: char| KeyPress {
        keysym: Keysym::from_char(c).expect("a letter"),
        modifiers: Modifiers::SUPER,
    };
    rig.pump();
    rig.backend.set_grabs([press('a')]).expect("grabs a");
    rig.backend
        .set_grabs([press('b')])
        .expect("grabs b, lets go of a");
    rig.seen.clear();

    // The key that is no longer grabbed goes to the focused window, so the backend sees
    // nothing; the key that is grabbed is reported. Both are pressed before it looks.
    session.client.press(&[SUPER], key('a'));
    session.client.press(&[SUPER], key('b'));

    session
        .client
        .wait_for("the grabbed key to be reported", || {
            rig.pump();
            rig.seen
                .contains(&Event::KeyPressed(press('b')))
                .then_some(())
        });
    assert!(!rig.seen.contains(&Event::KeyPressed(press('a'))));
}
