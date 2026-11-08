//! Custom commands, the pointer entering a window, and a window's changed title.

use lutece::{
    Action, CommandName, Effect, Engine, Event, KeyPress, Keymap, Keysym, Layout, Modifiers,
    Output, OutputId, Rect, StackSet, Tag, WinId, WindowInfo,
};

fn tag(name: &str) -> Tag {
    Tag::try_new(name).expect("test tags have text")
}

fn engine() -> Engine {
    let mut engine = Engine::new(
        StackSet::new(["1"].map(tag)).expect("one tag"),
        Layout::default(),
    );
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect: Rect::new(0, 0, 1000, 600).expect("non-zero size"),
    }]));
    for raw in [1, 2] {
        engine.handle(Event::WindowOpened {
            id: WinId::new(raw),
            info: WindowInfo::default(),
        });
    }
    engine
}

fn command(name: &str) -> CommandName {
    CommandName::try_new(name).expect("test commands are words")
}

#[test]
fn a_custom_action_asks_for_the_command_and_changes_nothing_else() {
    let mut engine = engine();
    let before = engine.clone();

    let effects = engine.handle(Event::ActionRequested(Action::Custom(command("go"))));

    assert_eq!(effects, [Effect::CommandInvoked(command("go"))]);
    assert_eq!(engine, before);
}

#[test]
fn a_key_bound_to_a_custom_command_asks_for_it() {
    let press = KeyPress {
        keysym: Keysym::from_char('d').expect("d is printable"),
        modifiers: Modifiers::SUPER,
    };
    let mut keymap = Keymap::new();
    keymap
        .bind(press, Action::Custom(command("dev")))
        .expect("an empty keymap takes it");
    let mut engine = engine().with_keymap(keymap);

    assert_eq!(
        engine.handle(Event::KeyPressed(press)),
        [Effect::CommandInvoked(command("dev"))]
    );
}

#[test]
fn the_pointer_entering_a_window_focuses_it() {
    let mut engine = engine();
    assert_eq!(engine.stacks().focused(), Some(WinId::new(2)));

    engine.handle(Event::PointerEntered(WinId::new(1)));

    assert_eq!(engine.stacks().focused(), Some(WinId::new(1)));
}

#[test]
fn the_pointer_entering_an_unknown_window_changes_nothing() {
    let mut engine = engine();
    let before = engine.clone();

    engine.handle(Event::PointerEntered(WinId::new(42)));

    assert_eq!(engine, before);
}

#[test]
fn a_changed_title_is_accepted_and_changes_no_state() {
    let mut engine = engine();
    let before = engine.clone();

    let effects = engine.handle(Event::WindowInfoChanged {
        id: WinId::new(1),
        info: WindowInfo {
            title: "new".into(),
            class: "term".into(),
            transient_for: None,
        },
    });

    assert!(effects.is_empty());
    assert_eq!(engine, before);
}
