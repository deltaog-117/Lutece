//! A snapshot of an engine brings back an equal engine, and a snapshot that was damaged or
//! invented is refused.

use super::replay::{arbitrary_event, keymap};
use lutece::snapshot::{LayoutState, VERSION};
use lutece::{
    Action, Engine, Error, Event, Layout, ManageDecision, Output, OutputId, PropertyKey,
    PropertyTarget, PropertyValue, Rect, StackSet, Tag, WinId, WindowInfo,
};
use proptest::prelude::*;

fn tag(name: &str) -> Tag {
    Tag::try_new(name).expect("test tags have text")
}

/// Windows 0 and 1 start on other workspaces, so the state has more than one workspace
/// with windows.
fn template() -> Engine {
    let mut stacks = StackSet::new(["1", "2", "3"].map(tag)).expect("three distinct tags");
    stacks.focus_tag(&tag("2")).expect("workspace exists");
    stacks.insert(WinId::new(0)).expect("new window");
    stacks.focus_tag(&tag("3")).expect("workspace exists");
    stacks.insert(WinId::new(1)).expect("new window");
    stacks.focus_tag(&tag("1")).expect("workspace exists");
    Engine::new(stacks, Layout::default())
        .with_keymap(keymap())
        .with_manage_hook(true)
}

fn property_event() -> impl Strategy<Value = Event> {
    (
        prop_oneof![
            (0u64..8).prop_map(|n| PropertyTarget::Window(WinId::new(n))),
            prop::sample::select(vec!["1", "2", "3", "9"])
                .prop_map(|name| PropertyTarget::Workspace(tag(name))),
        ],
        "[a-c]{1,2}",
        prop::option::of("[a-z ]{0,6}"),
    )
        .prop_map(|(target, key, value)| Event::PropertySet {
            target,
            key: PropertyKey::try_new(key).expect("short visible key"),
            value: value.map(|text| PropertyValue::try_new(text).expect("short value")),
        })
}

fn hook_event() -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::ActionRequested(Action::ToggleExternalLayout)),
        Just(Event::ActionRequested(Action::NextLayout)),
        (0u64..6).prop_map(|serial| Event::ManageDecided {
            serial,
            decision: ManageDecision::default(),
        }),
        (0u64..6).prop_map(|serial| Event::LayoutProvided {
            serial,
            placements: Vec::new(),
        }),
    ]
}

proptest! {
    /// Whatever the engine went through, restoring its snapshot onto the configuration it
    /// started from gives an engine equal to it, and taking the snapshot again changes
    /// nothing.
    #[test]
    fn a_snapshot_restores_an_equal_engine(
        events in prop::collection::vec(
            prop_oneof![
                6 => arbitrary_event(),
                2 => property_event(),
                2 => hook_event(),
            ],
            0..80,
        ),
    ) {
        let template = template();
        let mut engine = template.clone();
        for event in events {
            engine.handle(event);
        }

        let snapshot = engine.snapshot();
        let restored = Engine::restore(&template, &snapshot).expect("a real snapshot restores");

        prop_assert_eq!(&restored, &engine);
        prop_assert_eq!(restored.snapshot(), snapshot);
    }
}

/// An engine with windows on two workspaces, a style, properties and a floating window,
/// which the damage tests start from.
fn busy() -> Engine {
    let mut engine = template();
    let screen = Rect::new(0, 0, 1000, 600).expect("non-zero size");
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect: screen,
    }]));
    for raw in [10, 11, 12] {
        engine.handle(Event::WindowOpened {
            id: WinId::new(raw),
            info: WindowInfo::default(),
        });
    }
    engine.handle(Event::StyleChanged {
        id: WinId::new(10),
        style: lutece::Style::default().with_radius(6),
    });
    engine.handle(Event::PropertySet {
        target: PropertyTarget::Window(WinId::new(10)),
        key: PropertyKey::try_new("label").expect("key"),
        value: Some(PropertyValue::try_new("main").expect("value")),
    });
    engine.handle(Event::FullscreenRequested {
        id: WinId::new(12),
        on: true,
    });
    engine
}

fn refused(change: impl FnOnce(&mut lutece::snapshot::Snapshot)) {
    let mut snapshot = busy().snapshot();
    change(&mut snapshot);
    let result = Engine::restore(&template(), &snapshot);
    assert!(
        matches!(result, Err(Error::InvalidSnapshot(_))),
        "a damaged snapshot was accepted: {result:?}"
    );
}

#[test]
fn the_untouched_snapshot_restores() {
    let engine = busy();
    let restored = Engine::restore(&template(), &engine.snapshot()).expect("restores");
    assert_eq!(restored, engine);
}

#[test]
fn another_version_is_refused() {
    refused(|snapshot| snapshot.version = VERSION + 1);
}

#[test]
fn a_window_on_two_workspaces_is_refused() {
    refused(|snapshot| {
        let stolen = snapshot.screens[0]
            .workspace
            .stack
            .as_ref()
            .expect("the first screen has windows")
            .focus;
        let other = snapshot
            .hidden
            .iter_mut()
            .find_map(|workspace| workspace.stack.as_mut())
            .expect("a hidden workspace has windows");
        other.down.push(stolen);
    });
}

#[test]
fn a_window_listed_twice_in_a_stack_is_refused() {
    refused(|snapshot| {
        let stack = snapshot.screens[0]
            .workspace
            .stack
            .as_mut()
            .expect("windows");
        stack.down.push(stack.focus);
    });
}

#[test]
fn a_fullscreen_window_that_is_not_focused_is_refused() {
    refused(|snapshot| {
        let workspace = &mut snapshot.screens[0].workspace;
        let stack = workspace.stack.as_mut().expect("windows");
        workspace.fullscreen = stack.up.first().copied().or(stack.down.first().copied());
    });
}

#[test]
fn a_floating_window_that_is_not_in_the_stack_is_refused() {
    refused(|snapshot| {
        snapshot.screens[0]
            .workspace
            .floating
            .push(lutece::snapshot::FloatState {
                window: 999,
                dx: 0,
                dy: 0,
                dw: 0,
                dh: 0,
            });
    });
}

#[test]
fn a_workspace_the_configuration_does_not_have_is_refused() {
    refused(|snapshot| snapshot.screens[0].workspace.tag = "nope".into());
}

#[test]
fn a_workspace_at_the_wrong_rank_is_refused() {
    refused(|snapshot| snapshot.screens[0].workspace.rank += 1);
}

#[test]
fn a_missing_workspace_is_refused() {
    refused(|snapshot| {
        snapshot.hidden.pop();
    });
}

#[test]
fn a_ratio_the_layout_cannot_have_is_refused() {
    refused(|snapshot| snapshot.screens[0].workspace.layout = LayoutState::Tile(0));
    refused(|snapshot| snapshot.screens[0].workspace.tile_ratio = 1000);
}

#[test]
fn a_focused_screen_that_does_not_exist_is_refused() {
    refused(|snapshot| snapshot.current = snapshot.screens.len());
    refused(|snapshot| {
        snapshot.screens.clear();
        snapshot.current = 0;
    });
}

#[test]
fn two_screens_on_one_output_are_refused() {
    refused(|snapshot| {
        let screen = snapshot.screens[0].clone();
        snapshot.screens.push(screen);
    });
}

#[test]
fn a_style_for_a_window_that_is_not_managed_is_refused() {
    refused(|snapshot| {
        let mut entry = snapshot.styles[0];
        entry.window = 4242;
        snapshot.styles.push(entry);
    });
}

#[test]
fn a_default_style_that_the_engine_never_stores_is_refused() {
    refused(|snapshot| {
        let plain = lutece::snapshot::StyleEntry {
            window: snapshot.styles[0].window,
            style: busy_plain_style(),
        };
        snapshot.styles[0] = plain;
    });
}

fn busy_plain_style() -> lutece::snapshot::StyleState {
    let mut engine = template();
    engine.handle(Event::StyleChanged {
        id: WinId::new(0),
        style: lutece::Style::default().with_radius(1),
    });
    let mut state = engine.snapshot().styles[0].style;
    state.radius = 0;
    state
}

#[test]
fn properties_of_something_that_is_not_there_are_refused() {
    refused(|snapshot| {
        snapshot.properties[0].target = lutece::snapshot::TargetState::Window(4242);
    });
    refused(|snapshot| {
        snapshot.properties[0].entries.clear();
    });
    refused(|snapshot| {
        snapshot.properties[0].entries[0].key = String::new();
    });
}

#[test]
fn a_rectangle_with_no_area_is_refused() {
    refused(|snapshot| snapshot.outputs[0].rect.width = 0);
}
