//! Recorded event streams replayed through the engine, with the resulting scene checked.

use lutece::{
    Action, Change, Direction, Effect, Engine, Error, Event, KeyPress, Keymap, Keysym, Layout,
    ManageRule, Match, Modifiers, Outcome, Output, OutputId, Rect, Rules, Scene, SpawnCommand,
    StackSet, Tag, WinId, WindowInfo, diff,
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
        prop::collection::vec(0u32..4, 0..4).prop_map(|ids| {
            let outputs = ids
                .into_iter()
                .map(|id| Output {
                    id: OutputId::new(id),
                    rect: Rect::new(i32::try_from(id).expect("small") * 1000, 0, 1000, 600)
                        .expect("non-zero size"),
                })
                .collect();
            Event::OutputsChanged(outputs)
        }),
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
            let shown: BTreeSet<u64> = engine
                .stacks()
                .screens()
                .flat_map(|(_, ws)| ws.windows().map(WinId::get).collect::<Vec<_>>())
                .collect();
            if model.has_screen {
                prop_assert_eq!(scene.placements().iter().map(|p| p.id.get()).collect::<BTreeSet<_>>(), shown);
                prop_assert_eq!(scene.focus(), engine.stacks().focused());
                // On each screen, floating windows are drawn above the tiled ones.
                for (_, workspace) in engine.stacks().screens() {
                    let floating: Vec<bool> = scene
                        .placements()
                        .iter()
                        .filter(|p| workspace.contains(p.id))
                        .map(|p| workspace.is_floating(p.id))
                        .collect();
                    prop_assert!(floating.windows(2).all(|pair| pair[0] <= pair[1]));
                }
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
        (key('n', Modifiers::SUPER), Action::NextLayout),
        (key('g', Modifiers::SUPER), Action::GrowMaster),
        (key('s', Modifiers::SUPER), Action::ShrinkMaster),
        (key('f', Modifiers::SUPER), Action::ToggleFloat),
        (
            key('m', Modifiers::SUPER),
            Action::MoveFloat(Direction::Left),
        ),
        (
            key('r', Modifiers::SUPER),
            Action::ResizeFloat(Direction::Right),
        ),
        (key('o', Modifiers::SUPER), Action::FocusNextOutput),
        (key('p', Modifiers::SUPER), Action::FocusPrevOutput),
        (key('o', shift), Action::SendToNextOutput),
        (key('p', shift), Action::SendToPrevOutput),
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
    assert_eq!(engine.grabs().count(), 20);
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

fn master_width(engine: &Engine) -> u32 {
    scene(engine).placements()[0].rect.width()
}

#[test]
fn master_keys_resize_the_column_and_stop_at_the_edges() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);
    assert_eq!(master_width(&engine), 550);

    engine.handle(pressed('g', Modifiers::SUPER));
    assert_eq!(master_width(&engine), 600);
    engine.handle(pressed('s', Modifiers::SUPER));
    engine.handle(pressed('s', Modifiers::SUPER));
    assert_eq!(master_width(&engine), 500);

    for _ in 0..40 {
        engine.handle(pressed('g', Modifiers::SUPER));
    }
    assert_eq!(master_width(&engine), 999);
}

#[test]
fn the_layout_key_cycles_and_tile_keeps_its_ratio() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);
    engine.handle(pressed('g', Modifiers::SUPER));

    engine.handle(pressed('n', Modifiers::SUPER));
    assert_eq!(engine.stacks().current().layout(), Layout::Monocle);
    assert!(
        scene(&engine)
            .placements()
            .iter()
            .all(|p| p.rect.width() == 1000)
    );

    // Resizing is ignored while monocle shows.
    let before = engine.clone();
    engine.handle(pressed('g', Modifiers::SUPER));
    assert_eq!(engine, before);

    // Grid and centered master come between monocle and tile again.
    engine.handle(pressed('n', Modifiers::SUPER));
    assert_eq!(engine.stacks().current().layout(), Layout::Grid);
    engine.handle(pressed('n', Modifiers::SUPER));
    assert!(matches!(
        engine.stacks().current().layout(),
        Layout::CenteredMaster(_)
    ));
    engine.handle(pressed('n', Modifiers::SUPER));
    assert_eq!(master_width(&engine), 600);
}

#[test]
fn each_workspace_keeps_its_own_layout() {
    let mut engine = bound_engine();
    let shift = Modifiers::SUPER | Modifiers::SHIFT;
    replay(&mut engine, [screen(1000, 600), open(1)]);
    engine.handle(pressed('n', Modifiers::SUPER));

    engine.handle(pressed('2', shift));
    engine.handle(pressed('2', Modifiers::SUPER));

    let layouts: Vec<_> = engine.stacks().workspaces().map(|ws| ws.layout()).collect();
    assert_eq!(
        layouts,
        [Layout::Monocle, Layout::default(), Layout::default()]
    );
}

fn open_class(raw: u64, class: &str) -> Event {
    Event::WindowOpened {
        id: w(raw),
        info: WindowInfo {
            class: class.into(),
            ..WindowInfo::default()
        },
    }
}

fn ruled_engine() -> Engine {
    let mut rules = Rules::new();
    rules.push(ManageRule::new(
        Match::Class("Xmessage".into()),
        Outcome::Ignore,
    ));
    rules.push(ManageRule::new(
        Match::Class("Firefox".into()),
        Outcome::SendTo(tag("3")),
    ));
    rules.push(ManageRule::new(Match::Dialog, Outcome::Float));
    engine().with_rules(rules).expect("tag 3 exists")
}

fn open_dialog(raw: u64, parent: u64) -> Event {
    Event::WindowOpened {
        id: w(raw),
        info: WindowInfo {
            transient_for: Some(w(parent)),
            ..WindowInfo::default()
        },
    }
}

fn rect(x: i32, y: i32, width: u32, height: u32) -> Option<Rect> {
    Some(Rect::new(x, y, width, height).expect("non-zero size"))
}

#[test]
fn a_send_rule_opens_the_window_on_its_workspace_without_switching() {
    let mut engine = ruled_engine();
    replay(
        &mut engine,
        [screen(1000, 600), open(1), open_class(2, "Firefox")],
    );

    assert_eq!(ids(&scene(&engine)), [1]);
    assert_eq!(engine.stacks().tag_of(w(2)), Some(&tag("3")));

    engine.handle(Event::FocusRequested(w(2)));
    assert_eq!(ids(&scene(&engine)), [2]);
}

#[test]
fn an_ignored_window_is_released_and_never_managed() {
    let mut engine = ruled_engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = engine.clone();

    let effects = engine.handle(open_class(2, "Xmessage"));

    assert_eq!(effects, [Effect::Release(w(2))]);
    assert_eq!(engine, before);
    assert!(engine.handle(Event::FocusRequested(w(2))).is_empty());
    assert_eq!(engine, before);
}

#[test]
fn a_window_no_rule_matches_opens_on_the_current_workspace() {
    let mut engine = ruled_engine();
    let effects = engine.handle(open_class(1, "Other"));
    assert!(effects.is_empty());
    assert_eq!(engine.stacks().tag_of(w(1)), Some(&tag("1")));
}

#[test]
fn rules_that_name_a_missing_workspace_are_refused() {
    let mut rules = Rules::new();
    rules.push(ManageRule::new(Match::Dialog, Outcome::SendTo(tag("9"))));
    assert_eq!(engine().with_rules(rules), Err(Error::UnknownTag(tag("9"))));
}

#[test]
fn a_float_rule_opens_the_window_above_tiles_that_keep_the_screen() {
    let mut engine = ruled_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open_dialog(2, 1)]);

    let scene = scene(&engine);
    assert_eq!(ids(&scene), [1, 2]);
    assert_eq!(scene.get(w(1)).map(|p| p.rect), rect(0, 0, 1000, 600));
    assert_eq!(scene.get(w(2)).map(|p| p.rect), rect(200, 120, 600, 360));
    assert_eq!(scene.focus(), Some(w(2)));
}

#[test]
fn a_floating_window_is_above_the_tiles_even_when_it_is_not_focused() {
    let mut engine = ruled_engine();
    replay(
        &mut engine,
        [screen(1000, 600), open(1), open_dialog(2, 1), open(3)],
    );

    assert_eq!(ids(&scene(&engine)).last(), Some(&2));
    assert_eq!(scene(&engine).focus(), Some(w(3)));
}

#[test]
fn float_keys_toggle_move_and_resize_the_focused_window() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);

    // A tiled window ignores move and resize keys.
    let tiled = engine.clone();
    replay(
        &mut engine,
        [
            pressed('m', Modifiers::SUPER),
            pressed('r', Modifiers::SUPER),
        ],
    );
    assert_eq!(engine, tiled);

    replay(&mut engine, [pressed('f', Modifiers::SUPER)]);
    assert_eq!(
        scene(&engine).get(w(2)).map(|p| p.rect),
        rect(200, 120, 600, 360)
    );

    replay(
        &mut engine,
        [
            pressed('m', Modifiers::SUPER),
            pressed('r', Modifiers::SUPER),
        ],
    );
    assert_eq!(
        scene(&engine).get(w(2)).map(|p| p.rect),
        rect(160, 120, 640, 360)
    );
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 0, 1000, 600)
    );

    replay(&mut engine, [pressed('f', Modifiers::SUPER)]);
    assert!(!engine.stacks().current().is_floating(w(2)));
    let widths: Vec<u32> = scene(&engine)
        .placements()
        .iter()
        .map(|p| p.rect.width())
        .collect();
    assert_eq!(widths.iter().sum::<u32>(), 1000);
}

#[test]
fn a_focused_floating_window_stays_a_float_in_monocle() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            open(1),
            open(2),
            pressed('f', Modifiers::SUPER),
            pressed('n', Modifiers::SUPER),
        ],
    );

    let scene = scene(&engine);
    assert_eq!(ids(&scene), [1, 2]);
    assert_eq!(scene.get(w(1)).map(|p| p.rect), rect(0, 0, 1000, 600));
    assert_eq!(scene.get(w(2)).map(|p| p.rect), rect(200, 120, 600, 360));
}

#[test]
fn resizing_before_a_screen_exists_changes_nothing() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [
            open(1),
            pressed('f', Modifiers::SUPER),
            pressed('r', Modifiers::SUPER),
        ],
    );
    replay(&mut engine, [screen(1000, 600)]);
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(200, 120, 600, 360)
    );
}

fn output(id: u32, x: i32) -> Output {
    Output {
        id: OutputId::new(id),
        rect: Rect::new(x, 0, 1000, 600).expect("non-zero size"),
    }
}

/// Two side-by-side outputs, 1000x600 each, ids 0 and 1.
fn two_outputs() -> Event {
    Event::OutputsChanged(vec![output(0, 0), output(1, 1000)])
}

fn super_shift() -> Modifiers {
    Modifiers::SUPER | Modifiers::SHIFT
}

/// Windows 1 and 2 on the first output's workspace, then 2 sent to the second output.
fn split_engine() -> Engine {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [two_outputs(), open(1), open(2), pressed('o', super_shift())],
    );
    engine
}

#[test]
fn each_output_shows_its_own_workspace_in_its_own_area() {
    let engine = split_engine();

    let scene = scene(&engine);
    assert_eq!(scene.get(w(1)).map(|p| p.rect), rect(0, 0, 1000, 600));
    assert_eq!(scene.get(w(2)).map(|p| p.rect), rect(1000, 0, 1000, 600));
    // Sending a window away does not take the focus with it.
    assert_eq!(scene.focus(), Some(w(1)));
}

#[test]
fn the_output_keys_move_the_focus_and_wrap() {
    let mut engine = split_engine();

    replay(&mut engine, [pressed('o', Modifiers::SUPER)]);
    assert_eq!(scene(&engine).focus(), Some(w(2)));
    assert_eq!(engine.stacks().current_output(), OutputId::new(1));

    replay(&mut engine, [pressed('o', Modifiers::SUPER)]);
    assert_eq!(scene(&engine).focus(), Some(w(1)));

    replay(&mut engine, [pressed('p', Modifiers::SUPER)]);
    assert_eq!(scene(&engine).focus(), Some(w(2)));
}

#[test]
fn a_new_window_opens_on_the_focused_outputs_workspace() {
    let mut engine = split_engine();
    replay(&mut engine, [pressed('o', Modifiers::SUPER), open(3)]);

    let scene = scene(&engine);
    assert_eq!(scene.focus(), Some(w(3)));
    assert!(scene.get(w(3)).is_some_and(|p| p.rect.x() >= 1000));
    assert_eq!(scene.get(w(1)).map(|p| p.rect), rect(0, 0, 1000, 600));
}

#[test]
fn focusing_an_empty_output_leaves_no_focus() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [two_outputs(), open(1), pressed('o', Modifiers::SUPER)],
    );

    let scene = scene(&engine);
    assert_eq!(scene.focus(), None);
    assert_eq!(ids(&scene), [1]);
}

#[test]
fn a_focus_request_for_a_window_on_another_output_moves_the_focus_there() {
    let mut engine = split_engine();
    replay(&mut engine, [Event::FocusRequested(w(2))]);

    assert_eq!(engine.stacks().current_output(), OutputId::new(1));
    assert_eq!(scene(&engine).focus(), Some(w(2)));
}

#[test]
fn an_unplugged_output_hides_its_windows_and_a_returning_one_shows_them_again() {
    let mut engine = split_engine();

    replay(&mut engine, [Event::OutputsChanged(vec![output(0, 0)])]);
    assert_eq!(ids(&scene(&engine)), [1]);
    assert!(engine.stacks().contains(w(2)));

    replay(&mut engine, [two_outputs()]);
    assert_eq!(
        scene(&engine).get(w(2)).map(|p| p.rect),
        rect(1000, 0, 1000, 600)
    );
}

#[test]
fn a_floating_window_is_placed_relative_to_its_own_output() {
    let mut engine = split_engine();
    replay(
        &mut engine,
        [
            pressed('o', Modifiers::SUPER),
            pressed('f', Modifiers::SUPER),
        ],
    );

    assert_eq!(
        scene(&engine).get(w(2)).map(|p| p.rect),
        rect(1200, 120, 600, 360)
    );
}

#[test]
fn an_output_that_is_too_small_is_an_error_not_a_bad_scene() {
    let mut engine = bound_engine();
    let tiny = Output {
        id: OutputId::new(1),
        rect: Rect::new(1000, 0, 1, 1).expect("non-zero size"),
    };
    replay(
        &mut engine,
        [
            Event::OutputsChanged(vec![output(0, 0), tiny]),
            open(1),
            open(2),
            pressed('o', super_shift()),
        ],
    );
    // One window fits a 1x1 output; two tiled windows do not.
    assert!(engine.scene().is_ok());
    replay(&mut engine, [pressed('o', super_shift())]);
    assert!(matches!(
        engine.scene(),
        Err(Error::AreaTooSmall { windows: 2, .. })
    ));
}
