//! The external layout: what is asked of a plugin, and what it answered.
//!
//! The engine never waits for a plugin. A workspace with the external layout is shown with
//! the default layout until an answer that fits its windows arrives, so a plugin that is
//! slow, absent or wrong costs only the look of that workspace. Answers come in as events,
//! so a replayed session ends in the same state.

use super::Engine;
use crate::layout::Layout;
use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::scene::Placement;
use crate::shared::geometry::Rect;
use crate::shared::ids::{Tag, WinId};
use std::collections::{HashMap, HashSet};

/// What an answer is about: the area and the tiled windows, in reading order. An answer
/// is used only while the workspace still has exactly this key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Key {
    pub(super) area: Rect,
    pub(super) windows: Vec<WinId>,
}

/// The state of the external layout of one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Slot {
    /// The request that was made and not answered yet, with its serial.
    pending: Option<(u64, Key)>,
    /// The last answer that was accepted.
    answer: Option<(Key, Vec<Placement>)>,
    /// A key whose answer was refused. It is not asked again, or a plugin that answers
    /// wrongly would be asked in a loop.
    refused: Option<Key>,
}

/// The slots by workspace.
pub(super) type Slots = HashMap<Tag, Slot>;

impl Engine {
    /// Asks for a layout wherever a workspace on a screen has the external layout and no
    /// answer or request for its current windows and area.
    pub(super) fn request_layouts(&mut self, effects: &mut Vec<Effect>) {
        let wanted: Vec<(Tag, Key, Option<WinId>)> = self
            .stacks
            .screens()
            .filter(|(_, workspace)| workspace.layout() == Layout::External)
            .filter_map(|(output, workspace)| {
                let area = self.work_area_of(output)?;
                let windows: Vec<WinId> = workspace
                    .windows()
                    .filter(|&id| !workspace.is_floating(id))
                    .collect();
                (!windows.is_empty()).then(|| {
                    (
                        workspace.tag().clone(),
                        Key { area, windows },
                        workspace.focus(),
                    )
                })
            })
            .collect();
        for (tag, key, focus) in wanted {
            let slot = self.external.entry(tag.clone()).or_default();
            let known = slot.answer.as_ref().is_some_and(|(k, _)| *k == key)
                || slot.pending.as_ref().is_some_and(|(_, k)| *k == key)
                || slot.refused.as_ref() == Some(&key);
            if known {
                continue;
            }
            let serial = self.next_serial;
            self.next_serial = self.next_serial.wrapping_add(1);
            slot.pending = Some((serial, key.clone()));
            slot.refused = None;
            effects.push(Effect::LayoutRequested {
                tag,
                area: key.area,
                windows: key.windows,
                focus,
                serial,
            });
        }
    }

    /// The layout requests nobody has answered yet, oldest first.
    ///
    /// A backend or host that serves the answers should give these to a plugin that has
    /// just connected: a request is made once, when the windows change, so a plugin that
    /// was not there then would otherwise never be asked.
    #[must_use]
    pub fn pending_layout_requests(&self) -> Vec<Effect> {
        let mut pending: Vec<(u64, Effect)> = self
            .external
            .iter()
            .filter_map(|(tag, slot)| {
                let (serial, key) = slot.pending.as_ref()?;
                let focus = self
                    .stacks
                    .workspaces()
                    .find(|workspace| workspace.tag() == tag)
                    .and_then(|workspace| workspace.focus());
                Some((
                    *serial,
                    Effect::LayoutRequested {
                        tag: tag.clone(),
                        area: key.area,
                        windows: key.windows.clone(),
                        focus,
                        serial: *serial,
                    },
                ))
            })
            .collect();
        pending.sort_by_key(|(serial, _)| *serial);
        pending.into_iter().map(|(_, effect)| effect).collect()
    }

    /// Takes the answer to request `serial`, if that request is still outstanding.
    pub(super) fn provide_layout(&mut self, serial: u64, placements: Vec<Placement>) {
        let Some(slot) = self
            .external
            .values_mut()
            .find(|slot| slot.pending.as_ref().is_some_and(|(n, _)| *n == serial))
        else {
            return;
        };
        let Some((_, key)) = slot.pending.take() else {
            return;
        };
        if fits(&key, &placements) {
            slot.answer = Some((key, placements));
        } else {
            slot.refused = Some(key);
        }
    }

    /// The accepted answer for `tag` if it was made for exactly `key`.
    pub(super) fn answer_for(&self, tag: &Tag, key: &Key) -> Option<&[Placement]> {
        let (answered, placements) = self.external.get(tag)?.answer.as_ref()?;
        (answered == key).then_some(placements.as_slice())
    }
}

/// Whether `placements` names each window of the request once and keeps every rectangle
/// inside the request's area.
fn fits(key: &Key, placements: &[Placement]) -> bool {
    let asked: HashSet<WinId> = key.windows.iter().copied().collect();
    let answered: HashSet<WinId> = placements.iter().map(|p| p.id).collect();
    if placements.len() != key.windows.len() || answered != asked {
        return false;
    }
    let (left, top) = (i64::from(key.area.x()), i64::from(key.area.y()));
    let right = left + i64::from(key.area.width());
    let bottom = top + i64::from(key.area.height());
    placements.iter().all(|p| {
        let (x, y) = (i64::from(p.rect.x()), i64::from(p.rect.y()));
        x >= left
            && y >= top
            && x + i64::from(p.rect.width()) <= right
            && y + i64::from(p.rect.height()) <= bottom
    })
}
