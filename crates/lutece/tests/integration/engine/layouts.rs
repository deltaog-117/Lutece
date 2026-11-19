//! Layouts by name: a layout from another crate arranges a workspace, a name nobody has falls
//! back instead of failing, and the name and the ratio survive a snapshot.

use lutece::{
    Action, Engine, Error, Event, Layout, LayoutName, LayoutSet, MasterRatio, Output, OutputId,
    Placement, Rect, Result, StackSet, Tag, WinId, WindowInfo,
};

/// Every window on a row of its own, from the top.
#[derive(Debug)]
struct Rows;

impl Layout for Rows {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        _focus: Option<WinId>,
        _ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        let height = area.height() / u32::try_from(windows.len().max(1)).unwrap_or(1);
        windows
            .iter()
            .enumerate()
            .map(|(row, &id)| {
                let top = i32::try_from(u32::try_from(row).unwrap_or(0) * height).unwrap_or(0);
                Ok(Placement {
                    id,
                    rect: Rect::new(area.x(), area.y() + top, area.width(), height.max(1))?,
                })
            })
            .collect()
    }
}

fn name(text: &str) -> LayoutName {
    LayoutName::try_new(text).expect("valid layout name")
}

fn engine_with(initial: &str, layouts: LayoutSet) -> Engine {
    let stacks = StackSet::new([Tag::try_new("1").expect("has text")]).expect("one tag");
    let mut engine = Engine::new(stacks, name(initial)).with_layouts(layouts);
    engine.handle(Event::OutputsChanged(vec![Output {
        id: OutputId::new(0),
        rect: Rect::new(0, 0, 1000, 600).expect("non-zero size"),
    }]));
    for raw in 1..=2 {
        engine.handle(Event::WindowOpened {
            id: WinId::new(raw),
            info: WindowInfo::default(),
        });
    }
    engine
}

fn rows() -> LayoutSet {
    lutece_layouts::register(LayoutSet::new())
        .and_then(|set| set.with(name("rows"), Box::new(Rows)))
        .expect("distinct names")
}

fn heights(engine: &Engine) -> Vec<u32> {
    engine
        .scene()
        .expect("fits")
        .placements()
        .iter()
        .map(|placement| placement.rect.height())
        .collect()
}

#[test]
fn a_layout_from_another_crate_arranges_the_workspace() {
    let engine = engine_with("rows", rows());
    assert_eq!(heights(&engine), [300, 300]);
    assert!(engine.missing_layouts().is_empty());
}

#[test]
fn the_cycle_reaches_a_registered_layout_after_the_built_in_ones() {
    let mut engine = engine_with("tile", rows());
    for _ in 0..4 {
        engine.handle(Event::ActionRequested(Action::NextLayout));
    }
    assert_eq!(engine.stacks().current().layout().as_str(), "rows");
    engine.handle(Event::ActionRequested(Action::NextLayout));
    assert_eq!(engine.stacks().current().layout().as_str(), "tile");
}

#[test]
fn a_name_nobody_registered_is_arranged_by_the_fallback_and_reported() {
    let engine = engine_with("spiral", lutece_layouts::builtin());
    let scene = engine.scene().expect("the fallback fits");
    let widths: Vec<_> = scene.placements().iter().map(|p| p.rect.width()).collect();
    assert_eq!(widths, [500, 500]);

    let missing = engine.missing_layouts();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].1.as_str(), "spiral");
}

#[test]
fn the_external_layout_is_not_a_missing_one() {
    let mut engine = engine_with("tile", lutece_layouts::builtin());
    engine.handle(Event::ActionRequested(Action::ToggleExternalLayout));
    assert!(engine.stacks().current().layout().is_external());
    assert!(engine.missing_layouts().is_empty());
}

#[test]
fn a_snapshot_keeps_the_name_and_the_ratio() {
    let mut engine = engine_with("rows", rows());
    engine.handle(Event::ActionRequested(Action::GrowMaster));
    let snapshot = engine.snapshot();
    let workspace = &snapshot.screens[0].workspace;
    assert_eq!(workspace.layout, "rows");
    // Rows has no master column, so growing it leaves the ratio where it was.
    assert_eq!(workspace.ratio, MasterRatio::default().permille());

    let template = engine_with("tile", rows());
    let restored = Engine::restore(&template, &snapshot).expect("the same layouts");
    assert_eq!(restored.stacks().current().layout().as_str(), "rows");
}

#[test]
fn a_snapshot_with_a_bad_layout_name_is_refused() {
    let engine = engine_with("tile", lutece_layouts::builtin());
    let mut snapshot = engine.snapshot();
    snapshot.screens[0].workspace.layout = "Not A Name".to_owned();
    let template = engine_with("tile", lutece_layouts::builtin());
    assert!(matches!(
        Engine::restore(&template, &snapshot),
        Err(Error::InvalidSnapshot(_))
    ));
}
