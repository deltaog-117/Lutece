//! One workspace: an ordered list of windows with exactly one of them focused.

use crate::float::Float;
use crate::layout::{Layout, MasterRatio};
use crate::shared::ids::{Tag, WinId};
use std::collections::{HashMap, VecDeque};
use std::iter;
use std::mem;

/// A non-empty ordered list of windows split around the focused one (a zipper).
///
/// Reading order is `up`, then `focus`, then `down`. Because `focus` is a plain value
/// and not an `Option` or an index, a stack can never be focused on nothing or on a
/// window that is not in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stack {
    /// Windows before the focus; the one nearest the focus is last.
    up: VecDeque<WinId>,
    focus: WinId,
    /// Windows after the focus; the one nearest the focus is first.
    down: VecDeque<WinId>,
}

impl Stack {
    pub(crate) fn new(focus: WinId) -> Self {
        Self {
            up: VecDeque::new(),
            focus,
            down: VecDeque::new(),
        }
    }

    pub(crate) fn focus(&self) -> WinId {
        self.focus
    }

    pub(crate) fn len(&self) -> usize {
        self.up.len() + 1 + self.down.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = WinId> + '_ {
        self.up
            .iter()
            .copied()
            .chain(iter::once(self.focus))
            .chain(self.down.iter().copied())
    }

    pub(crate) fn contains(&self, id: WinId) -> bool {
        self.iter().any(|w| w == id)
    }

    /// The new window takes focus and the old focus sits right after it. Callers check
    /// that `id` is not already present.
    pub(crate) fn insert(&mut self, id: WinId) {
        let previous = mem::replace(&mut self.focus, id);
        self.down.push_front(previous);
    }

    /// Removes `id`. Returns `None` if that leaves the stack empty. If the focused
    /// window goes, focus passes to the next one, or to the previous one at the end.
    pub(crate) fn remove(mut self, id: WinId) -> Option<Self> {
        if self.focus != id {
            self.up.retain(|w| *w != id);
            self.down.retain(|w| *w != id);
            return Some(self);
        }
        if let Some(next) = self.down.pop_front() {
            self.focus = next;
            Some(self)
        } else if let Some(previous) = self.up.pop_back() {
            self.focus = previous;
            Some(self)
        } else {
            None
        }
    }

    /// Focus moves to the next window, wrapping from the last to the first.
    pub(crate) fn focus_next(&mut self) {
        if let Some(next) = self.down.pop_front() {
            let previous = mem::replace(&mut self.focus, next);
            self.up.push_back(previous);
        } else if let Some(first) = self.up.pop_front() {
            let previous = mem::replace(&mut self.focus, first);
            self.down = mem::take(&mut self.up);
            self.down.push_back(previous);
        }
    }

    /// Focus moves to the previous window, wrapping from the first to the last.
    pub(crate) fn focus_prev(&mut self) {
        if let Some(previous) = self.up.pop_back() {
            let next = mem::replace(&mut self.focus, previous);
            self.down.push_front(next);
        } else if let Some(last) = self.down.pop_back() {
            let next = mem::replace(&mut self.focus, last);
            self.up = mem::take(&mut self.down);
            self.up.push_front(next);
        }
    }

    /// The focused window trades places with the next one; from the end it goes to
    /// the front. It keeps focus.
    pub(crate) fn swap_next(&mut self) {
        if let Some(next) = self.down.pop_front() {
            self.up.push_back(next);
        } else {
            self.down = mem::take(&mut self.up);
        }
    }

    /// The focused window trades places with the previous one; from the front it goes
    /// to the end. It keeps focus.
    pub(crate) fn swap_prev(&mut self) {
        if let Some(previous) = self.up.pop_back() {
            self.down.push_front(previous);
        } else {
            self.up = mem::take(&mut self.down);
        }
    }

    /// Focuses `id` if it is present and reports whether it was.
    pub(crate) fn focus_on(&mut self, id: WinId) -> bool {
        // Bounded by the length, so a missing window cannot loop forever.
        for _ in 0..self.len() {
            if self.focus == id {
                return true;
            }
            self.focus_next();
        }
        self.focus == id
    }
}

/// A named workspace, the windows on it and how it arranges them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    tag: Tag,
    /// Where the tag sits in the configured order. A workspace moves between screens and
    /// the hidden pool, so the order cannot be read from where it is stored.
    rank: usize,
    stack: Option<Stack>,
    layout: Layout,
    /// The master ratio the layouts with a master column get back when the layout cycles
    /// round to them again. Only read while a layout without one is showing.
    tile_ratio: MasterRatio,
    /// The windows the layout leaves alone. Every key is a window in `stack`: `remove`
    /// drops the entry and `set_float` refuses strangers.
    floating: HashMap<WinId, Float>,
    /// The window that fills the whole screen. It is always the focused window: any
    /// change of focus drops it (see `settle`), so the window that has the keyboard is
    /// never hidden behind a fullscreen one.
    fullscreen: Option<WinId>,
}

impl Workspace {
    pub(crate) fn empty(tag: Tag, layout: Layout, rank: usize) -> Self {
        let tile_ratio = match layout {
            Layout::Tile(ratio) | Layout::CenteredMaster(ratio) => ratio,
            Layout::Monocle | Layout::Grid => MasterRatio::default(),
        };
        Self {
            tag,
            rank,
            stack: None,
            layout,
            tile_ratio,
            floating: HashMap::new(),
            fullscreen: None,
        }
    }

    /// The window that fills the whole screen, if any. It is the focused window; moving
    /// the focus to another window, or closing this one, ends its fullscreen.
    #[must_use]
    pub fn fullscreen(&self) -> Option<WinId> {
        self.fullscreen
    }

    /// Makes `id` fullscreen, or ends its fullscreen with `on` false. Fullscreening
    /// focuses the window and replaces any other fullscreen window. A window that is not
    /// on this workspace is ignored.
    pub(crate) fn set_fullscreen(&mut self, id: WinId, on: bool) {
        if !on {
            if self.fullscreen == Some(id) {
                self.fullscreen = None;
            }
        } else if self.focus_on(id) {
            self.fullscreen = Some(id);
        }
    }

    /// Restores the rule that only the focused window can be fullscreen. Called after
    /// every change of the stack.
    fn settle(&mut self) {
        if self.fullscreen != self.focus() {
            self.fullscreen = None;
        }
    }

    pub(crate) fn rank(&self) -> usize {
        self.rank
    }

    /// Whether `id` floats instead of being placed by the layout.
    #[must_use]
    pub fn is_floating(&self, id: WinId) -> bool {
        self.floating.contains_key(&id)
    }

    pub(crate) fn float_of(&self, id: WinId) -> Option<Float> {
        self.floating.get(&id).copied()
    }

    /// The floating windows with their state, in stacking order, bottom first.
    pub(crate) fn floats(&self) -> impl Iterator<Item = (WinId, Float)> + '_ {
        self.windows()
            .filter_map(|id| self.float_of(id).map(|float| (id, float)))
    }

    /// Makes `id` float with `float`, or tile again with `None`. A window that is not on
    /// this workspace is ignored.
    pub(crate) fn set_float(&mut self, id: WinId, float: Option<Float>) {
        if !self.contains(id) {
            return;
        }
        match float {
            Some(float) => self.floating.insert(id, float),
            None => self.floating.remove(&id),
        };
    }

    /// How the workspace arranges its windows.
    #[must_use]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Switches to the next layout (tile, monocle, grid, centered master), wrapping
    /// around. The layouts with a master column keep the ratio it had when one was last
    /// showing.
    pub(crate) fn next_layout(&mut self) {
        self.layout = match self.layout {
            Layout::Tile(ratio) => {
                self.tile_ratio = ratio;
                Layout::Monocle
            }
            Layout::Monocle => Layout::Grid,
            Layout::Grid => Layout::CenteredMaster(self.tile_ratio),
            Layout::CenteredMaster(ratio) => {
                self.tile_ratio = ratio;
                Layout::Tile(ratio)
            }
        };
    }

    /// Moves the master ratio by `delta` thousandths. Layouts without a master column
    /// ignore it.
    pub(crate) fn resize_master(&mut self, delta: i16) {
        match self.layout {
            Layout::Tile(ratio) => self.layout = Layout::Tile(ratio.step(delta)),
            Layout::CenteredMaster(ratio) => {
                self.layout = Layout::CenteredMaster(ratio.step(delta));
            }
            Layout::Monocle | Layout::Grid => {}
        }
    }

    pub(crate) fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        if let Layout::Tile(ratio) | Layout::CenteredMaster(ratio) = layout {
            self.tile_ratio = ratio;
        }
    }

    /// The workspace's name.
    #[must_use]
    pub fn tag(&self) -> &Tag {
        &self.tag
    }

    /// The focused window, or `None` if the workspace is empty.
    #[must_use]
    pub fn focus(&self) -> Option<WinId> {
        self.stack.as_ref().map(Stack::focus)
    }

    /// The windows in layout order, top of the list first.
    pub fn windows(&self) -> impl Iterator<Item = WinId> + '_ {
        self.stack.iter().flat_map(Stack::iter)
    }

    /// How many windows are on the workspace.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stack.as_ref().map_or(0, Stack::len)
    }

    /// Whether the workspace has no windows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_none()
    }

    /// Whether `id` is on this workspace.
    #[must_use]
    pub fn contains(&self, id: WinId) -> bool {
        self.stack.as_ref().is_some_and(|s| s.contains(id))
    }

    pub(crate) fn insert(&mut self, id: WinId) {
        if let Some(stack) = self.stack.as_mut() {
            stack.insert(id);
        } else {
            self.stack = Some(Stack::new(id));
        }
        self.settle();
    }

    pub(crate) fn remove(&mut self, id: WinId) {
        self.floating.remove(&id);
        if let Some(stack) = self.stack.take() {
            self.stack = stack.remove(id);
        }
        self.settle();
    }

    pub(crate) fn focus_next(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.focus_next();
        }
        self.settle();
    }

    pub(crate) fn focus_prev(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.focus_prev();
        }
        self.settle();
    }

    pub(crate) fn swap_next(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.swap_next();
        }
    }

    pub(crate) fn swap_prev(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.swap_prev();
        }
    }

    pub(crate) fn focus_on(&mut self, id: WinId) -> bool {
        let found = self.stack.as_mut().is_some_and(|s| s.focus_on(id));
        self.settle();
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    fn w(raw: u64) -> WinId {
        WinId::new(raw)
    }

    fn stack(up: &[u64], focus: u64, down: &[u64]) -> Stack {
        Stack {
            up: up.iter().copied().map(w).collect(),
            focus: w(focus),
            down: down.iter().copied().map(w).collect(),
        }
    }

    fn order(stack: &Stack) -> Vec<u64> {
        stack.iter().map(WinId::get).collect()
    }

    /// Distinct windows with one of them focused.
    fn stack_strategy() -> impl Strategy<Value = Stack> {
        (
            proptest::collection::hash_set(0u64..50, 1..8),
            any::<usize>(),
        )
            .prop_map(|(ids, pick)| {
                let ids: Vec<u64> = ids.into_iter().collect();
                let at = pick % ids.len();
                stack(&ids[..at], ids[at], &ids[at + 1..])
            })
    }

    fn workspace(layout: Layout) -> Workspace {
        Workspace::empty(Tag::try_new("1").expect("tag has text"), layout, 0)
    }

    fn ratio(permille: u16) -> MasterRatio {
        MasterRatio::try_new(permille).expect("in range")
    }

    #[test]
    fn cycling_the_layout_brings_tile_back_with_its_ratio() {
        let mut ws = workspace(Layout::Tile(ratio(550)));
        ws.resize_master(100);
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::Monocle);
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::Grid);
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::CenteredMaster(ratio(650)));
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::Tile(ratio(650)));
    }

    #[test]
    fn resizing_the_master_does_nothing_in_monocle_or_grid() {
        for layout in [Layout::Monocle, Layout::Grid] {
            let mut ws = workspace(layout);
            ws.resize_master(100);
            assert_eq!(ws.layout(), layout);
        }
        let mut ws = workspace(Layout::Monocle);
        ws.next_layout();
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::CenteredMaster(MasterRatio::default()));
    }

    #[test]
    fn resizing_the_master_moves_the_centered_ratio_and_tile_inherits_it() {
        let mut ws = workspace(Layout::CenteredMaster(ratio(550)));
        ws.resize_master(100);
        assert_eq!(ws.layout(), Layout::CenteredMaster(ratio(650)));
        ws.next_layout();
        assert_eq!(ws.layout(), Layout::Tile(ratio(650)));
    }

    #[test]
    fn a_floating_window_is_forgotten_when_it_leaves() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.insert(w(2));
        ws.set_float(w(1), Some(Float::default()));
        assert!(ws.is_floating(w(1)));
        assert!(!ws.is_floating(w(2)));

        ws.remove(w(1));
        assert!(!ws.is_floating(w(1)));
        assert_eq!(ws.floats().count(), 0);
    }

    #[test]
    fn only_windows_on_the_workspace_can_float() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.set_float(w(9), Some(Float::default()));
        assert!(!ws.is_floating(w(9)));

        ws.set_float(w(1), Some(Float::default()));
        ws.set_float(w(1), None);
        assert!(!ws.is_floating(w(1)));
    }

    #[test]
    fn floats_come_out_in_stacking_order() {
        let mut ws = workspace(Layout::default());
        for id in [1, 2, 3] {
            ws.insert(w(id));
            ws.set_float(w(id), Some(Float::default()));
        }
        let order: Vec<u64> = ws.floats().map(|(id, _)| id.get()).collect();
        assert_eq!(order, ws.windows().map(WinId::get).collect::<Vec<_>>());
    }

    #[test]
    fn fullscreen_belongs_to_the_focused_window_only() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.insert(w(2));
        ws.set_fullscreen(w(2), true);
        assert_eq!(ws.fullscreen(), Some(w(2)));

        ws.focus_next();
        assert_eq!(ws.fullscreen(), None);
        ws.focus_next();
        assert_eq!(ws.fullscreen(), None, "coming back does not restore it");
    }

    #[test]
    fn fullscreen_ends_when_the_window_closes_or_a_new_one_opens() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.set_fullscreen(w(1), true);
        ws.insert(w(2));
        assert_eq!(ws.fullscreen(), None);

        ws.set_fullscreen(w(2), true);
        ws.remove(w(2));
        assert_eq!(ws.fullscreen(), None);
    }

    #[test]
    fn asking_for_fullscreen_focuses_the_window_and_replaces_the_old_one() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.insert(w(2));
        ws.set_fullscreen(w(2), true);

        ws.set_fullscreen(w(1), true);
        assert_eq!((ws.fullscreen(), ws.focus()), (Some(w(1)), Some(w(1))));

        ws.set_fullscreen(w(1), false);
        assert_eq!(ws.fullscreen(), None);
        // Strangers, and ending the fullscreen of another window, change nothing.
        ws.set_fullscreen(w(9), true);
        ws.set_fullscreen(w(2), false);
        assert_eq!((ws.fullscreen(), ws.focus()), (None, Some(w(1))));
    }

    #[test]
    fn swapping_keeps_the_focused_window_fullscreen() {
        let mut ws = workspace(Layout::default());
        ws.insert(w(1));
        ws.insert(w(2));
        ws.set_fullscreen(w(2), true);
        ws.swap_next();
        ws.swap_prev();
        assert_eq!(ws.fullscreen(), Some(w(2)));
    }

    #[test]
    fn a_new_window_takes_focus_and_the_old_focus_follows_it() {
        let mut s = stack(&[1], 2, &[3]);
        s.insert(w(9));
        assert_eq!(order(&s), [1, 9, 2, 3]);
        assert_eq!(s.focus(), w(9));
    }

    #[test]
    fn focus_wraps_in_both_directions() {
        let mut s = stack(&[1, 2], 3, &[]);
        s.focus_next();
        assert_eq!((order(&s), s.focus()), (vec![1, 2, 3], w(1)));
        s.focus_prev();
        assert_eq!((order(&s), s.focus()), (vec![1, 2, 3], w(3)));
    }

    #[test]
    fn swapping_past_the_end_moves_the_window_to_the_other_end() {
        let mut s = stack(&[1, 2], 3, &[]);
        s.swap_next();
        assert_eq!((order(&s), s.focus()), (vec![3, 1, 2], w(3)));
        s.swap_prev();
        assert_eq!((order(&s), s.focus()), (vec![1, 2, 3], w(3)));
    }

    #[test]
    fn removing_the_focus_passes_it_to_the_next_then_the_previous() {
        let s = stack(&[1], 2, &[3]).remove(w(2)).expect("two windows left");
        assert_eq!((order(&s), s.focus()), (vec![1, 3], w(3)));
        let s = s.remove(w(3)).expect("one window left");
        assert_eq!((order(&s), s.focus()), (vec![1], w(1)));
        assert_eq!(s.remove(w(1)), None);
    }

    proptest! {
        #[test]
        fn moving_focus_never_changes_which_windows_exist(
            mut s in stack_strategy(), steps in proptest::collection::vec(0u8..4, 0..12),
        ) {
            let before: HashSet<WinId> = s.iter().collect();
            for step in steps {
                match step {
                    0 => s.focus_next(),
                    1 => s.focus_prev(),
                    2 => s.swap_next(),
                    _ => s.swap_prev(),
                }
                prop_assert!(s.contains(s.focus()));
                prop_assert_eq!(s.iter().collect::<HashSet<_>>(), before.clone());
                prop_assert_eq!(s.len(), before.len());
            }
        }

        #[test]
        fn focus_next_a_full_lap_returns_to_the_start(mut s in stack_strategy()) {
            let start = s.clone();
            for _ in 0..s.len() {
                s.focus_next();
            }
            prop_assert_eq!(s, start);
        }

        #[test]
        fn focus_prev_undoes_focus_next(mut s in stack_strategy()) {
            let start = s.clone();
            s.focus_next();
            s.focus_prev();
            prop_assert_eq!(s, start);
        }

        #[test]
        fn swap_prev_undoes_swap_next(mut s in stack_strategy()) {
            let start = s.clone();
            s.swap_next();
            s.swap_prev();
            prop_assert_eq!(s, start);
        }

        #[test]
        fn focus_on_reaches_every_member_and_rejects_strangers(
            mut s in stack_strategy(), target in 0u64..60,
        ) {
            let present = s.contains(w(target));
            prop_assert_eq!(s.focus_on(w(target)), present);
            if present {
                prop_assert_eq!(s.focus(), w(target));
            }
        }

        #[test]
        fn removing_a_window_removes_only_that_window(
            s in stack_strategy(), target in 0u64..60,
        ) {
            let mut expected: HashSet<WinId> = s.iter().collect();
            expected.remove(&w(target));
            match s.remove(w(target)) {
                Some(rest) => {
                    prop_assert!(rest.contains(rest.focus()));
                    prop_assert_eq!(rest.iter().collect::<HashSet<_>>(), expected);
                }
                None => prop_assert!(expected.is_empty()),
            }
        }
    }
}
