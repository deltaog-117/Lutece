//! Monocle: every window fills the whole area and the focused one is on top.

use crate::shared::contracts::scene::Placement;
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

/// Stacks all windows at `area`, bottom to top, with `focus` last so it is the one seen.
/// A `focus` that is not among `windows` is ignored.
pub(crate) fn arrange(area: Rect, windows: &[WinId], focus: Option<WinId>) -> Vec<Placement> {
    windows
        .iter()
        .copied()
        .filter(|&id| Some(id) != focus)
        .chain(focus.filter(|f| windows.contains(f)))
        .map(|id| Placement { id, rect: area })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn area() -> Rect {
        Rect::new(5, 7, 800, 600).expect("non-zero size")
    }

    proptest! {
        #[test]
        fn every_window_fills_the_area_and_none_is_lost(
            count in 0_u64..16, focus_at in proptest::option::of(0_u64..20),
        ) {
            let windows: Vec<_> = (0..count).map(WinId::new).collect();
            let focus = focus_at.map(WinId::new);
            let placed = arrange(area(), &windows, focus);

            prop_assert!(placed.iter().all(|p| p.rect == area()));
            let mut ids: Vec<_> = placed.iter().map(|p| p.id).collect();
            ids.sort();
            prop_assert_eq!(ids, windows);
        }

        #[test]
        fn a_focused_window_in_the_list_is_on_top(count in 1_u64..16, pick in 0_u64..16) {
            let windows: Vec<_> = (0..count).map(WinId::new).collect();
            let focus = WinId::new(pick % count);
            let placed = arrange(area(), &windows, Some(focus));
            prop_assert_eq!(placed.last().map(|p| p.id), Some(focus));
        }
    }

    #[test]
    fn no_windows_means_no_placements() {
        assert!(arrange(area(), &[], None).is_empty());
    }
}
