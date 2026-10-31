//! Deriving the scene from the engine's state.

use super::Engine;
use crate::shared::contracts::scene::{Placement, Scene};
use crate::shared::error::Result;
use crate::shared::ids::WinId;

impl Engine {
    /// The scene the backend should show: the current workspace's windows, laid out on
    /// the screen, with its floating windows above them. Windows on other workspaces are
    /// left out, so they are hidden.
    ///
    /// Empty until the backend has reported a screen.
    ///
    /// # Errors
    ///
    /// Returns the layout's error if the screen is too small for the tiled windows on the
    /// current workspace, or reaches past the coordinate range.
    pub fn scene(&self) -> Result<Scene> {
        let Some(area) = self.screen else {
            return Ok(Scene::empty());
        };
        let workspace = self.stacks.current();
        let tiled: Vec<WinId> = workspace
            .windows()
            .filter(|&id| !workspace.is_floating(id))
            .collect();
        let mut placements = workspace
            .layout()
            .arrange(area, &tiled, workspace.focus())?;
        for (id, float) in workspace.floats() {
            placements.push(Placement {
                id,
                rect: float.rect(area)?,
            });
        }
        Scene::try_new(placements, workspace.focus())
    }
}
