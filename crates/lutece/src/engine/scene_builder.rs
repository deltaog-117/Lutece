//! Deriving the scene from the engine's state.

use super::Engine;
use super::external::Key;
use crate::layout::Layout;
use crate::shared::contracts::scene::{Placement, Scene};
use crate::shared::error::Result;
use crate::shared::ids::WinId;

impl Engine {
    /// The scene the backend should show: the workspace on each screen, laid out in the
    /// area of its output that bars left free, with that workspace's floating windows
    /// above its tiles. A fullscreen window fills the whole output, bars included, and
    /// is drawn above everything else on it; the other windows keep their places below.
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
        let mut fullscreen = Vec::new();
        for (output, workspace) in self.stacks.screens() {
            let (Some(area), Some(work)) = (self.area_of(output), self.work_area_of(output)) else {
                continue;
            };
            let tiled: Vec<WinId> = workspace
                .windows()
                .filter(|&id| !workspace.is_floating(id))
                .collect();
            let mut here = if workspace.layout() == Layout::External {
                let key = Key {
                    area: work,
                    windows: tiled.clone(),
                };
                match self.answer_for(workspace.tag(), &key) {
                    Some(answer) => answer.to_vec(),
                    // No answer fits yet: the default layout keeps the screen usable.
                    None => Layout::default().arrange(work, &tiled, workspace.focus())?,
                }
            } else {
                workspace
                    .layout()
                    .arrange(work, &tiled, workspace.focus())?
            };
            for (id, float) in workspace.floats() {
                here.push(Placement {
                    id,
                    rect: float.rect(work)?,
                });
            }
            if let Some(id) = workspace.fullscreen() {
                here.retain(|placement| placement.id != id);
                here.push(Placement { id, rect: area });
                fullscreen.push(id);
            }
            placements.extend(here);
        }
        // A focused screen with no area has no windows in the scene to focus.
        let focus = self
            .area_of(self.stacks.current_output())
            .and(self.stacks.focused());
        // Only the windows shown get a style: one on a hidden workspace keeps its style
        // in the engine and gets it back when it returns.
        let styled: Vec<_> = placements
            .iter()
            .filter_map(|p| self.styles.get(&p.id).map(|style| (p.id, *style)))
            .collect();
        Scene::try_new(placements, focus)?
            .with_fullscreen(fullscreen)?
            .with_style(styled)
    }
}
