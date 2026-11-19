//! The focus hook: when the window that has the focus of its workspace closes, a plugin may
//! choose which window takes the focus instead of the engine's own rule.
//!
//! The engine never waits for a plugin. It gives the focus to its own choice at once and
//! asks; an answer moves the focus only if nothing else has moved it since, so a plugin
//! that is slow, absent or wrong costs only a window that had the focus for a moment.
//! Answers come in as events, so a replayed session ends in the same state.

use super::Engine;
use crate::shared::contracts::effect::Effect;
use crate::shared::ids::{Tag, WinId};
use std::collections::BTreeMap;

/// A request nobody has answered. The windows are not kept: they are read from the
/// workspace when the request is made or repeated, so they are never out of date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Ask {
    /// Names the request in the answer.
    pub(super) serial: u64,
    /// The window that closed.
    pub(super) closed: WinId,
    /// The window the engine focused. An answer is used only while it still has the focus.
    pub(super) focus: WinId,
}

/// The open requests by workspace: at most one each, so a plugin that never answers cannot
/// make them grow, and a newer close replaces an older one.
pub(super) type Asks = BTreeMap<Tag, Ask>;

impl Engine {
    /// The workspace `id` has the focus of, when the focus hook is on.
    pub(super) fn focus_owner(&self, id: WinId) -> Option<Tag> {
        if !self.focus_hook {
            return None;
        }
        let workspace = self.stacks.workspaces().find(|w| w.contains(id))?;
        (workspace.focus() == Some(id)).then(|| workspace.tag().clone())
    }

    /// Asks for the focus of `tag` after `closed` went, if there is a choice to make.
    pub(super) fn ask_focus(&mut self, tag: Tag, closed: WinId, effects: &mut Vec<Effect>) {
        let Some(workspace) = self.stacks.workspaces().find(|w| *w.tag() == tag) else {
            return;
        };
        let windows: Vec<WinId> = workspace.windows().collect();
        let focus = workspace.focus();
        let Some(focus) = focus.filter(|_| windows.len() >= 2) else {
            // One window, or none: nothing to choose, and an older question is moot.
            self.focus_asks.remove(&tag);
            return;
        };
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        self.focus_asks.insert(
            tag.clone(),
            Ask {
                serial,
                closed,
                focus,
            },
        );
        effects.push(Effect::FocusDecisionRequested {
            serial,
            tag,
            closed,
            windows,
            focus,
        });
    }

    /// The focus requests nobody has answered and that can still be, oldest first.
    ///
    /// A host that serves the answers should give these to a plugin that has just connected,
    /// since a request is made once, when the window closes. One whose workspace has moved its
    /// focus since could not be used any more and is left out.
    #[must_use]
    pub fn pending_focus_requests(&self) -> Vec<Effect> {
        let mut pending: Vec<(u64, Effect)> = self
            .focus_asks
            .iter()
            .filter_map(|(tag, ask)| {
                let workspace = self.stacks.workspaces().find(|w| w.tag() == tag)?;
                (workspace.focus() == Some(ask.focus)).then(|| {
                    (
                        ask.serial,
                        Effect::FocusDecisionRequested {
                            serial: ask.serial,
                            tag: tag.clone(),
                            closed: ask.closed,
                            windows: workspace.windows().collect(),
                            focus: ask.focus,
                        },
                    )
                })
            })
            .collect();
        pending.sort_by_key(|(serial, _)| *serial);
        pending.into_iter().map(|(_, effect)| effect).collect()
    }

    /// Takes the answer to request `serial`: focuses `id` if the request is still
    /// outstanding, its workspace still has the focus where the engine put it, and `id` is a
    /// window of that workspace. The screen that shows the workspace does not change.
    pub(super) fn decide_focus(&mut self, serial: u64, id: WinId) {
        let Some(tag) = self
            .focus_asks
            .iter()
            .find(|(_, ask)| ask.serial == serial)
            .map(|(tag, _)| tag.clone())
        else {
            return;
        };
        let Some(ask) = self.focus_asks.remove(&tag) else {
            return;
        };
        let untouched = self
            .stacks
            .workspaces()
            .find(|w| *w.tag() == tag)
            .is_some_and(|w| w.focus() == Some(ask.focus));
        if untouched {
            let _ = self.stacks.focus_in(&tag, id);
        }
    }
}
