//! The single error type returned by every fallible operation in the crate.

use crate::shared::geometry::Rect;
use crate::shared::ids::{Tag, WinId};
use thiserror::Error;

/// Reasons a value was rejected or an operation refused.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A window cannot have a zero width or height.
    #[error("size must be non-zero, got {width}x{height}")]
    InvalidSize {
        /// The rejected width.
        width: u32,
        /// The rejected height.
        height: u32,
    },
    /// A scene or workspace set lists each window at most once.
    #[error("window {0:?} is already present")]
    DuplicateWindow(WinId),
    /// Focus must point at a window the scene shows.
    #[error("focused window {0:?} is not in the scene")]
    FocusedWindowMissing(WinId),
    /// A spawn effect needs a program to run.
    #[error("spawn command has an empty program name")]
    EmptyProgram,
    /// Modifier bits outside the defined set.
    #[error("unknown modifier bits {0:#010b}")]
    UnknownModifierBits(u8),
    /// A workspace tag needs visible text.
    #[error("workspace tag must not be empty")]
    EmptyTag,
    /// Two workspaces cannot share a tag.
    #[error("workspace tag {0} is used more than once")]
    DuplicateTag(Tag),
    /// A window manager needs at least one workspace.
    #[error("at least one workspace is required")]
    NoWorkspaces,
    /// No workspace has this tag.
    #[error("no workspace is tagged {0}")]
    UnknownTag(Tag),
    /// No workspace holds this window.
    #[error("window {0:?} is not managed")]
    UnknownWindow(WinId),
    /// A master ratio must leave room for both the master and the rest.
    #[error("master ratio must be 1..=999 per mille, got {0}")]
    InvalidRatio(u16),
    /// Every window needs at least one pixel in each direction.
    #[error("a {width}x{height} area cannot hold {windows} tiled windows")]
    AreaTooSmall {
        /// How many windows were to be placed.
        windows: usize,
        /// Width of the area.
        width: u32,
        /// Height of the area.
        height: u32,
    },
    /// The area's far edges do not fit the coordinate range.
    #[error("area {0:?} extends beyond the coordinate range")]
    AreaOutOfRange(Rect),
}

/// Result alias using the crate [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
