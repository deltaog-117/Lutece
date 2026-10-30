//! What a key press can make the window manager do.

use crate::shared::contracts::effect::SpawnCommand;
use crate::shared::ids::Tag;

/// An operation a key binding triggers.
///
/// A closed enum, like [`Layout`](crate::Layout): a binding is a value that can be
/// compared and stored, and the compiler flags every `match` that misses a new action.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Action {
    /// Stop the window manager.
    Quit,
    /// Ask the focused window to close. Does nothing on an empty workspace.
    Close,
    /// Start a program.
    Spawn(SpawnCommand),
    /// Focus the next window on the current workspace, wrapping around.
    FocusNext,
    /// Focus the previous window on the current workspace, wrapping around.
    FocusPrev,
    /// Swap the focused window with the next one; the focus follows it.
    SwapNext,
    /// Swap the focused window with the previous one; the focus follows it.
    SwapPrev,
    /// Switch the current workspace to its next layout, wrapping around.
    NextLayout,
    /// Widen the current workspace's master column. Does nothing in layouts without one.
    GrowMaster,
    /// Narrow the current workspace's master column. Does nothing in layouts without one.
    ShrinkMaster,
    /// Show the workspace with this tag. Does nothing if there is none.
    ViewTag(Tag),
    /// Send the focused window to the workspace with this tag. Does nothing if there is
    /// none.
    MoveToTag(Tag),
}
