//! Deriving the scene from the engine's state.

use super::Engine;
use crate::shared::contracts::scene::Scene;
use crate::shared::error::Result;
use crate::shared::ids::WinId;

impl Engine {
    /// The scene the backend should show: the current workspace's windows, laid out on
    /// the screen. Windows on other workspaces are left out, so they are hidden.
    ///
    /// Empty until the backend has reported a screen.
    ///
    /// # Errors
    ///
    /// Returns the layout's error if the screen is too small for the windows on the
    /// current workspace, or reaches past the coordinate range.
    pub fn scene(&self) -> Result<Scene> {
        let Some(area) = self.screen else {
            return Ok(Scene::empty());
        };
        let workspace = self.stacks.current();
        let windows: Vec<WinId> = workspace.windows().collect();
        let placements = self.layout.arrange(area, &windows, workspace.focus())?;
        Scene::try_new(placements, workspace.focus())
    }
}
