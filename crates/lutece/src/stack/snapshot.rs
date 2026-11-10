//! The set of workspaces as plain data and back.

use super::stackset::{Screen, StackSet};
use super::workspace::Workspace;
use crate::shared::error::{Error, Result};
use crate::shared::ids::OutputId;
use crate::snapshot::{ScreenState, WorkspaceState};
use std::collections::HashSet;

fn refused(why: impl std::fmt::Display) -> Error {
    Error::InvalidSnapshot(format!("workspaces: {why}"))
}

impl StackSet {
    /// The screens in order, which of them has the focus, and the hidden workspaces, as plain
    /// data.
    pub(crate) fn to_state(&self) -> (Vec<ScreenState>, usize, Vec<WorkspaceState>) {
        let screens = self
            .before
            .iter()
            .chain(std::iter::once(&self.current))
            .chain(&self.after)
            .map(|screen| ScreenState {
                output: screen.output.get(),
                workspace: screen.workspace.to_state(),
            })
            .collect();
        let hidden = self.hidden.iter().map(Workspace::to_state).collect();
        (screens, self.before.len(), hidden)
    }

    /// Builds the set from plain data, for a configuration whose workspaces are those of
    /// `template`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSnapshot`] if a workspace is not valid, the workspaces are not
    /// exactly the template's (same tags, same order), there is no screen, `current` is not
    /// a screen, an output has two screens, or a window is on two workspaces.
    pub(crate) fn from_state(
        template: &Self,
        screens: &[ScreenState],
        current: usize,
        hidden: &[WorkspaceState],
    ) -> Result<Self> {
        if screens.is_empty() {
            return Err(refused("there is no screen"));
        }
        if current >= screens.len() {
            return Err(refused(format!(
                "screen {current} has the focus, but there are {} screens",
                screens.len()
            )));
        }

        let mut outputs = HashSet::new();
        let mut shown = Vec::with_capacity(screens.len());
        for screen in screens {
            if !outputs.insert(screen.output) {
                return Err(refused(format!("output {} has two screens", screen.output)));
            }
            shown.push(Screen {
                output: OutputId::new(screen.output),
                workspace: Workspace::from_state(&screen.workspace)?,
            });
        }
        let hidden = hidden
            .iter()
            .map(Workspace::from_state)
            .collect::<Result<Vec<_>>>()?;

        let wanted: Vec<_> = template.tags().cloned().collect();
        let mut held = HashSet::new();
        let mut windows = HashSet::new();
        for workspace in shown.iter().map(|screen| &screen.workspace).chain(&hidden) {
            let Some(rank) = wanted.iter().position(|tag| tag == workspace.tag()) else {
                return Err(refused(format!(
                    "{} is not a workspace of the configuration",
                    workspace.tag()
                )));
            };
            if workspace.rank() != rank {
                return Err(refused(format!(
                    "{} has rank {}, the configuration puts it at {rank}",
                    workspace.tag(),
                    workspace.rank()
                )));
            }
            if !held.insert(rank) {
                return Err(refused(format!("{} is held twice", workspace.tag())));
            }
            for id in workspace.windows() {
                if !windows.insert(id) {
                    return Err(refused(format!("window {} is on two workspaces", id.get())));
                }
            }
        }
        if held.len() != wanted.len() {
            return Err(refused("a workspace of the configuration is missing"));
        }

        let mut after = shown.split_off(current);
        let current = after.remove(0);
        Ok(Self {
            before: shown,
            current,
            after,
            hidden,
        })
    }
}
