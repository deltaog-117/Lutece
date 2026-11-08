//! Changing the keys and the manage hook of an engine that has windows.

use lutece::{
    Action, Effect, Engine, Event, KeyPress, Keymap, Keysym, Layout, Modifiers, Output, OutputId,
    Rect, StackSet, Tag, WinId, WindowInfo,
};
use proptest::prelude::*;

fn engine() -> Engine {
    let tags = ["1", "2"].map(|name| Tag::try_new(name).expect("test tags have text"));
    let mut engine = Engine::new(
        StackSet::new(tags).expect("distinct tags"),
        Layout::default(),
    );
    let rect = Rect::new(0, 0, 1000, 600).expect("non-zero size");
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect,
    }]));
    engine
}

fn key(c: char) -> KeyPress {
    KeyPress {
        keysym: Keysym::from_char(c).expect("test keys are printable"),
        modifiers: Modifiers::ALT,
    }
}

fn keymap(c: char, action: Action) -> Keymap {
    let mut keymap = Keymap::new();
    keymap.bind(key(c), action).expect("one binding fits");
    keymap
}

fn open(raw: u64) -> Event {
    Event::WindowOpened {
        id: WinId::new(raw),
        info: WindowInfo::default(),
    }
}

#[test]
fn the_new_keys_work_and_the_old_ones_stop() {
    let mut engine = engine().with_keymap(keymap('a', Action::Quit));
    engine.reconfigure(keymap('b', Action::Quit), false);

    assert!(engine.handle(Event::KeyPressed(key('a'))).is_empty());
    assert!(
        engine
            .handle(Event::KeyPressed(key('b')))
            .contains(&Effect::Quit)
    );
    assert_eq!(engine.grabs().collect::<Vec<_>>(), vec![key('b')]);
}

#[test]
fn turning_the_hook_on_offers_the_next_window_to_it() {
    let mut engine = engine();
    engine.reconfigure(Keymap::new(), true);
    let effects = engine.handle(open(1));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ManageRequested { .. }))
    );

    engine.reconfigure(Keymap::new(), false);
    let effects = engine.handle(open(2));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::ManageRequested { .. }))
    );
}

#[test]
fn a_request_left_open_when_the_hook_is_turned_off_is_still_pending() {
    let mut engine = engine();
    engine.reconfigure(Keymap::new(), true);
    engine.handle(open(1));
    engine.reconfigure(Keymap::new(), false);
    assert_eq!(engine.pending_manage_requests().len(), 1);
}

proptest! {
    /// Windows, focus and layout are state of the engine, not of the configuration, so a
    /// reload must never move them.
    #[test]
    fn a_reload_leaves_the_windows_where_they_were(
        count in 0u64..12,
        hook in any::<bool>(),
        letter in proptest::char::range('a', 'z'),
    ) {
        let mut engine = engine();
        for raw in 1..=count {
            engine.handle(open(raw));
        }
        let before = engine.scene().expect("the screen fits the windows");
        let stacks = engine.stacks().clone();

        engine.reconfigure(keymap(letter, Action::FocusNext), hook);

        prop_assert_eq!(engine.scene().expect("the screen fits the windows"), before);
        prop_assert_eq!(engine.stacks(), &stacks);
    }

    /// Reloading what is already running changes nothing at all.
    #[test]
    fn reloading_the_same_configuration_is_the_identity(
        count in 0u64..8,
        hook in any::<bool>(),
    ) {
        let map = keymap('a', Action::Close);
        let mut engine = engine().with_keymap(map.clone()).with_manage_hook(hook);
        for raw in 1..=count {
            engine.handle(open(raw));
        }
        let before = engine.clone();
        engine.reconfigure(map, hook);
        prop_assert_eq!(engine, before);
    }
}
