//! The built-in layouts of lutece, as plain implementations of [`lutece::Layout`].
//!
//! The core knows one trivial layout of its own and nothing else; this crate is what gives a
//! window manager its usual ones. It is also the model for a layout of your own: build a
//! crate that implements [`Layout`], register it next to these with [`LayoutSet::register`],
//! and a workspace can switch to it by name.

mod centered;
mod grid;
mod monocle;
mod span;
mod tile;

use lutece::{Layout, LayoutName, LayoutSet, MasterRatio, Placement, Rect, Result, WinId};

/// Names of the built-in layouts, in the order the next-layout action cycles through them.
pub const NAMES: [&str; 4] = ["tile", "monocle", "grid", "centered"];

/// The first window on the left, the rest stacked on the right.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tile;

/// Every window full size, the focused one on top.
#[derive(Debug, Clone, Copy, Default)]
pub struct Monocle;

/// Rows of equal height, as square as the window count allows.
#[derive(Debug, Clone, Copy, Default)]
pub struct Grid;

/// The first window in a middle column, the rest alternating to its right and left.
#[derive(Debug, Clone, Copy, Default)]
pub struct CenteredMaster;

impl Layout for Tile {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        _focus: Option<WinId>,
        ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        tile::arrange(ratio, area, windows)
    }

    fn uses_master_ratio(&self) -> bool {
        true
    }
}

impl Layout for Monocle {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        focus: Option<WinId>,
        _ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        Ok(monocle::arrange(area, windows, focus))
    }
}

impl Layout for Grid {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        _focus: Option<WinId>,
        _ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        grid::arrange(area, windows)
    }
}

impl Layout for CenteredMaster {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        _focus: Option<WinId>,
        ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        centered::arrange(ratio, area, windows)
    }

    fn uses_master_ratio(&self) -> bool {
        true
    }
}

/// A [`LayoutSet`] with the four built-in layouts registered, as [`NAMES`] lists them.
///
/// # Examples
///
/// ```
/// let layouts = lutece_layouts::builtin();
/// assert_eq!(layouts.first().as_str(), "tile");
/// ```
#[must_use]
pub fn builtin() -> LayoutSet {
    register(LayoutSet::new()).expect("a new set holds none of the built-in names")
}

/// Registers the built-in layouts on `set` and returns it.
///
/// # Errors
///
/// Returns the set's error if it already holds a layout named like one of [`NAMES`].
pub fn register(set: LayoutSet) -> Result<LayoutSet> {
    let layouts: [(&str, Box<dyn Layout>); 4] = [
        ("tile", Box::new(Tile)),
        ("monocle", Box::new(Monocle)),
        ("grid", Box::new(Grid)),
        ("centered", Box::new(CenteredMaster)),
    ];
    layouts.into_iter().try_fold(set, |set, (name, layout)| {
        set.with(LayoutName::try_new(name)?, layout)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_lists_the_names_in_order() {
        let names: Vec<_> = builtin()
            .names()
            .into_iter()
            .map(|n| n.as_str().to_owned())
            .collect();
        assert_eq!(names, NAMES);
    }

    #[test]
    fn registering_twice_is_refused_by_the_set() {
        let set = builtin();
        let again = LayoutName::try_new("tile").expect("valid");
        assert!(set.with(again, Box::new(Tile)).is_err());
    }
}
