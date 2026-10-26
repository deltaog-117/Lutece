//! The minimal list of changes that turns one scene into the next, so a backend sends
//! only what differs instead of redrawing everything.

use super::scene::{Placement, Scene};
use crate::shared::ids::WinId;

/// One step a backend applies to its current state.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Change {
    /// The window left the scene; hide it.
    Hide(WinId),
    /// The window entered the scene; show it at this placement.
    Show(Placement),
    /// The window stays visible but its area changed.
    Move(Placement),
    /// The stacking order changed; this is the complete new order, bottom to top.
    Restack(Vec<WinId>),
    /// Focus changed.
    Focus(Option<WinId>),
}

/// Compares two scenes.
///
/// Changes come in a fixed order: hides, then shows and moves in stacking order, then
/// at most one restack, then at most one focus change. Hiding first frees the space
/// before newcomers fill it. A restack is sent whenever a window is shown, because
/// where the newcomer sits in the stack is only known from the full order.
///
/// # Examples
///
/// ```
/// use lutece::{Change, Placement, Rect, Scene, WinId, diff};
///
/// let win = WinId::new(1);
/// let rect = Rect::new(0, 0, 800, 600)?;
/// let after = Scene::try_new(vec![Placement { id: win, rect }], Some(win))?;
///
/// let changes = diff(&Scene::empty(), &after);
/// assert_eq!(changes[0], Change::Show(Placement { id: win, rect }));
/// assert!(diff(&after, &after).is_empty());
/// # Ok::<(), lutece::Error>(())
/// ```
#[must_use]
pub fn diff(old: &Scene, new: &Scene) -> Vec<Change> {
    let hides = old
        .placements()
        .iter()
        .filter(|p| new.get(p.id).is_none())
        .map(|p| Change::Hide(p.id));

    let shows_and_moves = new.placements().iter().filter_map(|p| match old.get(p.id) {
        None => Some(Change::Show(*p)),
        Some(before) if before.rect != p.rect => Some(Change::Move(*p)),
        Some(_) => None,
    });

    let new_order: Vec<WinId> = new.placements().iter().map(|p| p.id).collect();
    let surviving_order = old
        .placements()
        .iter()
        .map(|p| p.id)
        .filter(|id| new.get(*id).is_some());
    let restack =
        (!surviving_order.eq(new_order.iter().copied())).then_some(Change::Restack(new_order));

    let focus = (old.focus() != new.focus()).then(|| Change::Focus(new.focus()));

    hides
        .chain(shows_and_moves)
        .chain(restack)
        .chain(focus)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::geometry::Rect;
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};

    fn rect_strategy() -> impl Strategy<Value = Rect> {
        (-2000i32..2000, -2000i32..2000, 1u32..4000, 1u32..4000)
            .prop_map(|(x, y, w, h)| Rect::new(x, y, w, h).expect("generated non-zero size"))
    }

    /// Windows come from a small id range so two generated scenes usually overlap.
    fn scene_strategy() -> impl Strategy<Value = Scene> {
        (
            proptest::collection::vec((0u64..12, rect_strategy()), 0..10),
            proptest::option::of(any::<usize>()),
        )
            .prop_map(|(raw, focus_pick)| {
                let mut seen = HashSet::new();
                let placements: Vec<Placement> = raw
                    .into_iter()
                    .filter(|(id, _)| seen.insert(*id))
                    .map(|(id, rect)| Placement {
                        id: WinId::new(id),
                        rect,
                    })
                    .collect();
                let focus = focus_pick
                    .filter(|_| !placements.is_empty())
                    .map(|pick| placements[pick % placements.len()].id);
                Scene::try_new(placements, focus).expect("ids are deduplicated, focus is listed")
            })
    }

    /// A stand-in backend: it only knows what the changes tell it.
    fn apply(old: &Scene, changes: &[Change]) -> Scene {
        let mut rects: HashMap<WinId, Rect> =
            old.placements().iter().map(|p| (p.id, p.rect)).collect();
        let mut order: Vec<WinId> = old.placements().iter().map(|p| p.id).collect();
        let mut focus = old.focus();

        for change in changes {
            match change {
                Change::Hide(id) => {
                    rects.remove(id);
                    order.retain(|other| other != id);
                }
                Change::Show(p) => {
                    rects.insert(p.id, p.rect);
                    order.push(p.id);
                }
                Change::Move(p) => {
                    rects.insert(p.id, p.rect);
                }
                Change::Restack(new_order) => order.clone_from(new_order),
                Change::Focus(id) => focus = *id,
            }
        }

        let placements = order
            .into_iter()
            .map(|id| Placement {
                id,
                rect: rects[&id],
            })
            .collect();
        Scene::try_new(placements, focus).expect("changes keep the scene valid")
    }

    proptest! {
        #[test]
        fn identical_scenes_need_no_changes(scene in scene_strategy()) {
            prop_assert!(diff(&scene, &scene).is_empty());
        }

        #[test]
        fn applying_the_diff_reaches_the_new_scene(
            old in scene_strategy(), new in scene_strategy(),
        ) {
            prop_assert_eq!(apply(&old, &diff(&old, &new)), new);
        }

        #[test]
        fn hides_are_exactly_the_windows_that_left(
            old in scene_strategy(), new in scene_strategy(),
        ) {
            let hidden: HashSet<WinId> = diff(&old, &new)
                .into_iter()
                .filter_map(|c| if let Change::Hide(id) = c { Some(id) } else { None })
                .collect();
            let left: HashSet<WinId> = old
                .placements()
                .iter()
                .map(|p| p.id)
                .filter(|id| new.get(*id).is_none())
                .collect();
            prop_assert_eq!(hidden, left);
        }
    }

    #[test]
    fn moving_one_window_sends_only_that_move() {
        let rect = |x| Rect::new(x, 0, 10, 10).expect("non-zero size");
        let a = WinId::new(1);
        let b = WinId::new(2);
        let before = Scene::try_new(
            vec![
                Placement {
                    id: a,
                    rect: rect(0),
                },
                Placement {
                    id: b,
                    rect: rect(10),
                },
            ],
            Some(a),
        )
        .expect("valid scene");
        let after = Scene::try_new(
            vec![
                Placement {
                    id: a,
                    rect: rect(0),
                },
                Placement {
                    id: b,
                    rect: rect(20),
                },
            ],
            Some(a),
        )
        .expect("valid scene");

        assert_eq!(
            diff(&before, &after),
            [Change::Move(Placement {
                id: b,
                rect: rect(20)
            })]
        );
    }
}
