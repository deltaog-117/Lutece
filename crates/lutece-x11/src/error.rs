//! The single error type returned by every fallible operation in the backend.

use lutece::WinId;
use std::io;
use thiserror::Error;
use x11rb::errors::{ConnectError, ConnectionError, ReplyError};

/// Reasons the backend could not talk to the X server or carry out a request.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No X server could be reached, for example `DISPLAY` is unset.
    #[error("cannot connect to the X server: {0}")]
    Connect(#[from] ConnectError),
    /// The connection to the X server broke.
    #[error("lost the connection to the X server: {0}")]
    Connection(#[from] ConnectionError),
    /// The X server refused a request we waited for an answer to.
    #[error("the X server refused a request: {0}")]
    Reply(#[from] ReplyError),
    /// Only one client at a time may redirect window management on the root window.
    #[error("another window manager is already running on this display")]
    AnotherWindowManager,
    /// A spawn effect named a program that could not be started.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// The program that failed to start.
        program: String,
        /// Why it failed.
        #[source]
        source: io::Error,
    },
    /// A scene named a window this backend never reported, so it is not an X window id.
    #[error("window {0:?} was not reported by this backend")]
    ForeignWindow(WinId),
    /// The core rejected a value built from what the X server reported.
    #[error(transparent)]
    Core(#[from] lutece::Error),
}

/// Result alias using the backend [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
