//! One workspace: an ordered list of windows with exactly one of them focused.

use crate::shared::ids::{Tag, WinId};
use std::collections::VecDeque;
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

/// A named workspace and the windows on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    tag: Tag,
    stack: Option<Stack>,
}

impl Workspace {
    pub(crate) fn empty(tag: Tag) -> Self {
        Self { tag, stack: None }
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
    }

    pub(crate) fn remove(&mut self, id: WinId) {
        if let Some(stack) = self.stack.take() {
            self.stack = stack.remove(id);
        }
    }

    pub(crate) fn focus_next(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.focus_next();
        }
    }

    pub(crate) fn focus_prev(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.focus_prev();
        }
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
        self.stack.as_mut().is_some_and(|s| s.focus_on(id))
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
