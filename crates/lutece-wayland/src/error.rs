//! The single error type returned by every fallible operation in the backend.

use smithay::reexports::wayland_server::{BindError, backend::InitError};
use std::io;
use thiserror::Error;

/// Reasons the backend could not serve clients or carry out a request.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The Wayland display could not be created.
    #[error("cannot create the Wayland display: {0}")]
    Display(#[from] InitError),
    /// No socket could be bound for clients to connect to, for example
    /// `XDG_RUNTIME_DIR` is unset.
    #[error("cannot bind a Wayland socket: {0}")]
    Bind(#[from] BindError),
    /// The window to show the compositor in could not be opened.
    #[error("cannot open the window: {0}")]
    Window(#[from] smithay::backend::winit::Error),
    /// Drawing a frame failed.
    #[error("cannot draw a frame: {0}")]
    Render(String),
    /// The keyboard layout could not be loaded.
    #[error("cannot load the keyboard layout: {0}")]
    Keyboard(smithay::input::keyboard::Error),
    /// Talking to a client, or waiting for one, failed.
    #[error("I/O error while serving clients: {0}")]
    Io(#[from] io::Error),
    /// A spawn effect named a program that could not be started.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// The program that failed to start.
        program: String,
        /// Why it failed.
        #[source]
        source: io::Error,
    },
    /// The core rejected a value built from what the backend reported.
    #[error(transparent)]
    Core(#[from] lutece::Error),
}

/// Result alias using the backend [`enum@Error`].
pub type Result<T> = std::result::Result<T, Error>;
