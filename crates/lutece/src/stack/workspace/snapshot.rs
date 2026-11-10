//! A workspace as plain data and back.
//!
//! Reading a workspace from data goes through the same checks the workspace keeps all the
//! time: a window is listed once, only windows of the stack float, and the fullscreen window
//! is the focused one.

use super::{Stack, Workspace};
use crate::float::Float;
use crate::layout::{Layout, MasterRatio};
use crate::shared::error::{Error, Result};
use crate::shared::ids::{Tag, WinId};
use crate::snapshot::{FloatState, LayoutState, StackState, WorkspaceState};
use std::collections::{HashMap, HashSet, VecDeque};

fn refused(tag: &str, why: impl std::fmt::Display) -> Error {
    Error::InvalidSnapshot(format!("workspace {tag:?}: {why}"))
}

impl Workspace {
    /// What this workspace holds, as plain data.
    pub(in crate::stack) fn to_state(&self) -> WorkspaceState {
        let mut floating: Vec<FloatState> = self
            .floating
            .iter()
            .map(|(id, float)| {
                let (dx, dy, dw, dh) = float.parts();
                FloatState {
                    window: id.get(),
                    dx,
                    dy,
                    dw,
                    dh,
                }
            })
            .collect();
        floating.sort_by_key(|float| float.window);
        WorkspaceState {
            tag: self.tag.as_str().to_owned(),
            rank: self.rank,
            stack: self.stack.as_ref().map(|stack| StackState {
                up: stack.up.iter().map(|id| id.get()).collect(),
                focus: stack.focus.get(),
                down: stack.down.iter().map(|id| id.get()).collect(),
            }),
            layout: match self.layout {
                Layout::Tile(ratio) => LayoutState::Tile(ratio.permille()),
                Layout::Monocle => LayoutState::Monocle,
                Layout::Grid => LayoutState::Grid,
                Layout::CenteredMaster(ratio) => LayoutState::CenteredMaster(ratio.permille()),
                Layout::External => LayoutState::External,
            },
            tile_ratio: self.tile_ratio.permille(),
            floating,
            fullscreen: self.fullscreen.map(WinId::get),
        }
    }

    /// Builds a workspace from `state`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSnapshot`] if the tag or a ratio is not valid, a window is
    /// listed twice, a floating window is not in the stack, or the fullscreen window is not
    /// the focused one.
    pub(in crate::stack) fn from_state(state: &WorkspaceState) -> Result<Self> {
        let tag = Tag::try_new(state.tag.as_str()).map_err(|error| refused(&state.tag, error))?;
        let ratio =
            |permille| MasterRatio::try_new(permille).map_err(|error| refused(&state.tag, error));
        let layout = match state.layout {
            LayoutState::Tile(permille) => Layout::Tile(ratio(permille)?),
            LayoutState::Monocle => Layout::Monocle,
            LayoutState::Grid => Layout::Grid,
            LayoutState::CenteredMaster(permille) => Layout::CenteredMaster(ratio(permille)?),
            LayoutState::External => Layout::External,
        };
        let tile_ratio = ratio(state.tile_ratio)?;

        let stack = match &state.stack {
            None => None,
            Some(stack) => {
                let mut seen = HashSet::new();
                let all = stack
                    .up
                    .iter()
                    .chain(std::iter::once(&stack.focus))
                    .chain(&stack.down);
                for raw in all {
                    if !seen.insert(*raw) {
                        return Err(refused(&state.tag, format!("window {raw} is listed twice")));
                    }
                }
                Some(Stack {
                    up: stack
                        .up
                        .iter()
                        .map(|raw| WinId::new(*raw))
                        .collect::<VecDeque<_>>(),
                    focus: WinId::new(stack.focus),
                    down: stack
                        .down
                        .iter()
                        .map(|raw| WinId::new(*raw))
                        .collect::<VecDeque<_>>(),
                })
            }
        };

        let mut floating = HashMap::new();
        for float in &state.floating {
            let id = WinId::new(float.window);
            if !stack.as_ref().is_some_and(|stack| stack.contains(id)) {
                return Err(refused(
                    &state.tag,
                    format!("floating window {} is not in the stack", float.window),
                ));
            }
            let value = Float::from_parts(float.dx, float.dy, float.dw, float.dh);
            if floating.insert(id, value).is_some() {
                return Err(refused(
                    &state.tag,
                    format!("window {} floats twice", float.window),
                ));
            }
        }

        let fullscreen = state.fullscreen.map(WinId::new);
        if fullscreen.is_some() && fullscreen != stack.as_ref().map(Stack::focus) {
            return Err(refused(
                &state.tag,
                "the fullscreen window is not the focused one",
            ));
        }

        Ok(Self {
            tag,
            rank: state.rank,
            stack,
            layout,
            tile_ratio,
            floating,
            fullscreen,
        })
    }
}
