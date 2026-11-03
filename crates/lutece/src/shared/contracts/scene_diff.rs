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
    /// The window's fullscreen state in the scene differs from what it was. A window that
    /// appears already fullscreen gets this change too, ahead of its `Show`. A window that
    /// leaves the scene gets only a `Hide`; its state is forgotten.
    Fullscreen {
        /// The window.
        id: WinId,
        /// Whether it is now fullscreen.
        on: bool,
    },
    /// Focus changed.
    Focus(Option<WinId>),
}

/// Compares two scenes.
///
/// Changes come in a fixed order: hides, then fullscreen changes, then shows and moves in
/// stacking order, then at most one restack, then at most one focus change. Hiding first
/// frees the space before newcomers fill it, and fullscreen comes before the placement
/// so a backend that sizes a window by its state does it once. A restack is sent whenever a window is shown, because
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

    let fullscreen = new.placements().iter().filter_map(|p| {
        // A window that was not visible before starts from "not fullscreen".
        let was = old.get(p.id).is_some() && old.is_fullscreen(p.id);
        let on = new.is_fullscreen(p.id);
        (was != on).then_some(Change::Fullscreen { id: p.id, on })
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
        .chain(fullscreen)
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
            0u64..4,
        )
            .prop_map(|(raw, focus_pick, fullscreen_salt)| {
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
                let fullscreen: Vec<WinId> = placements
                    .iter()
                    .filter(|p| p.id.get() % 3 == fullscreen_salt % 3)
                    .map(|p| p.id)
                    .collect();
                Scene::try_new(placements, focus)
                    .and_then(|scene| scene.with_fullscreen(fullscreen))
                    .expect("ids are deduplicated, focus and fullscreen are listed")
            })
    }

    /// A stand-in backend: it only knows what the changes tell it.
    fn apply(old: &Scene, changes: &[Change]) -> Scene {
        let mut rects: HashMap<WinId, Rect> =
            old.placements().iter().map(|p| (p.id, p.rect)).collect();
        let mut order: Vec<WinId> = old.placements().iter().map(|p| p.id).collect();
        let mut focus = old.focus();
        let mut fullscreen: HashSet<WinId> = old
            .placements()
            .iter()
            .map(|p| p.id)
            .filter(|id| old.is_fullscreen(*id))
            .collect();

        for change in changes {
            match change {
                Change::Hide(id) => {
                    rects.remove(id);
                    order.retain(|other| other != id);
                    fullscreen.remove(id);
                }
                Change::Fullscreen { id, on } => {
                    if *on {
                        fullscreen.insert(*id);
                    } else {
                        fullscreen.remove(id);
                    }
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
        Scene::try_new(placements, focus)
            .and_then(|scene| scene.with_fullscreen(fullscreen))
            .expect("changes keep the scene valid")
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
    fn a_window_entering_fullscreen_is_announced_before_its_move() {
        let rect = |w| Rect::new(0, 0, w, 10).expect("non-zero size");
        let a = WinId::new(1);
        let plain = Scene::try_new(
            vec![Placement {
                id: a,
                rect: rect(10),
            }],
            Some(a),
        )
        .expect("valid scene");
        let full = Scene::try_new(
            vec![Placement {
                id: a,
                rect: rect(99),
            }],
            Some(a),
        )
        .and_then(|scene| scene.with_fullscreen([a]))
        .expect("valid scene");

        assert_eq!(
            diff(&plain, &full),
            [
                Change::Fullscreen { id: a, on: true },
                Change::Move(Placement {
                    id: a,
                    rect: rect(99)
                }),
            ]
        );
        assert_eq!(
            diff(&full, &plain)[0],
            Change::Fullscreen { id: a, on: false }
        );
    }

    #[test]
    fn a_window_that_appears_fullscreen_is_announced_before_its_show() {
        let a = WinId::new(1);
        let rect = Rect::new(0, 0, 10, 10).expect("non-zero size");
        let full = Scene::try_new(vec![Placement { id: a, rect }], None)
            .and_then(|scene| scene.with_fullscreen([a]))
            .expect("valid scene");

        let changes = diff(&Scene::empty(), &full);
        assert_eq!(changes[0], Change::Fullscreen { id: a, on: true });
        assert!(matches!(changes[1], Change::Show(_)));
        // A window that leaves while fullscreen is only hidden.
        assert_eq!(diff(&full, &Scene::empty()), [Change::Hide(a)]);
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
