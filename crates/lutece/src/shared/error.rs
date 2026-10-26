//! The single error type returned by every fallible constructor in the crate.

use crate::shared::ids::WinId;
use thiserror::Error;

/// Reasons a value was rejected at construction.
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
    /// A scene lists each window at most once.
    #[error("window {0:?} appears more than once in the scene")]
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
}

/// Result alias using the crate [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
