//! Properties plugins set on windows and workspaces: kept as plain data, bounded, and
//! identical after a replay.

use lutece::{
    Engine, Event, Layout, MAX_PROPERTIES_PER_TARGET, Output, OutputId, PropertyKey,
    PropertyTarget, PropertyValue, Rect, StackSet, Tag, WinId, WindowInfo,
};
use proptest::prelude::*;

fn tag(name: &str) -> Tag {
    Tag::try_new(name).expect("test tags have text")
}

fn engine_with_window() -> Engine {
    let mut engine = Engine::new(
        StackSet::new(["1", "2"].map(tag)).expect("two distinct tags"),
        Layout::default(),
    );
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect: Rect::new(0, 0, 1000, 600).expect("non-zero size"),
    }]));
    engine.handle(Event::WindowOpened {
        id: WinId::new(1),
        info: WindowInfo::default(),
    });
    engine
}

fn key(name: &str) -> PropertyKey {
    PropertyKey::try_new(name).expect("test keys are words")
}

fn value(text: &str) -> PropertyValue {
    PropertyValue::try_new(text).expect("test values are short")
}

fn set(target: PropertyTarget, name: &str, text: Option<&str>) -> Event {
    Event::PropertySet {
        target,
        key: key(name),
        value: text.map(value),
    }
}

fn window() -> PropertyTarget {
    PropertyTarget::Window(WinId::new(1))
}

fn read(engine: &Engine, target: &PropertyTarget) -> Vec<(String, String)> {
    engine
        .properties_of(target)
        .map(|(key, value)| (key.as_str().to_owned(), value.as_str().to_owned()))
        .collect()
}

#[test]
fn a_property_is_set_read_and_removed() {
    let mut engine = engine_with_window();

    engine.handle(set(window(), "bar.hidden", Some("yes")));
    assert_eq!(
        read(&engine, &window()),
        [("bar.hidden".to_owned(), "yes".to_owned())]
    );

    engine.handle(set(window(), "bar.hidden", Some("no")));
    assert_eq!(
        read(&engine, &window()),
        [("bar.hidden".to_owned(), "no".to_owned())]
    );

    engine.handle(set(window(), "bar.hidden", None));
    assert!(read(&engine, &window()).is_empty());
    assert_eq!(engine.properties().count(), 0);
}

#[test]
fn a_workspace_has_properties_too() {
    let mut engine = engine_with_window();
    let target = PropertyTarget::Workspace(tag("2"));

    engine.handle(set(target.clone(), "label", Some("mail")));

    assert_eq!(
        read(&engine, &target),
        [("label".to_owned(), "mail".to_owned())]
    );
}

#[test]
fn a_target_the_core_does_not_have_changes_nothing() {
    let mut engine = engine_with_window();
    let before = engine.clone();

    engine.handle(set(PropertyTarget::Window(WinId::new(99)), "a", Some("b")));
    engine.handle(set(PropertyTarget::Workspace(tag("nope")), "a", Some("b")));

    assert_eq!(engine, before);
}

#[test]
fn removing_what_is_not_there_changes_nothing() {
    let mut engine = engine_with_window();
    let before = engine.clone();

    engine.handle(set(window(), "absent", None));

    assert_eq!(engine, before);
}

#[test]
fn a_window_loses_its_properties_when_it_closes() {
    let mut engine = engine_with_window();
    engine.handle(set(window(), "a", Some("b")));

    engine.handle(Event::WindowClosed(WinId::new(1)));

    assert_eq!(engine.properties().count(), 0);
}

#[test]
fn a_target_stops_taking_new_keys_at_the_limit_but_keeps_changing_old_ones() {
    let mut engine = engine_with_window();
    for n in 0..MAX_PROPERTIES_PER_TARGET {
        engine.handle(set(window(), &format!("k{n}"), Some("v")));
    }

    engine.handle(set(window(), "one-too-many", Some("v")));
    engine.handle(set(window(), "k0", Some("changed")));

    let held = read(&engine, &window());
    assert_eq!(held.len(), MAX_PROPERTIES_PER_TARGET);
    assert!(held.iter().all(|(name, _)| name != "one-too-many"));
    assert!(held.contains(&("k0".to_owned(), "changed".to_owned())));
}

#[test]
fn a_property_is_an_event_so_a_replay_ends_the_same() {
    let events = [
        set(window(), "a", Some("1")),
        set(PropertyTarget::Workspace(tag("1")), "b", Some("2")),
        set(window(), "a", None),
    ];
    let (mut live, mut replayed) = (engine_with_window(), engine_with_window());

    for event in events.clone() {
        live.handle(event);
    }
    for event in events {
        replayed.handle(event);
    }

    assert_eq!(live, replayed);
}

proptest! {
    #[test]
    fn no_sequence_of_sets_goes_past_the_limit_or_leaves_an_empty_entry(
        steps in proptest::collection::vec(("[a-z]{1,2}", proptest::option::of("[a-z]{0,3}")), 0..200),
    ) {
        let mut engine = engine_with_window();
        for (name, text) in &steps {
            engine.handle(set(window(), name, text.as_deref()));
            prop_assert!(read(&engine, &window()).len() <= MAX_PROPERTIES_PER_TARGET);
        }
        let targets: std::collections::HashSet<_> =
            engine.properties().map(|(target, _, _)| target.clone()).collect();
        for target in targets {
            prop_assert!(!read(&engine, &target).is_empty());
        }
    }
}
