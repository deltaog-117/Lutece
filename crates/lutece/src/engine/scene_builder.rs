//! Deriving the scene from the engine's state.

use super::Engine;
use crate::shared::contracts::scene::{Placement, Scene};
use crate::shared::error::Result;
use crate::shared::ids::WinId;

impl Engine {
    /// The scene the backend should show: the workspace on each screen, laid out in the
    /// area of its output, with that workspace's floating windows above its tiles.
    /// Windows on hidden workspaces are left out, so they are hidden. The focus is the
    /// focused screen's focused window.
    ///
    /// Empty until the backend has reported an output.
    ///
    /// # Errors
    ///
    /// Returns the layout's error if an output is too small for the tiled windows on its
    /// workspace, or reaches past the coordinate range.
    pub fn scene(&self) -> Result<Scene> {
        let mut placements = Vec::new();
        for (output, workspace) in self.stacks.screens() {
            let Some(area) = self.area_of(output) else {
                continue;
            };
            let tiled: Vec<WinId> = workspace
                .windows()
                .filter(|&id| !workspace.is_floating(id))
                .collect();
            placements.extend(
                workspace
                    .layout()
                    .arrange(area, &tiled, workspace.focus())?,
            );
            for (id, float) in workspace.floats() {
                placements.push(Placement {
                    id,
                    rect: float.rect(area)?,
                });
            }
        }
        // A focused screen with no area has no windows in the scene to focus.
        let focus = self
            .area_of(self.stacks.current_output())
            .and(self.stacks.focused());
        Scene::try_new(placements, focus)
    }
}
