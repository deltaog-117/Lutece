//! All workspaces and the windows on them. This is the pure state a window manager
//! changes in response to events; it never touches a display.

use super::workspace::Workspace;
use crate::layout::Layout;
use crate::shared::error::{Error, Result};
use crate::shared::ids::{Tag, WinId};
use std::collections::HashSet;
use std::iter;
use std::mem;

/// Every workspace, in tag order, with one of them current.
///
/// A window lives on exactly one workspace. At least one workspace always exists, so
/// there is always a current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackSet {
    before: Vec<Workspace>,
    current: Workspace,
    after: Vec<Workspace>,
}

impl StackSet {
    /// Builds the set with one empty workspace per tag. The first tag is current. Every
    /// workspace starts with the default layout.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoWorkspaces`] if `tags` is empty, or
    /// [`Error::DuplicateTag`] if a tag appears twice.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{StackSet, Tag, WinId};
    ///
    /// let mut set = StackSet::new([Tag::try_new("1")?, Tag::try_new("2")?])?;
    /// set.insert(WinId::new(7))?;
    /// assert_eq!(set.focused(), Some(WinId::new(7)));
    /// assert!(set.insert(WinId::new(7)).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn new<I: IntoIterator<Item = Tag>>(tags: I) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut workspaces = Vec::new();
        for tag in tags {
            if !seen.insert(tag.clone()) {
                return Err(Error::DuplicateTag(tag));
            }
            workspaces.push(Workspace::empty(tag, Layout::default()));
        }
        let mut workspaces = workspaces.into_iter();
        let current = workspaces.next().ok_or(Error::NoWorkspaces)?;
        Ok(Self {
            before: Vec::new(),
            current,
            after: workspaces.collect(),
        })
    }

    /// Gives every workspace `layout`.
    pub(crate) fn with_layout(mut self, layout: Layout) -> Self {
        self.workspaces_mut()
            .for_each(|workspace| workspace.set_layout(layout));
        self
    }

    /// The workspace on screen.
    #[must_use]
    pub fn current(&self) -> &Workspace {
        &self.current
    }

    /// The focused window of the current workspace.
    #[must_use]
    pub fn focused(&self) -> Option<WinId> {
        self.current.focus()
    }

    /// Every workspace, in tag order.
    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.before
            .iter()
            .chain(iter::once(&self.current))
            .chain(self.after.iter())
    }

    fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace> {
        self.before
            .iter_mut()
            .chain(iter::once(&mut self.current))
            .chain(self.after.iter_mut())
    }

    /// Every tag, in order.
    pub fn tags(&self) -> impl Iterator<Item = &Tag> {
        self.workspaces().map(Workspace::tag)
    }

    /// Whether any workspace holds `id`.
    #[must_use]
    pub fn contains(&self, id: WinId) -> bool {
        self.workspaces().any(|w| w.contains(id))
    }

    /// The tag of the workspace holding `id`.
    #[must_use]
    pub fn tag_of(&self, id: WinId) -> Option<&Tag> {
        self.workspaces()
            .find(|w| w.contains(id))
            .map(Workspace::tag)
    }

    /// Adds a window to the current workspace and focuses it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateWindow`] if the window is already managed.
    pub fn insert(&mut self, id: WinId) -> Result<()> {
        if self.contains(id) {
            return Err(Error::DuplicateWindow(id));
        }
        self.current.insert(id);
        Ok(())
    }

    /// Removes a window from whichever workspace holds it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownWindow`] if the window is not managed.
    pub fn remove(&mut self, id: WinId) -> Result<()> {
        let workspace = self
            .workspaces_mut()
            .find(|w| w.contains(id))
            .ok_or(Error::UnknownWindow(id))?;
        workspace.remove(id);
        Ok(())
    }

    /// Focuses `id`, switching to its workspace first if it is on another one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownWindow`] if the window is not managed.
    pub fn focus_window(&mut self, id: WinId) -> Result<()> {
        let index = self
            .workspaces()
            .position(|w| w.contains(id))
            .ok_or(Error::UnknownWindow(id))?;
        self.move_current_to(index);
        self.current.focus_on(id);
        Ok(())
    }

    /// Focuses the next window on the current workspace, wrapping around.
    pub fn focus_next(&mut self) {
        self.current.focus_next();
    }

    /// Focuses the previous window on the current workspace, wrapping around.
    pub fn focus_prev(&mut self) {
        self.current.focus_prev();
    }

    /// Swaps the focused window with the next one; the focus follows it.
    pub fn swap_next(&mut self) {
        self.current.swap_next();
    }

    /// Swaps the focused window with the previous one; the focus follows it.
    pub fn swap_prev(&mut self) {
        self.current.swap_prev();
    }

    /// Switches the current workspace to its next layout, wrapping around.
    pub fn next_layout(&mut self) {
        self.current.next_layout();
    }

    /// Moves the current workspace's master ratio by `delta` thousandths, if its layout
    /// has a master column.
    pub fn resize_master(&mut self, delta: i16) {
        self.current.resize_master(delta);
    }

    /// Makes the workspace with this tag current.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownTag`] if no workspace has the tag.
    pub fn focus_tag(&mut self, tag: &Tag) -> Result<()> {
        let index = self.index_of_tag(tag)?;
        self.move_current_to(index);
        Ok(())
    }

    /// Makes the next workspace current, wrapping around.
    pub fn focus_next_tag(&mut self) {
        let count = self.count();
        self.move_current_to((self.before.len() + 1) % count);
    }

    /// Makes the previous workspace current, wrapping around.
    pub fn focus_prev_tag(&mut self) {
        let count = self.count();
        self.move_current_to((self.before.len() + count - 1) % count);
    }

    /// Sends the focused window to another workspace. The current workspace stays on
    /// screen; the window becomes the focus of its new workspace. Does nothing if the
    /// current workspace is empty or is the target.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownTag`] if no workspace has the tag.
    pub fn move_focused_to(&mut self, tag: &Tag) -> Result<()> {
        let target = self.index_of_tag(tag)?;
        if target == self.before.len() {
            return Ok(());
        }
        let Some(id) = self.current.focus() else {
            return Ok(());
        };
        self.current.remove(id);
        // `target` came from `index_of_tag`, so the workspace exists.
        if let Some(workspace) = self.workspaces_mut().nth(target) {
            workspace.insert(id);
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.before.len() + 1 + self.after.len()
    }

    fn index_of_tag(&self, tag: &Tag) -> Result<usize> {
        self.tags()
            .position(|t| t == tag)
            .ok_or_else(|| Error::UnknownTag(tag.clone()))
    }

    /// Makes the workspace at `target` current. An out-of-range index changes nothing.
    fn move_current_to(&mut self, target: usize) {
        let here = self.before.len();
        if target < here {
            let mut moved = self.before.split_off(target).into_iter();
            let Some(next_current) = moved.next() else {
                return;
            };
            let previous = mem::replace(&mut self.current, next_current);
            let mut after: Vec<Workspace> = moved.collect();
            after.push(previous);
            after.append(&mut self.after);
            self.after = after;
        } else if target > here {
            let mut tail = self.after.split_off(target - here - 1).into_iter();
            let Some(next_current) = tail.next() else {
                return;
            };
            let previous = mem::replace(&mut self.current, next_current);
            self.before.push(previous);
            self.before.append(&mut self.after);
            self.after = tail.collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseError;

    fn w(raw: u64) -> WinId {
        WinId::new(raw)
    }

    fn tag(name: &str) -> Tag {
        Tag::try_new(name).expect("test tags have text")
    }

    fn tags() -> Vec<Tag> {
        ["1", "2", "3"].into_iter().map(tag).collect()
    }

    fn set() -> StackSet {
        StackSet::new(tags()).expect("three distinct tags")
    }

    fn windows_of(set: &StackSet, name: &str) -> Vec<u64> {
        set.workspaces()
            .find(|w| w.tag().as_str() == name)
            .expect("workspace exists")
            .windows()
            .map(WinId::get)
            .collect()
    }

    #[test]
    fn construction_needs_at_least_one_unique_tag() {
        assert_eq!(StackSet::new([]), Err(Error::NoWorkspaces));
        assert_eq!(
            StackSet::new([tag("a"), tag("a")]),
            Err(Error::DuplicateTag(tag("a")))
        );
    }

    #[test]
    fn the_first_tag_starts_current() {
        assert_eq!(set().current().tag(), &tag("1"));
    }

    #[test]
    fn a_window_cannot_be_managed_twice() {
        let mut s = set();
        s.insert(w(1)).expect("new window");
        s.focus_next_tag();
        assert_eq!(s.insert(w(1)), Err(Error::DuplicateWindow(w(1))));
    }

    #[test]
    fn unknown_windows_and_tags_are_refused() {
        let mut s = set();
        assert_eq!(s.remove(w(1)), Err(Error::UnknownWindow(w(1))));
        assert_eq!(s.focus_window(w(1)), Err(Error::UnknownWindow(w(1))));
        assert_eq!(s.focus_tag(&tag("9")), Err(Error::UnknownTag(tag("9"))));
        assert_eq!(
            s.move_focused_to(&tag("9")),
            Err(Error::UnknownTag(tag("9")))
        );
    }

    #[test]
    fn workspaces_wrap_in_both_directions() {
        let mut s = set();
        s.focus_prev_tag();
        assert_eq!(s.current().tag(), &tag("3"));
        s.focus_next_tag();
        assert_eq!(s.current().tag(), &tag("1"));
    }

    #[test]
    fn moving_the_focused_window_keeps_the_current_workspace_on_screen() {
        let mut s = set();
        s.insert(w(1)).expect("new window");
        s.insert(w(2)).expect("new window");
        s.move_focused_to(&tag("2")).expect("tag exists");
        assert_eq!(s.current().tag(), &tag("1"));
        assert_eq!(windows_of(&s, "1"), [1]);
        assert_eq!(windows_of(&s, "2"), [2]);
        assert_eq!(s.focused(), Some(w(1)));
    }

    #[test]
    fn focusing_a_window_on_another_workspace_switches_to_it() {
        let mut s = set();
        s.insert(w(1)).expect("new window");
        s.insert(w(2)).expect("new window");
        s.move_focused_to(&tag("3")).expect("tag exists");
        s.focus_window(w(2)).expect("window is managed");
        assert_eq!(s.current().tag(), &tag("3"));
        assert_eq!(s.focused(), Some(w(2)));
    }

    #[derive(Debug, Clone)]
    enum Op {
        Insert(u64),
        Remove(u64),
        FocusNext,
        FocusPrev,
        SwapNext,
        SwapPrev,
        FocusTag(usize),
        FocusNextTag,
        FocusPrevTag,
        FocusWindow(u64),
        MoveFocused(usize),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u64..8).prop_map(Op::Insert),
            (0u64..8).prop_map(Op::Remove),
            Just(Op::FocusNext),
            Just(Op::FocusPrev),
            Just(Op::SwapNext),
            Just(Op::SwapPrev),
            (0usize..3).prop_map(Op::FocusTag),
            Just(Op::FocusNextTag),
            Just(Op::FocusPrevTag),
            (0u64..8).prop_map(Op::FocusWindow),
            (0usize..3).prop_map(Op::MoveFocused),
        ]
    }

    /// Applies `op`, mirrors it in a plain set of managed windows, and checks the
    /// results that depend on the operation.
    fn step(
        set: &mut StackSet,
        model: &mut HashSet<WinId>,
        op: &Op,
    ) -> std::result::Result<(), TestCaseError> {
        let all_tags = tags();
        match op {
            Op::Insert(id) => {
                let known = model.contains(&w(*id));
                prop_assert_eq!(set.insert(w(*id)).is_err(), known);
                if !known {
                    model.insert(w(*id));
                    prop_assert_eq!(set.focused(), Some(w(*id)));
                }
            }
            Op::Remove(id) => {
                let known = model.remove(&w(*id));
                prop_assert_eq!(set.remove(w(*id)).is_ok(), known);
            }
            Op::FocusWindow(id) => {
                let known = model.contains(&w(*id));
                prop_assert_eq!(set.focus_window(w(*id)).is_ok(), known);
                if known {
                    prop_assert_eq!(set.focused(), Some(w(*id)));
                }
            }
            Op::FocusNext => set.focus_next(),
            Op::FocusPrev => set.focus_prev(),
            Op::SwapNext => set.swap_next(),
            Op::SwapPrev => set.swap_prev(),
            Op::FocusTag(i) => {
                let target = &all_tags[*i];
                prop_assert!(set.focus_tag(target).is_ok());
                prop_assert_eq!(set.current().tag(), target);
            }
            Op::FocusNextTag => set.focus_next_tag(),
            Op::FocusPrevTag => set.focus_prev_tag(),
            Op::MoveFocused(i) => {
                let target = &all_tags[*i];
                let before = (set.current().tag().clone(), set.focused());
                prop_assert!(set.move_focused_to(target).is_ok());
                prop_assert_eq!(set.current().tag(), &before.0);
                if let Some(id) = before.1.filter(|_| target != &before.0) {
                    prop_assert_eq!(set.tag_of(id), Some(target));
                }
            }
        }
        Ok(())
    }

    fn check_invariants(
        set: &StackSet,
        model: &HashSet<WinId>,
    ) -> std::result::Result<(), TestCaseError> {
        // Every managed window is on exactly one workspace, and nothing else is.
        let mut listed: Vec<WinId> = set.workspaces().flat_map(|w| w.windows()).collect();
        listed.sort();
        let mut expected: Vec<WinId> = model.iter().copied().collect();
        expected.sort();
        prop_assert_eq!(listed, expected);

        // Focus exists exactly when the workspace has windows, and is one of them.
        for workspace in set.workspaces() {
            prop_assert_eq!(workspace.focus().is_some(), !workspace.is_empty());
            if let Some(focus) = workspace.focus() {
                prop_assert!(workspace.contains(focus));
            }
        }

        // The tags never change or reorder, and the current one is among them.
        prop_assert_eq!(set.tags().cloned().collect::<Vec<_>>(), tags());
        prop_assert!(set.tags().any(|t| t == set.current().tag()));
        Ok(())
    }

    proptest! {
        #[test]
        fn any_sequence_of_operations_keeps_the_state_valid(
            ops in proptest::collection::vec(op_strategy(), 0..60),
        ) {
            let mut state = set();
            let mut model = HashSet::new();
            for op in &ops {
                step(&mut state, &mut model, op)?;
                check_invariants(&state, &model)?;
            }
        }
    }
}
