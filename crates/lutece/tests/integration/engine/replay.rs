//! Recorded event streams replayed through the engine, with the resulting scene checked.

use lutece::{
    Action, Change, Effect, Engine, Error, Event, KeyPress, Keymap, Keysym, Layout, Modifiers,
    Output, OutputId, Rect, Scene, SpawnCommand, StackSet, Tag, WinId, WindowInfo, diff,
};
use proptest::prelude::*;
use std::collections::BTreeSet;

fn w(raw: u64) -> WinId {
    WinId::new(raw)
}

fn tag(name: &str) -> Tag {
    Tag::try_new(name).expect("test tags have text")
}

fn stacks() -> StackSet {
    StackSet::new(["1", "2", "3"].map(tag)).expect("three distinct tags")
}

fn engine() -> Engine {
    Engine::new(stacks(), Layout::default())
}

fn screen(width: u32, height: u32) -> Event {
    let rect = Rect::new(0, 0, width, height).expect("non-zero size");
    Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect,
    }])
}

fn open(raw: u64) -> Event {
    Event::WindowOpened {
        id: w(raw),
        info: WindowInfo::default(),
    }
}

/// Feeds every event to the engine, in order.
fn replay(engine: &mut Engine, events: impl IntoIterator<Item = Event>) {
    for event in events {
        engine.handle(event);
    }
}

fn scene(engine: &Engine) -> Scene {
    engine.scene().expect("the screen fits the windows")
}

fn ids(scene: &Scene) -> Vec<u64> {
    scene.placements().iter().map(|p| p.id.get()).collect()
}

#[test]
fn nothing_is_shown_before_a_screen_is_reported() {
    let mut engine = engine();
    replay(&mut engine, [open(1)]);
    assert_eq!(scene(&engine), Scene::empty());
}

#[test]
fn a_single_window_fills_the_screen_and_has_focus() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);

    let scene = scene(&engine);
    assert_eq!(ids(&scene), [1]);
    assert_eq!(scene.focus(), Some(w(1)));
    assert_eq!(
        scene.get(w(1)).map(|p| p.rect),
        Some(Rect::new(0, 0, 1000, 600).expect("non-zero size"))
    );
}

#[test]
fn the_newest_window_takes_the_master_column_and_focus() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2), open(3)]);

    let scene = scene(&engine);
    // Reading order is 3, 2, 1: the layout lists the master first.
    assert_eq!(ids(&scene), [3, 2, 1]);
    assert_eq!(scene.focus(), Some(w(3)));

    let master = scene.get(w(3)).expect("visible").rect;
    assert_eq!((master.x(), master.height()), (0, 600));
    let covered: u64 = scene
        .placements()
        .iter()
        .map(|p| u64::from(p.rect.width()) * u64::from(p.rect.height()))
        .sum();
    assert_eq!(covered, 1000 * 600);
}

#[test]
fn closing_the_focused_window_passes_focus_on() {
    let mut engine = engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            open(1),
            open(2),
            Event::WindowClosed(w(2)),
        ],
    );

    let scene = scene(&engine);
    assert_eq!(ids(&scene), [1]);
    assert_eq!(scene.focus(), Some(w(1)));
}

#[test]
fn a_focus_request_moves_focus_without_reordering_windows() {
    let mut engine = engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            open(1),
            open(2),
            Event::FocusRequested(w(1)),
        ],
    );

    let scene = scene(&engine);
    assert_eq!(scene.focus(), Some(w(1)));
    assert_eq!(ids(&scene), [2, 1]);
}

#[test]
fn a_focus_request_for_a_window_elsewhere_switches_workspace() {
    let mut stacks = stacks();
    stacks.insert(w(1)).expect("new window");
    stacks.focus_tag(&tag("2")).expect("workspace exists");
    stacks.insert(w(2)).expect("new window");
    stacks.focus_tag(&tag("1")).expect("workspace exists");

    let mut engine = Engine::new(stacks, Layout::default());
    replay(&mut engine, [screen(1000, 600)]);
    assert_eq!(ids(&scene(&engine)), [1]);

    replay(&mut engine, [Event::FocusRequested(w(2))]);
    let scene = scene(&engine);
    assert_eq!(ids(&scene), [2]);
    assert_eq!(scene.focus(), Some(w(2)));
}

#[test]
fn events_that_no_longer_apply_change_nothing() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);
    let before = engine.clone();

    replay(
        &mut engine,
        [
            open(2),
            Event::WindowClosed(w(9)),
            Event::FocusRequested(w(9)),
            Event::KeyPressed(KeyPress {
                keysym: Keysym(0x61),
                modifiers: Modifiers::SUPER,
            }),
        ],
    );
    assert_eq!(engine, before);
}

#[test]
fn losing_every_output_leaves_nothing_to_show() {
    let mut engine = engine();
    replay(
        &mut engine,
        [screen(1000, 600), open(1), Event::OutputsChanged(vec![])],
    );
    assert_eq!(scene(&engine), Scene::empty());
}

#[test]
fn a_screen_too_small_for_the_windows_is_an_error_not_a_bad_scene() {
    let mut engine = engine();
    replay(&mut engine, [screen(1, 1), open(1), open(2)]);
    assert!(matches!(
        engine.scene(),
        Err(Error::AreaTooSmall { windows: 2, .. })
    ));
}

#[test]
fn opening_a_window_diffs_to_a_show_and_a_move() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = scene(&engine);

    replay(&mut engine, [open(2)]);
    let changes = diff(&before, &scene(&engine));

    assert!(matches!(changes.first(), Some(Change::Show(p)) if p.id == w(2)));
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, Change::Move(p) if p.id == w(1)))
    );
}

/// What a recorded stream does to the set of open windows and to the screen.
#[derive(Debug, Default)]
struct Model {
    open: BTreeSet<u64>,
    has_screen: bool,
}

impl Model {
    fn apply(&mut self, event: &Event) {
        match event {
            Event::WindowOpened { id, .. } => {
                self.open.insert(id.get());
            }
            Event::WindowClosed(id) => {
                self.open.remove(&id.get());
            }
            Event::OutputsChanged(outputs) => self.has_screen = !outputs.is_empty(),
            _ => {}
        }
    }
}

fn arbitrary_event() -> impl Strategy<Value = Event> {
    prop_oneof![
        (0u64..8).prop_map(open),
        (0u64..8).prop_map(|n| Event::WindowClosed(w(n))),
        (0u64..8).prop_map(|n| Event::FocusRequested(w(n))),
        // Bound keys that move focus, windows and workspaces, an unbound one, and the
        // effect-only actions.
        prop::sample::select(keymap().presses().collect::<Vec<_>>()).prop_map(Event::KeyPressed),
        Just(Event::KeyPressed(KeyPress {
            keysym: Keysym(0x61),
            modifiers: Modifiers::SUPER,
        })),
        (100u32..2000, 100u32..2000).prop_map(|(width, height)| screen(width, height)),
        Just(Event::OutputsChanged(vec![])),
    ]
}

proptest! {
    #[test]
    fn any_event_stream_keeps_the_state_and_scene_consistent(
        events in prop::collection::vec(arbitrary_event(), 0..60),
    ) {
        // Windows 0 and 1 start on other workspaces, so focus requests can cross them.
        let mut stacks = stacks();
        stacks.focus_tag(&tag("2")).expect("workspace exists");
        stacks.insert(w(0)).expect("new window");
        stacks.focus_tag(&tag("3")).expect("workspace exists");
        stacks.insert(w(1)).expect("new window");
        stacks.focus_tag(&tag("1")).expect("workspace exists");

        let mut engine = Engine::new(stacks, Layout::default()).with_keymap(keymap());
        let mut model = Model { open: BTreeSet::from([0, 1]), has_screen: false };

        for event in events {
            model.apply(&event);
            engine.handle(event);

            // Every open window is managed on exactly one workspace, and nothing else is.
            let managed: Vec<u64> = engine
                .stacks()
                .workspaces()
                .flat_map(|ws| ws.windows().map(WinId::get).collect::<Vec<_>>())
                .collect();
            prop_assert_eq!(managed.len(), model.open.len());
            prop_assert_eq!(managed.into_iter().collect::<BTreeSet<_>>(), model.open.clone());

            // The screen is large enough for every window, so the scene always builds.
            let scene = engine.scene().expect("the screen fits the windows");
            let current: BTreeSet<u64> =
                engine.stacks().current().windows().map(WinId::get).collect();
            if model.has_screen {
                prop_assert_eq!(scene.placements().iter().map(|p| p.id.get()).collect::<BTreeSet<_>>(), current);
                prop_assert_eq!(scene.focus(), engine.stacks().focused());
            } else {
                prop_assert!(scene.placements().is_empty());
            }
        }
    }
}

fn key(c: char, modifiers: Modifiers) -> KeyPress {
    KeyPress {
        keysym: Keysym::from_char(c).expect("test keys are printable"),
        modifiers,
    }
}

fn pressed(c: char, modifiers: Modifiers) -> Event {
    Event::KeyPressed(key(c, modifiers))
}

fn keymap() -> Keymap {
    let mut keymap = Keymap::new();
    let shift = Modifiers::SUPER | Modifiers::SHIFT;
    let binds = [
        (key('q', shift), Action::Quit),
        (key('c', shift), Action::Close),
        (
            key('t', Modifiers::SUPER),
            Action::Spawn(SpawnCommand::try_new("xterm", Vec::<String>::new()).expect("program")),
        ),
        (key('j', Modifiers::SUPER), Action::FocusNext),
        (key('k', Modifiers::SUPER), Action::FocusPrev),
        (key('2', Modifiers::SUPER), Action::ViewTag(tag("2"))),
        (key('2', shift), Action::MoveToTag(tag("2"))),
        (key('9', Modifiers::SUPER), Action::ViewTag(tag("9"))),
        (key('l', Modifiers::SUPER), Action::SwapNext),
        (key('h', Modifiers::SUPER), Action::SwapPrev),
    ];
    for (press, action) in binds {
        keymap.bind(press, action).expect("distinct bindings");
    }
    keymap
}

fn bound_engine() -> Engine {
    engine().with_keymap(keymap())
}

#[test]
fn the_engine_lists_the_keys_to_grab() {
    let engine = bound_engine();
    assert_eq!(engine.grabs().count(), 10);
    assert_eq!(
        engine.grabs().next(),
        Some(key('q', Modifiers::SUPER | Modifiers::SHIFT))
    );
    assert_eq!(self::engine().grabs().count(), 0);
}

#[test]
fn quit_spawn_and_close_come_back_as_effects() {
    let mut engine = bound_engine();
    let shift = Modifiers::SUPER | Modifiers::SHIFT;
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);

    assert_eq!(engine.handle(pressed('q', shift)), [Effect::Quit]);
    assert_eq!(engine.handle(pressed('c', shift)), [Effect::Close(w(2))]);
    let spawned = engine.handle(pressed('t', Modifiers::SUPER));
    assert!(matches!(spawned.as_slice(), [Effect::Spawn(cmd)] if cmd.program() == "xterm"));
}

#[test]
fn closing_with_nothing_focused_does_nothing() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600)]);
    assert!(
        engine
            .handle(pressed('c', Modifiers::SUPER | Modifiers::SHIFT))
            .is_empty()
    );
}

#[test]
fn focus_keys_move_the_focus_and_wrap() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);
    assert_eq!(scene(&engine).focus(), Some(w(2)));

    assert!(engine.handle(pressed('j', Modifiers::SUPER)).is_empty());
    let after_next = scene(&engine).focus();
    engine.handle(pressed('k', Modifiers::SUPER));
    assert_ne!(scene(&engine).focus(), after_next);
    assert_eq!(scene(&engine).focus(), Some(w(2)));
}

#[test]
fn tag_keys_switch_and_move_windows_and_ignore_unknown_tags() {
    let mut engine = bound_engine();
    let shift = Modifiers::SUPER | Modifiers::SHIFT;
    replay(&mut engine, [screen(1000, 600), open(1)]);

    engine.handle(pressed('2', shift));
    assert!(scene(&engine).placements().is_empty());
    assert_eq!(engine.stacks().tag_of(w(1)).map(Tag::as_str), Some("2"));

    engine.handle(pressed('2', Modifiers::SUPER));
    assert_eq!(engine.stacks().current().tag().as_str(), "2");
    assert_eq!(ids(&scene(&engine)), [1]);

    let before = engine.clone();
    engine.handle(pressed('9', Modifiers::SUPER));
    assert_eq!(engine, before);
}

#[test]
fn a_press_that_is_not_bound_changes_nothing() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = engine.clone();
    assert!(engine.handle(pressed('j', Modifiers::NONE)).is_empty());
    assert!(engine.handle(pressed('z', Modifiers::SUPER)).is_empty());
    assert_eq!(engine, before);
}
