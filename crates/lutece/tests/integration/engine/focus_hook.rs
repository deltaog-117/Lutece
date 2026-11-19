//! The focus hook: when the focused window of a workspace closes, the engine focuses its own
//! choice at once and asks a plugin, whose answer moves the focus only if nothing else has.

use lutece::{
    Action, Effect, Engine, Event, Output, OutputId, Rect, StackSet, Tag, WinId, WindowInfo,
};
use proptest::prelude::*;

fn w(raw: u64) -> WinId {
    WinId::new(raw)
}

fn tag(name: &str) -> Tag {
    Tag::try_new(name).expect("test tags have text")
}

fn open(raw: u64) -> Event {
    Event::WindowOpened {
        id: w(raw),
        info: WindowInfo::default(),
    }
}

/// An engine with the hook on, one screen, and windows 1, 2 and 3 on workspace "1", where
/// 3 has the focus.
fn engine_with_three() -> Engine {
    let stacks = StackSet::new(["1", "2"].map(tag)).expect("two distinct tags");
    let mut engine = Engine::new(stacks, lutece_layouts::builtin().first())
        .with_layouts(lutece_layouts::builtin())
        .with_focus_hook(true);
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect: Rect::new(0, 0, 1000, 600).expect("non-zero size"),
    }]));
    for raw in 1..=3 {
        engine.handle(open(raw));
    }
    engine
}

fn focus_of(engine: &Engine, name: &str) -> Option<u64> {
    engine
        .stacks()
        .workspaces()
        .find(|workspace| *workspace.tag() == tag(name))
        .and_then(|workspace| workspace.focus())
        .map(WinId::get)
}

/// The serial of the focus request among `effects`, if there is one.
fn asked(effects: &[Effect]) -> Option<u64> {
    effects.iter().find_map(|effect| match effect {
        Effect::FocusDecisionRequested { serial, .. } => Some(*serial),
        _ => None,
    })
}

fn decided(serial: u64, raw: u64) -> Event {
    Event::FocusDecided { serial, id: w(raw) }
}

#[test]
fn closing_the_focused_window_focuses_the_engines_choice_and_asks() {
    let mut engine = engine_with_three();
    // Opening 3 left the stack 3, 2, 1: the engine's rule is the next window, 2.
    let effects = engine.handle(Event::WindowClosed(w(3)));

    assert_eq!(focus_of(&engine, "1"), Some(2), "the engine does not wait");
    let [
        Effect::FocusDecisionRequested {
            tag: asked_tag,
            closed,
            windows,
            focus,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("one focus request expected, got {effects:?}");
    };
    assert_eq!(*asked_tag, tag("1"));
    assert_eq!(*closed, w(3));
    assert_eq!(*focus, w(2));
    let mut listed: Vec<u64> = windows.iter().map(|id| id.get()).collect();
    listed.sort_unstable();
    assert_eq!(listed, vec![1, 2]);
}

#[test]
fn there_is_no_question_when_there_is_no_choice() {
    let mut hooked = engine_with_three();
    // The focused one is not the one that closes.
    assert_eq!(asked(&hooked.handle(Event::WindowClosed(w(1)))), None);
    // Closing 3 leaves only window 2: nothing to choose between.
    assert_eq!(asked(&hooked.handle(Event::WindowClosed(w(3)))), None);
    // And the last window leaves none.
    assert_eq!(asked(&hooked.handle(Event::WindowClosed(w(2)))), None);
    assert!(hooked.pending_focus_requests().is_empty());
}

#[test]
fn the_hook_is_off_unless_asked_for() {
    let mut engine = engine_with_three();
    engine.set_focus_hook(false);
    assert_eq!(asked(&engine.handle(Event::WindowClosed(w(3)))), None);
    assert_eq!(focus_of(&engine, "1"), Some(2));
}

#[test]
fn an_answer_moves_the_focus() {
    let mut engine = engine_with_three();
    let serial = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");

    engine.handle(decided(serial, 1));

    assert_eq!(focus_of(&engine, "1"), Some(1));
    assert!(engine.pending_focus_requests().is_empty());
}

#[test]
fn an_answer_is_used_once() {
    let mut engine = engine_with_three();
    let serial = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");
    engine.handle(decided(serial, 1));
    engine.handle(Event::FocusRequested(w(2)));

    engine.handle(decided(serial, 1));

    assert_eq!(focus_of(&engine, "1"), Some(2));
}

#[test]
fn an_answer_is_ignored_when_the_focus_moved_meanwhile() {
    let mut engine = engine_with_three();
    let serial = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");
    // The user focused window 1 before the plugin answered.
    engine.handle(Event::FocusRequested(w(1)));
    assert!(engine.pending_focus_requests().is_empty());

    engine.handle(decided(serial, 2));

    assert_eq!(focus_of(&engine, "1"), Some(1));
}

#[test]
fn an_answer_that_names_a_window_of_another_workspace_or_none_is_ignored() {
    let mut engine = engine_with_three();
    engine.handle(Event::ActionRequested(Action::ViewTag(tag("2"))));
    engine.handle(open(9));
    engine.handle(Event::ActionRequested(Action::ViewTag(tag("1"))));
    let serial = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");

    engine.handle(decided(serial, 9));
    assert_eq!(focus_of(&engine, "1"), Some(2));
    assert_eq!(focus_of(&engine, "2"), Some(9));

    // The refused answer used the request up, so a right one now is late.
    engine.handle(decided(serial, 1));
    assert_eq!(focus_of(&engine, "1"), Some(2));
    engine.handle(decided(serial.wrapping_add(100), 1));
    assert_eq!(focus_of(&engine, "1"), Some(2));
}

#[test]
fn an_answer_for_a_hidden_workspace_does_not_change_what_is_shown() {
    let mut engine = engine_with_three();
    // Workspace "1" is hidden while its focused window closes.
    engine.handle(Event::ActionRequested(Action::ViewTag(tag("2"))));
    let closing = engine.handle(Event::WindowClosed(w(3)));
    let serial = asked(&closing).expect("a hidden workspace is asked about too");

    engine.handle(decided(serial, 1));

    assert_eq!(focus_of(&engine, "1"), Some(1));
    assert_eq!(*engine.stacks().current().tag(), tag("2"));
}

#[test]
fn a_new_close_replaces_the_older_question_of_its_workspace() {
    let mut engine = engine_with_three();
    engine.handle(open(4));
    let first = asked(&engine.handle(Event::WindowClosed(w(4)))).expect("asked");
    // The first answer was never given, and the focus (window 3) closes too.
    let second = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");
    assert_ne!(first, second);

    assert_eq!(engine.pending_focus_requests().len(), 1);
    engine.handle(decided(first, 1));
    assert_eq!(
        focus_of(&engine, "1"),
        Some(2),
        "the old serial is not the open one"
    );
}

#[test]
fn a_late_plugin_is_given_the_questions_still_open_in_the_order_asked() {
    let mut engine = engine_with_three();
    engine.handle(Event::ActionRequested(Action::ViewTag(tag("2"))));
    for raw in 5..=7 {
        engine.handle(open(raw));
    }
    let on_two = asked(&engine.handle(Event::WindowClosed(w(7)))).expect("asked");
    engine.handle(Event::ActionRequested(Action::ViewTag(tag("1"))));
    let on_one = asked(&engine.handle(Event::WindowClosed(w(3)))).expect("asked");

    let serials: Vec<u64> = engine
        .pending_focus_requests()
        .iter()
        .filter_map(|effect| asked(std::slice::from_ref(effect)))
        .collect();

    assert_eq!(serials, vec![on_two, on_one]);
}

fn hook_event() -> impl Strategy<Value = Event> {
    prop_oneof![
        4 => (0u64..8).prop_map(open),
        4 => (0u64..8).prop_map(|n| Event::WindowClosed(w(n))),
        2 => (0u64..8).prop_map(|n| Event::FocusRequested(w(n))),
        2 => (0u64..12, 0u64..8).prop_map(|(serial, n)| decided(serial, n)),
        1 => prop::sample::select(vec!["1", "2"])
            .prop_map(|name| Event::ActionRequested(Action::ViewTag(tag(name)))),
    ]
}

proptest! {
    /// Whatever is asked and answered, an answer never changes which workspace is shown,
    /// never puts a focus outside its workspace, and equal events give equal engines.
    #[test]
    fn answers_only_move_the_focus_inside_a_workspace(
        events in prop::collection::vec(hook_event(), 0..80),
    ) {
        let mut engine = engine_with_three();
        let mut twin = engine_with_three();
        for event in events {
            let shown = engine.stacks().current().tag().clone();
            let answering = matches!(event, Event::FocusDecided { .. });
            engine.handle(event.clone());
            twin.handle(event);
            if answering {
                prop_assert_eq!(engine.stacks().current().tag(), &shown);
            }
            for workspace in engine.stacks().workspaces() {
                if let Some(focus) = workspace.focus() {
                    prop_assert!(workspace.contains(focus));
                }
            }
            prop_assert_eq!(&engine, &twin);
        }
    }

    /// The questions kept for a late plugin are bounded by the workspaces, however many
    /// windows close and however few answers come.
    #[test]
    fn no_more_questions_stay_open_than_there_are_workspaces(
        events in prop::collection::vec(hook_event(), 0..120),
    ) {
        let mut engine = engine_with_three();
        for event in events {
            engine.handle(event);
            prop_assert!(engine.pending_focus_requests().len() <= 2);
        }
    }
}
