//! Workspaces and focus: which windows are on which workspace, which workspaces the
//! screens show, and which window is focused. Pure state, with no display and no
//! dependency on the other features.

mod outputs;
mod snapshot;
pub mod stackset;
pub mod workspace;
