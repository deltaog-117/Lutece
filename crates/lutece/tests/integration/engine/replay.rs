//! Recorded event streams replayed through the engine, with the resulting scene checked.

use lutece::{
    Action, Change, Curve, Direction, Effect, Engine, Error, Event, Insets, KeyPress, Keymap,
    Keysym, Layout, ManageDecision, ManageRule, Match, Modifiers, Outcome, Output, OutputId,
    Placement, Rect, Rules, Scene, SpawnCommand, StackSet, Style, Tag, Transition, WinId,
    WindowInfo, diff,
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
        ((0u64..8), any::<bool>()).prop_map(|(n, on)| Event::FullscreenRequested { id: w(n), on }),
        // Styles for windows that may not exist, may be hidden, or may be reset to plain.
        ((0u64..8), 0u16..30, any::<u8>(), 0u16..400).prop_map(
            |(n, radius, opacity, duration_ms)| {
                let style = if radius == 0 {
                    Style::default()
                } else {
                    Style::default()
                        .with_radius(radius)
                        .with_opacity(opacity)
                        .with_transition(Transition {
                            curve: Curve::EaseInOut,
                            duration_ms,
                        })
                };
                Event::StyleChanged { id: w(n), style }
            }
        ),
        // Insets up to 90 pixels leave room for the windows on the smallest screen.
        ((0u32..4), (0u32..90), (0u32..90), (0u32..90), (0u32..90)).prop_map(
            |(output, left, top, right, bottom)| Event::ReservedChanged {
                output: OutputId::new(output),
                insets: Insets::new(left, top, right, bottom),
            }
        ),
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
                // On each screen, floating windows are drawn above the tiled ones, and a
                // fullscreen window above both.
                for (_, workspace) in engine.stacks().screens() {
                    let shown: Vec<_> = scene
                        .placements()
                        .iter()
                        .filter(|p| workspace.contains(p.id))
                        .collect();
                    let beneath: Vec<bool> = shown
                        .iter()
                        .filter(|p| workspace.fullscreen() != Some(p.id))
                        .map(|p| workspace.is_floating(p.id))
                        .collect();
                    prop_assert!(beneath.windows(2).all(|pair| pair[0] <= pair[1]));
                    if let Some(id) = workspace.fullscreen() {
                        prop_assert_eq!(shown.last().map(|p| p.id), Some(id));
                        prop_assert_eq!(workspace.focus(), Some(id));
                    }
                    for placement in &shown {
                        prop_assert_eq!(
                            scene.is_fullscreen(placement.id),
                            workspace.fullscreen() == Some(placement.id)
                        );
                    }
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
        (key('u', Modifiers::SUPER), Action::ToggleFullscreen),
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
    assert_eq!(engine.grabs().count(), 21);
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

fn drag(raw: u64, dx: i32, dy: i32, dw: i32, dh: i32) -> Event {
    Event::FloatAdjusted {
        id: w(raw),
        dx,
        dy,
        dw,
        dh,
    }
}

/// An engine with window 1 tiled and window 2 floating and focused, on a 1000x600 screen.
fn engine_with_a_floating_window() -> Engine {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);
    engine.handle(pressed('f', Modifiers::SUPER));
    engine
}

fn rect_of(engine: &Engine, raw: u64) -> Rect {
    scene(engine).get(w(raw)).expect("the window is shown").rect
}

#[test]
fn dragging_a_floating_window_moves_and_resizes_it() {
    let mut engine = engine_with_a_floating_window();
    let before = rect_of(&engine, 2);

    assert!(engine.handle(drag(2, 30, -20, 0, 0)).is_empty());
    let moved = rect_of(&engine, 2);
    assert_eq!((moved.x(), moved.y()), (before.x() + 30, before.y() - 20));
    assert_eq!(
        (moved.width(), moved.height()),
        (before.width(), before.height())
    );

    engine.handle(drag(2, 0, 0, 50, 10));
    let resized = rect_of(&engine, 2);
    assert_eq!((resized.x(), resized.y()), (moved.x(), moved.y()));
    assert_eq!(resized.width(), moved.width() + 50);
    assert_eq!(resized.height(), moved.height() + 10);
}

#[test]
fn dragging_a_tiled_or_unknown_window_changes_nothing() {
    let mut engine = engine_with_a_floating_window();
    let before = scene(&engine);

    engine.handle(drag(1, 30, 30, 10, 10));
    engine.handle(drag(99, 30, 30, 10, 10));

    assert_eq!(scene(&engine), before);
}

proptest! {
    #[test]
    fn dragging_and_dragging_back_restores_the_scene(
        dx in -50_000_i32..50_000, dy in -50_000_i32..50_000,
    ) {
        let mut engine = engine_with_a_floating_window();
        let before = scene(&engine);
        engine.handle(drag(2, dx, dy, 0, 0));
        engine.handle(drag(2, -dx, -dy, 0, 0));
        prop_assert_eq!(scene(&engine), before);
    }

    #[test]
    fn no_drag_can_make_a_scene_invalid(
        dx in any::<i32>(), dy in any::<i32>(), dw in any::<i32>(), dh in any::<i32>(),
    ) {
        let mut engine = engine_with_a_floating_window();
        engine.handle(drag(2, dx, dy, dw, dh));
        prop_assert!(engine.scene().is_ok());
        let rect = rect_of(&engine, 2);
        prop_assert!(rect.width() >= 1 && rect.height() >= 1);
    }
}

fn reserve(output: u32, left: u32, top: u32, right: u32, bottom: u32) -> Event {
    Event::ReservedChanged {
        output: OutputId::new(output),
        insets: Insets::new(left, top, right, bottom),
    }
}

fn fullscreen(raw: u64, on: bool) -> Event {
    Event::FullscreenRequested { id: w(raw), on }
}

#[test]
fn a_bar_shrinks_the_area_the_tiles_share() {
    let mut engine = engine();
    replay(
        &mut engine,
        [screen(1000, 600), reserve(0, 0, 30, 0, 0), open(1)],
    );
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 30, 1000, 570)
    );

    // A bar on each side: the reserve is the complete one, not added to the last.
    replay(&mut engine, [reserve(0, 10, 30, 20, 40)]);
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(10, 30, 970, 530)
    );

    replay(&mut engine, [reserve(0, 0, 0, 0, 0)]);
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 0, 1000, 600)
    );
}

#[test]
fn a_bar_only_reserves_space_on_its_own_output() {
    let mut engine = engine();
    replay(
        &mut engine,
        [two_outputs(), reserve(1, 0, 0, 0, 50), open(1)],
    );
    engine.handle(Event::FocusRequested(w(1)));
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 0, 1000, 600)
    );
}

#[test]
fn a_bar_reported_before_the_screen_still_applies() {
    let mut engine = engine();
    replay(
        &mut engine,
        [reserve(0, 0, 30, 0, 0), open(1), screen(1000, 600)],
    );
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 30, 1000, 570)
    );
}

#[test]
fn a_floating_window_is_centered_in_what_the_bar_leaves() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            reserve(0, 0, 100, 0, 0),
            open(1),
            pressed('f', Modifiers::SUPER),
        ],
    );
    // 60% of 1000x500, centered in the 500 pixels below the bar.
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(200, 200, 600, 300)
    );
}

#[test]
fn a_bar_claiming_most_of_the_screen_is_ignored() {
    let mut engine = engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            reserve(0, 0, 5000, 0, 5000),
            open(1),
            open(2),
        ],
    );
    assert_eq!(
        scene(&engine).get(w(2)).map(|p| p.rect).map(|r| r.height()),
        Some(600)
    );
}

#[test]
fn a_fullscreen_window_covers_the_bar_and_is_drawn_on_top() {
    let mut engine = engine();
    replay(
        &mut engine,
        [screen(1000, 600), reserve(0, 0, 30, 0, 0), open(1), open(2)],
    );
    let tiled_1 = scene(&engine).get(w(1)).map(|p| p.rect);

    replay(&mut engine, [fullscreen(2, true)]);
    let scene = scene(&engine);
    assert_eq!(scene.get(w(2)).map(|p| p.rect), rect(0, 0, 1000, 600));
    assert!(scene.is_fullscreen(w(2)));
    assert!(!scene.is_fullscreen(w(1)));
    assert_eq!(ids(&scene).last(), Some(&2));
    // The other window keeps the slot it would have had, under the fullscreen one.
    assert_eq!(scene.get(w(1)).map(|p| p.rect).map(|r| r.y()), Some(30));
    assert!(tiled_1.is_some());

    replay(&mut engine, [fullscreen(2, false)]);
    let back = self::scene(&engine);
    assert!(!back.is_fullscreen(w(2)));
    assert_eq!(back.get(w(2)).map(|p| p.rect).map(|r| r.y()), Some(30));
}

#[test]
fn the_fullscreen_key_toggles_the_focused_window() {
    let mut engine = bound_engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);

    engine.handle(pressed('u', Modifiers::SUPER));
    assert!(scene(&engine).is_fullscreen(w(2)));
    engine.handle(pressed('u', Modifiers::SUPER));
    assert!(!scene(&engine).is_fullscreen(w(2)));

    // Nothing to fullscreen on an empty workspace.
    let mut empty = bound_engine();
    replay(&mut empty, [screen(1000, 600)]);
    let before = empty.clone();
    empty.handle(pressed('u', Modifiers::SUPER));
    assert_eq!(empty, before);
}

#[test]
fn moving_the_focus_ends_fullscreen_so_the_focused_window_is_never_hidden() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [screen(1000, 600), open(1), open(2), fullscreen(2, true)],
    );

    engine.handle(pressed('j', Modifiers::SUPER));
    let scene = scene(&engine);
    assert_eq!(scene.focus(), Some(w(1)));
    assert!(!scene.is_fullscreen(w(2)));
}

#[test]
fn a_fullscreen_floating_window_returns_to_where_it_floated() {
    let mut engine = bound_engine();
    replay(
        &mut engine,
        [screen(1000, 600), open(1), pressed('f', Modifiers::SUPER)],
    );
    let floated = scene(&engine).get(w(1)).map(|p| p.rect);

    replay(&mut engine, [fullscreen(1, true)]);
    assert_eq!(
        scene(&engine).get(w(1)).map(|p| p.rect),
        rect(0, 0, 1000, 600)
    );
    replay(&mut engine, [fullscreen(1, false)]);
    assert_eq!(scene(&engine).get(w(1)).map(|p| p.rect), floated);
}

#[test]
fn fullscreen_requests_for_unknown_windows_change_nothing() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = engine.clone();
    replay(
        &mut engine,
        [
            fullscreen(9, true),
            fullscreen(9, false),
            fullscreen(1, false),
        ],
    );
    assert_eq!(engine, before);
}

#[test]
fn a_fullscreen_request_does_not_switch_workspace() {
    let mut stacks = stacks();
    stacks.insert(w(1)).expect("new window");
    stacks.focus_tag(&tag("2")).expect("workspace exists");
    stacks.insert(w(2)).expect("new window");
    stacks.focus_tag(&tag("1")).expect("workspace exists");
    let mut engine = Engine::new(stacks, Layout::default());
    replay(&mut engine, [screen(1000, 600), fullscreen(2, true)]);

    assert_eq!(ids(&scene(&engine)), [1]);
    assert_eq!(engine.stacks().current().tag().as_str(), "1");
    // The hidden window is fullscreen when its workspace comes back.
    engine.handle(Event::FocusRequested(w(2)));
    assert!(scene(&engine).is_fullscreen(w(2)));
}

#[test]
fn each_output_can_have_its_own_fullscreen_window() {
    let mut engine = split_engine();
    replay(&mut engine, [fullscreen(1, true), fullscreen(2, true)]);

    let scene = scene(&engine);
    assert_eq!(scene.get(w(1)).map(|p| p.rect), rect(0, 0, 1000, 600));
    assert_eq!(scene.get(w(2)).map(|p| p.rect), rect(1000, 0, 1000, 600));
    assert!(scene.is_fullscreen(w(1)) && scene.is_fullscreen(w(2)));
}

#[test]
fn fullscreen_shows_up_in_the_diff_before_the_move() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = scene(&engine);

    replay(&mut engine, [fullscreen(1, true)]);
    let changes = diff(&before, &scene(&engine));
    assert_eq!(changes, [Change::Fullscreen { id: w(1), on: true }]);

    replay(&mut engine, [reserve(0, 0, 30, 0, 0)]);
    // The window is fullscreen, so the bar does not move it.
    assert!(diff(&scene(&engine), &scene(&engine)).is_empty());
}

fn styled(raw: u64, style: Style) -> Event {
    Event::StyleChanged { id: w(raw), style }
}

#[test]
fn a_style_set_through_an_event_reaches_the_scene_and_the_diff() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = scene(&engine);

    let style = Style::default()
        .with_radius(10)
        .with_transition(Transition {
            curve: Curve::EaseOut,
            duration_ms: 150,
        });
    replay(&mut engine, [styled(1, style)]);
    let after = scene(&engine);
    assert_eq!(after.style_of(w(1)), style);
    assert_eq!(diff(&before, &after), [Change::Style { id: w(1), style }]);

    replay(&mut engine, [styled(1, Style::default())]);
    assert_eq!(scene(&engine), before);
}

#[test]
fn a_style_for_an_unknown_window_is_ignored_and_does_not_linger() {
    let mut engine = engine();
    replay(
        &mut engine,
        [
            screen(1000, 600),
            styled(7, Style::default().with_radius(5)),
        ],
    );
    replay(&mut engine, [open(7)]);
    assert_eq!(scene(&engine).style_of(w(7)), Style::default());
}

#[test]
fn a_closed_window_loses_its_style() {
    let mut engine = engine();
    let style = Style::default().with_opacity(100);
    replay(&mut engine, [screen(1000, 600), open(1), styled(1, style)]);

    // The id comes back as a new window, which must not inherit the old look.
    replay(&mut engine, [Event::WindowClosed(w(1)), open(1)]);
    assert_eq!(scene(&engine).style_of(w(1)), Style::default());
}

#[test]
fn a_hidden_window_keeps_its_style_for_when_it_returns() {
    let mut engine = bound_engine();
    let shift = Modifiers::SUPER | Modifiers::SHIFT;
    let style = Style::default().with_radius(9);
    replay(&mut engine, [screen(1000, 600), open(1), styled(1, style)]);

    engine.handle(pressed('2', shift));
    assert!(scene(&engine).placements().is_empty());

    engine.handle(pressed('2', Modifiers::SUPER));
    assert_eq!(scene(&engine).style_of(w(1)), style);
}

#[test]
fn an_action_request_does_what_the_bound_key_does() {
    let setup = [screen(1000, 600), open(1), open(2)];
    let mut by_key = bound_engine();
    let mut by_request = bound_engine();
    replay(&mut by_key, setup.clone());
    replay(&mut by_request, setup);

    let from_key = by_key.handle(pressed('2', Modifiers::SUPER | Modifiers::SHIFT));
    let from_request = by_request.handle(Event::ActionRequested(Action::MoveToTag(tag("2"))));

    assert_eq!(from_request, from_key);
    assert_eq!(by_request, by_key);
}

#[test]
fn an_action_request_needs_no_binding() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1), open(2)]);

    engine.handle(Event::ActionRequested(Action::ViewTag(tag("2"))));

    assert_eq!(engine.stacks().current().tag().as_str(), "2");
}

#[test]
fn an_action_request_can_ask_for_effects() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);

    assert_eq!(
        engine.handle(Event::ActionRequested(Action::Quit)),
        [Effect::Quit]
    );
    assert_eq!(
        engine.handle(Event::ActionRequested(Action::Close)),
        [Effect::Close(w(1))]
    );
}

#[test]
fn an_action_request_for_a_tag_nobody_has_changes_nothing() {
    let mut engine = engine();
    replay(&mut engine, [screen(1000, 600), open(1)]);
    let before = engine.clone();

    engine.handle(Event::ActionRequested(Action::ViewTag(tag("nope"))));

    assert_eq!(engine, before);
}

fn external_engine() -> Engine {
    let mut engine = engine();
    engine.handle(screen(1000, 600));
    engine.handle(Event::ActionRequested(Action::ToggleExternalLayout));
    engine
}

fn requests(effects: &[Effect]) -> Vec<(u64, Vec<WinId>, Rect)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::LayoutRequested {
                serial,
                windows,
                area,
                ..
            } => Some((*serial, windows.clone(), *area)),
            _ => None,
        })
        .collect()
}

fn column_answer(serial: u64, windows: &[WinId], area: Rect) -> Event {
    let width = area.width() / u32::try_from(windows.len()).expect("few windows");
    let placements = windows
        .iter()
        .zip(0u32..)
        .map(|(&id, n)| Placement {
            id,
            rect: Rect::new(
                area.x() + i32::try_from(n * width).expect("small"),
                area.y(),
                width,
                area.height(),
            )
            .expect("non-zero size"),
        })
        .collect();
    Event::LayoutProvided { serial, placements }
}

#[test]
fn an_external_workspace_asks_for_its_windows_and_shows_the_default_meanwhile() {
    let mut engine = external_engine();

    let first = requests(&engine.handle(open(1)));
    let second = requests(&engine.handle(open(2)));

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].1, [w(1)]);
    assert_eq!(second.len(), 1);
    // The newest window is the master, so it comes first.
    assert_eq!(second[0].1, [w(2), w(1)]);
    assert_ne!(first[0].0, second[0].0);

    let mut builtin = bound_engine();
    replay(&mut builtin, [screen(1000, 600), open(1), open(2)]);
    assert_eq!(scene(&engine), scene(&builtin));
}

#[test]
fn an_answer_that_fits_is_used_and_not_asked_again() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (serial, windows, area) = requests(&engine.handle(open(2))).remove(0);

    let effects = engine.handle(column_answer(serial, &windows, area));

    assert!(requests(&effects).is_empty());
    let scene = scene(&engine);
    assert_eq!(scene.get(w(2)).expect("shown").rect.x(), 0);
    assert_eq!(scene.get(w(2)).expect("shown").rect.width(), 500);
    assert_eq!(scene.get(w(1)).expect("shown").rect.x(), 500);
    assert!(requests(&engine.handle(Event::FocusRequested(w(1)))).is_empty());
}

#[test]
fn an_answer_to_an_old_request_is_ignored() {
    let mut engine = external_engine();
    let (old, windows, area) = requests(&engine.handle(open(1))).remove(0);
    let (new, new_windows, new_area) = requests(&engine.handle(open(2))).remove(0);

    engine.handle(column_answer(old, &windows, area));
    let stale = scene(&engine);
    assert_ne!(scene(&engine).get(w(1)).expect("shown").rect.width(), 1000);

    engine.handle(column_answer(new, &new_windows, new_area));
    assert_ne!(scene(&engine), stale);
}

#[test]
fn a_closing_window_makes_the_answer_stale_and_asks_again() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (serial, windows, area) = requests(&engine.handle(open(2))).remove(0);
    engine.handle(column_answer(serial, &windows, area));

    let again = requests(&engine.handle(Event::WindowClosed(w(2))));

    assert_eq!(again.len(), 1);
    assert_eq!(again[0].1, [w(1)]);
}

#[test]
fn a_wrong_answer_is_refused_and_the_same_question_is_not_repeated() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (serial, windows, area) = requests(&engine.handle(open(2))).remove(0);
    let outside = Rect::new(area.x() - 5, area.y(), area.width(), area.height()).expect("size");
    let bad = [
        Event::LayoutProvided {
            serial,
            placements: vec![Placement {
                id: w(1),
                rect: area,
            }],
        },
        Event::LayoutProvided {
            serial,
            placements: windows
                .iter()
                .map(|&id| Placement { id, rect: outside })
                .collect(),
        },
    ];
    for answer in bad {
        // The first answer consumes the request; ask again to test the second.
        let mut engine = engine.clone();
        let effects = engine.handle(answer);
        assert!(requests(&effects).is_empty());
        assert!(requests(&engine.handle(Event::FocusRequested(w(1)))).is_empty());
        let mut builtin = bound_engine();
        replay(&mut builtin, [screen(1000, 600), open(1), open(2)]);
        // The focus request above moved the focus; the places are what is compared.
        assert_eq!(scene(&engine).placements(), scene(&builtin).placements());
    }
}

#[test]
fn a_repeated_window_in_an_answer_is_refused() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (serial, _, area) = requests(&engine.handle(open(2))).remove(0);

    engine.handle(Event::LayoutProvided {
        serial,
        placements: vec![
            Placement {
                id: w(1),
                rect: area,
            },
            Placement {
                id: w(1),
                rect: area,
            },
        ],
    });

    let mut builtin = bound_engine();
    replay(&mut builtin, [screen(1000, 600), open(1), open(2)]);
    assert_eq!(scene(&engine), scene(&builtin));
}

#[test]
fn nothing_is_asked_for_an_empty_workspace_or_floating_windows() {
    let mut engine = external_engine();
    assert!(requests(&engine.handle(Event::ActionRequested(Action::FocusNext))).is_empty());

    engine.handle(open(1));
    let effects = engine.handle(Event::ActionRequested(Action::ToggleFloat));
    assert!(requests(&effects).is_empty());
}

#[test]
fn leaving_the_external_layout_stops_the_requests_and_uses_the_tile_layout() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (serial, windows, area) = requests(&engine.handle(open(2))).remove(0);
    engine.handle(column_answer(serial, &windows, area));

    let effects = engine.handle(Event::ActionRequested(Action::ToggleExternalLayout));
    assert!(requests(&effects).is_empty());

    let mut builtin = bound_engine();
    replay(&mut builtin, [screen(1000, 600), open(1), open(2)]);
    assert_eq!(scene(&engine), scene(&builtin));
}

#[test]
fn a_replayed_session_with_a_plugin_ends_in_the_same_state() {
    let mut live = external_engine();
    let mut log = vec![
        screen(1000, 600),
        Event::ActionRequested(Action::ToggleExternalLayout),
    ];
    for event in [open(1), open(2), open(3)] {
        log.push(event.clone());
        for (serial, windows, area) in requests(&live.handle(event)) {
            let answer = column_answer(serial, &windows, area);
            log.push(answer.clone());
            live.handle(answer);
        }
    }

    let mut rebuilt = engine();
    replay(&mut rebuilt, log);

    assert_eq!(rebuilt, live);
    assert_eq!(scene(&rebuilt), scene(&live));
}

proptest! {
    #[test]
    fn answering_every_request_never_breaks_the_scene(
        steps in proptest::collection::vec((0u8..6, 1u64..5), 0..40)
    ) {
        let mut engine = external_engine();
        for (kind, raw) in steps {
            let event = match kind {
                0 | 1 => open(raw),
                2 => Event::WindowClosed(w(raw)),
                3 => Event::ActionRequested(Action::ToggleExternalLayout),
                4 => Event::ActionRequested(Action::ViewTag(tag(&raw.to_string()))),
                _ => Event::ActionRequested(Action::ToggleFloat),
            };
            let mut effects = engine.handle(event);
            while let Some((serial, windows, area)) = requests(&effects).into_iter().next() {
                effects = engine.handle(column_answer(serial, &windows, area));
            }
            prop_assert!(engine.scene().is_ok());
        }
    }
}

#[test]
fn the_unanswered_requests_can_be_asked_for_again() {
    let mut engine = external_engine();
    engine.handle(open(1));
    let (old, _, _) = requests(&engine.handle(open(2))).remove(0);
    assert_eq!(
        requests(&engine.pending_layout_requests())
            .iter()
            .map(|(serial, windows, _)| (*serial, windows.clone()))
            .collect::<Vec<_>>(),
        [(old, vec![w(2), w(1)])]
    );

    let (serial, windows, area) = requests(&engine.pending_layout_requests()).remove(0);
    engine.handle(column_answer(serial, &windows, area));

    assert!(engine.pending_layout_requests().is_empty());
}

fn manage_engine() -> Engine {
    let mut engine = engine().with_manage_hook(true);
    engine.handle(screen(1000, 600));
    engine
}

fn manage_requests(effects: &[Effect]) -> Vec<(u64, WinId, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ManageRequested {
                serial, id, tag, ..
            } => Some((*serial, *id, tag.as_str().to_owned())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_new_window_is_offered_to_the_hook_after_the_rules_placed_it() {
    let mut engine = manage_engine();

    let first = manage_requests(&engine.handle(open(1)));
    let again = manage_requests(&engine.handle(open(1)));

    assert_eq!(first.len(), 1);
    assert_eq!((first[0].1, first[0].2.as_str()), (w(1), "1"));
    assert!(
        again.is_empty(),
        "a window already managed is not asked about twice"
    );
    assert_eq!(ids(&scene(&engine)), [1]);
}

#[test]
fn without_the_hook_nothing_is_asked() {
    let mut engine = engine();
    engine.handle(screen(1000, 600));

    assert!(manage_requests(&engine.handle(open(1))).is_empty());
}

#[test]
fn a_decision_moves_floats_and_focuses_the_window() {
    let mut engine = manage_engine();
    let (serial, _, _) = manage_requests(&engine.handle(open(1))).remove(0);
    engine.handle(open(2));

    engine.handle(Event::ManageDecided {
        serial,
        decision: ManageDecision {
            tag: Some(tag("2")),
            floating: Some(true),
            focus: false,
        },
    });

    assert_eq!(engine.stacks().tag_of(w(1)).map(Tag::as_str), Some("2"));
    assert!(
        engine
            .stacks()
            .workspaces()
            .any(|ws| ws.tag().as_str() == "2" && ws.is_floating(w(1)))
    );
    assert_eq!(
        ids(&scene(&engine)),
        [2],
        "the window left the shown workspace"
    );

    let (serial, _, _) = manage_requests(&engine.handle(open(3))).remove(0);
    engine.handle(Event::ManageDecided {
        serial,
        decision: ManageDecision {
            tag: Some(tag("2")),
            floating: None,
            focus: true,
        },
    });
    assert_eq!(engine.stacks().current().tag().as_str(), "2");
    assert_eq!(scene(&engine).focus(), Some(w(3)));
}

#[test]
fn a_decision_keeps_what_it_leaves_out() {
    let mut engine = manage_engine();
    let (serial, _, _) = manage_requests(&engine.handle(open(1))).remove(0);
    engine.handle(Event::ActionRequested(Action::ToggleFloat));
    let before = engine.clone();

    engine.handle(Event::ManageDecided {
        serial,
        decision: ManageDecision::default(),
    });

    // Only the answered request is gone from the engine's record of them.
    assert_eq!(engine.stacks(), before.stacks());
    assert!(engine.pending_manage_requests().is_empty());
}

#[test]
fn a_decision_that_does_not_apply_changes_nothing() {
    let mut engine = manage_engine();
    let (serial, _, _) = manage_requests(&engine.handle(open(1))).remove(0);
    let stacks = engine.stacks().clone();

    let unknown_tag = Event::ManageDecided {
        serial,
        decision: ManageDecision {
            tag: Some(tag("nope")),
            ..ManageDecision::default()
        },
    };
    engine.handle(unknown_tag.clone());
    assert_eq!(engine.stacks(), &stacks);

    // The request was consumed: answering again, or with an unknown serial, does nothing.
    engine.handle(unknown_tag);
    engine.handle(Event::ManageDecided {
        serial: 9999,
        decision: ManageDecision {
            tag: Some(tag("2")),
            ..ManageDecision::default()
        },
    });
    assert_eq!(engine.stacks(), &stacks);
}

#[test]
fn a_window_that_closed_cannot_be_decided_about() {
    let mut engine = manage_engine();
    let (serial, _, _) = manage_requests(&engine.handle(open(1))).remove(0);
    engine.handle(Event::WindowClosed(w(1)));
    assert!(engine.pending_manage_requests().is_empty());

    engine.handle(Event::ManageDecided {
        serial,
        decision: ManageDecision {
            tag: Some(tag("2")),
            ..ManageDecision::default()
        },
    });

    assert!(engine.stacks().workspaces().all(|ws| ws.is_empty()));
}

#[test]
fn unanswered_manage_requests_can_be_asked_for_again() {
    let mut engine = manage_engine();
    engine.handle(open(1));
    let (serial, _, _) = manage_requests(&engine.handle(open(2))).remove(0);

    let pending = manage_requests(&engine.pending_manage_requests());

    assert_eq!(pending.len(), 2);
    assert_eq!(pending[1].0, serial);
}

#[test]
fn a_replayed_session_with_manage_decisions_ends_in_the_same_state() {
    let mut live = manage_engine();
    let mut log = vec![screen(1000, 600)];
    for event in [open(1), open(2)] {
        log.push(event.clone());
        for (serial, id, _) in manage_requests(&live.handle(event)) {
            let decision = Event::ManageDecided {
                serial,
                decision: ManageDecision {
                    tag: Some(tag(if id == w(1) { "2" } else { "1" })),
                    floating: Some(id == w(2)),
                    focus: false,
                },
            };
            log.push(decision.clone());
            live.handle(decision);
        }
    }

    let mut rebuilt = engine().with_manage_hook(true);
    replay(&mut rebuilt, log);

    assert_eq!(rebuilt, live);
}
