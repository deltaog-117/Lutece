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
    /// Move the focus to the next output's screen, wrapping around. Does nothing with one
    /// screen.
    FocusNextOutput,
    /// Move the focus to the previous output's screen, wrapping around.
    FocusPrevOutput,
    /// Send the focused window to the workspace the next output's screen shows; the focus
    /// stays on this screen. Does nothing with one screen.
    SendToNextOutput,
    /// Send the focused window to the workspace the previous output's screen shows.
    SendToPrevOutput,
    /// Float the focused window if it is tiled, or tile it again if it floats.
    ToggleFloat,
    /// Make the focused window fill its whole screen, or put it back if it already does.
    ToggleFullscreen,
    /// Move the focused window a step this way. Does nothing unless it floats.
    MoveFloat(Direction),
    /// Resize the focused window by a step: right and down widen and heighten it, left
    /// and up narrow and shorten it. Does nothing unless it floats.
    ResizeFloat(Direction),
}

/// One of the four ways a floating window can be nudged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Direction {
    /// Towards the left edge.
    Left,
    /// Towards the right edge.
    Right,
    /// Towards the top edge.
    Up,
    /// Towards the bottom edge.
    Down,
}

impl Direction {
    /// The horizontal and vertical change of a step of `size` pixels; screen
    /// coordinates grow to the right and downwards.
    pub(crate) const fn delta(self, size: i32) -> (i32, i32) {
        match self {
            Self::Left => (-size, 0),
            Self::Right => (size, 0),
            Self::Up => (0, -size),
            Self::Down => (0, size),
        }
    }
}
