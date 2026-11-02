//! All workspaces and the windows on them. This is the pure state a window manager
//! changes in response to events; it never touches a display.

use super::workspace::Workspace;
use crate::float::Float;
use crate::layout::Layout;
use crate::shared::error::{Error, Result};
use crate::shared::ids::{OutputId, Tag, WinId};
use std::collections::HashSet;
use std::iter;
use std::mem;

/// A workspace on display at one output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Screen {
    pub(super) output: OutputId,
    pub(super) workspace: Workspace,
}

/// Every workspace, in tag order, shown on the screens or kept hidden.
///
/// Each screen shows one workspace and one of the screens has the focus; its workspace is
/// the current one. A workspace is either on one screen or hidden, never both and never
/// on two screens, and it stays in the set whatever happens to the screens, so a window
/// lives on exactly one workspace and an unplugged monitor cannot lose it. There is
/// always at least one workspace and one screen, so there is always a current workspace.
///
/// Until [`set_outputs`](StackSet::set_outputs) is called, the set has one screen, for
/// output 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackSet {
    pub(super) before: Vec<Screen>,
    pub(super) current: Screen,
    pub(super) after: Vec<Screen>,
    pub(super) hidden: Vec<Workspace>,
}

impl StackSet {
    /// Builds the set with one empty workspace per tag. The first tag is shown on the
    /// first screen and current; the others are hidden. Every workspace starts with the
    /// default layout.
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
        for (rank, tag) in tags.into_iter().enumerate() {
            if !seen.insert(tag.clone()) {
                return Err(Error::DuplicateTag(tag));
            }
            workspaces.push(Workspace::empty(tag, Layout::default(), rank));
        }
        let mut workspaces = workspaces.into_iter();
        let first = workspaces.next().ok_or(Error::NoWorkspaces)?;
        Ok(Self {
            before: Vec::new(),
            current: Screen {
                output: OutputId::new(0),
                workspace: first,
            },
            after: Vec::new(),
            hidden: workspaces.collect(),
        })
    }

    /// Gives every workspace `layout`.
    pub(crate) fn with_layout(mut self, layout: Layout) -> Self {
        self.workspaces_mut()
            .for_each(|workspace| workspace.set_layout(layout));
        self
    }

    /// The workspace on the focused screen.
    #[must_use]
    pub fn current(&self) -> &Workspace {
        &self.current.workspace
    }

    /// The output of the focused screen.
    #[must_use]
    pub fn current_output(&self) -> OutputId {
        self.current.output
    }

    /// The focused window of the current workspace.
    #[must_use]
    pub fn focused(&self) -> Option<WinId> {
        self.current.workspace.focus()
    }

    /// The screens in the order the backend reported their outputs, each with the
    /// workspace it shows.
    pub fn screens(&self) -> impl Iterator<Item = (OutputId, &Workspace)> {
        self.screens_in_order()
            .map(|screen| (screen.output, &screen.workspace))
    }

    fn screens_in_order(&self) -> impl Iterator<Item = &Screen> {
        self.before
            .iter()
            .chain(iter::once(&self.current))
            .chain(self.after.iter())
    }

    /// Every workspace, in tag order, whether it is shown or hidden.
    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        let mut all: Vec<&Workspace> = self
            .screens_in_order()
            .map(|screen| &screen.workspace)
            .chain(self.hidden.iter())
            .collect();
        all.sort_by_key(|workspace| workspace.rank());
        all.into_iter()
    }

    /// Every workspace in no particular order, for changes that do not depend on it.
    fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace> {
        self.before
            .iter_mut()
            .chain(iter::once(&mut self.current))
            .chain(self.after.iter_mut())
            .map(|screen| &mut screen.workspace)
            .chain(self.hidden.iter_mut())
    }

    fn workspace_mut(&mut self, tag: &Tag) -> Result<&mut Workspace> {
        self.workspaces_mut()
            .find(|workspace| workspace.tag() == tag)
            .ok_or_else(|| Error::UnknownTag(tag.clone()))
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
        self.current.workspace.insert(id);
        Ok(())
    }

    /// Adds a window to the workspace with this tag and focuses it there. The current
    /// workspace stays on screen.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateWindow`] if the window is already managed, or
    /// [`Error::UnknownTag`] if no workspace has the tag.
    pub fn insert_on(&mut self, id: WinId, tag: &Tag) -> Result<()> {
        if self.contains(id) {
            return Err(Error::DuplicateWindow(id));
        }
        self.workspace_mut(tag)?.insert(id);
        Ok(())
    }

    /// Adds a window to the current workspace as a floating window and focuses it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateWindow`] if the window is already managed.
    pub(crate) fn insert_floating(&mut self, id: WinId) -> Result<()> {
        self.insert(id)?;
        self.current.workspace.set_float(id, Some(Float::default()));
        Ok(())
    }

    /// Floats the focused window, or tiles it again if it floats. Does nothing on an
    /// empty workspace.
    pub fn toggle_float(&mut self) {
        let workspace = &mut self.current.workspace;
        let Some(id) = workspace.focus() else {
            return;
        };
        let float = (!workspace.is_floating(id)).then(Float::default);
        workspace.set_float(id, float);
    }

    /// Changes the focused window's float state. Does nothing if it is tiled.
    pub(crate) fn update_focused_float(&mut self, change: impl FnOnce(Float) -> Float) {
        let workspace = &mut self.current.workspace;
        let Some(id) = workspace.focus() else {
            return;
        };
        if let Some(float) = workspace.float_of(id) {
            workspace.set_float(id, Some(change(float)));
        }
    }

    /// Changes the float of `id` wherever it is, if it is floating; otherwise does nothing.
    pub(crate) fn update_float(&mut self, id: WinId, change: impl FnOnce(Float) -> Float) {
        let Some(workspace) = self.workspaces_mut().find(|w| w.contains(id)) else {
            return;
        };
        if let Some(float) = workspace.float_of(id) {
            workspace.set_float(id, Some(change(float)));
        }
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

    /// Focuses `id`, going to its workspace first: the screen that shows it gets the
    /// focus, or, if it is hidden, the focused screen shows it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownWindow`] if the window is not managed.
    pub fn focus_window(&mut self, id: WinId) -> Result<()> {
        let tag = self.tag_of(id).ok_or(Error::UnknownWindow(id))?.clone();
        self.focus_tag(&tag)?;
        self.current.workspace.focus_on(id);
        Ok(())
    }

    /// Focuses the next window on the current workspace, wrapping around.
    pub fn focus_next(&mut self) {
        self.current.workspace.focus_next();
    }

    /// Focuses the previous window on the current workspace, wrapping around.
    pub fn focus_prev(&mut self) {
        self.current.workspace.focus_prev();
    }

    /// Swaps the focused window with the next one; the focus follows it.
    pub fn swap_next(&mut self) {
        self.current.workspace.swap_next();
    }

    /// Swaps the focused window with the previous one; the focus follows it.
    pub fn swap_prev(&mut self) {
        self.current.workspace.swap_prev();
    }

    /// Switches the current workspace to its next layout, wrapping around.
    pub fn next_layout(&mut self) {
        self.current.workspace.next_layout();
    }

    /// Moves the current workspace's master ratio by `delta` thousandths, if its layout
    /// has a master column.
    pub fn resize_master(&mut self, delta: i16) {
        self.current.workspace.resize_master(delta);
    }

    /// Makes the workspace with this tag current. If another screen shows it, that screen
    /// gets the focus and nothing else changes; if it is hidden, the focused screen
    /// shows it and its old workspace is hidden.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownTag`] if no workspace has the tag.
    pub fn focus_tag(&mut self, tag: &Tag) -> Result<()> {
        let shown = self.screens().position(|(_, w)| w.tag() == tag);
        if let Some(index) = shown {
            self.focus_screen(index);
            return Ok(());
        }
        let slot = self
            .hidden
            .iter_mut()
            .find(|w| w.tag() == tag)
            .ok_or_else(|| Error::UnknownTag(tag.clone()))?;
        mem::swap(&mut self.current.workspace, slot);
        Ok(())
    }

    /// Makes the next workspace in tag order current, wrapping around. See
    /// [`focus_tag`](StackSet::focus_tag) for where it is shown.
    pub fn focus_next_tag(&mut self) {
        self.step_tag(true);
    }

    /// Makes the previous workspace in tag order current, wrapping around.
    pub fn focus_prev_tag(&mut self) {
        self.step_tag(false);
    }

    /// Gives the focus to the next screen, wrapping around.
    pub fn focus_next_output(&mut self) {
        self.focus_screen(self.neighbour(true));
    }

    /// Gives the focus to the previous screen, wrapping around.
    pub fn focus_prev_output(&mut self) {
        self.focus_screen(self.neighbour(false));
    }

    /// Sends the focused window to the workspace the next screen shows, as
    /// [`move_focused_to`](StackSet::move_focused_to) does. Does nothing with one screen.
    pub fn send_focused_to_next_output(&mut self) {
        self.send_focused_to_neighbour(true);
    }

    /// Sends the focused window to the workspace the previous screen shows.
    pub fn send_focused_to_prev_output(&mut self) {
        self.send_focused_to_neighbour(false);
    }

    /// Sends the focused window to another workspace, wherever that is shown. The
    /// current workspace stays on screen; the window becomes the focus of its new
    /// workspace. Does nothing if the current workspace is empty or is the target.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownTag`] if no workspace has the tag.
    pub fn move_focused_to(&mut self, tag: &Tag) -> Result<()> {
        if !self.tags().any(|t| t == tag) {
            return Err(Error::UnknownTag(tag.clone()));
        }
        if self.current.workspace.tag() == tag {
            return Ok(());
        }
        let Some(id) = self.current.workspace.focus() else {
            return Ok(());
        };
        // A window keeps floating, and where it was put, when it changes workspace.
        let float = self.current.workspace.float_of(id);
        self.current.workspace.remove(id);
        // The tag was checked above, so the workspace exists.
        if let Ok(workspace) = self.workspace_mut(tag) {
            workspace.insert(id);
            workspace.set_float(id, float);
        }
        Ok(())
    }

    pub(super) fn workspace_count(&self) -> usize {
        self.screens_in_order().count() + self.hidden.len()
    }

    fn step_tag(&mut self, forward: bool) {
        let count = self.workspace_count();
        let here = self
            .tags()
            .position(|t| t == self.current.workspace.tag())
            .unwrap_or(0);
        let next = if forward {
            (here + 1) % count
        } else {
            (here + count - 1) % count
        };
        let tag = self.tags().nth(next).cloned();
        if let Some(tag) = tag {
            // The tag came from the set, so it exists.
            let _ = self.focus_tag(&tag);
        }
    }

    /// The index of the screen after (or before) the focused one, wrapping around.
    fn neighbour(&self, forward: bool) -> usize {
        let count = self.screens_in_order().count();
        let here = self.before.len();
        if forward {
            (here + 1) % count
        } else {
            (here + count - 1) % count
        }
    }

    fn send_focused_to_neighbour(&mut self, forward: bool) {
        let target = self.neighbour(forward);
        let tag = self.screens().nth(target).map(|(_, w)| w.tag().clone());
        if let Some(tag) = tag {
            // The tag came from a screen, so it exists.
            let _ = self.move_focused_to(&tag);
        }
    }

    /// Gives the focus to the screen at `target`. An out-of-range index changes nothing.
    fn focus_screen(&mut self, target: usize) {
        if target >= self.screens_in_order().count() {
            return;
        }
        let here = self.before.len();
        if target < here {
            let mut moved = self.before.split_off(target).into_iter();
            let Some(next_current) = moved.next() else {
                return;
            };
            let previous = mem::replace(&mut self.current, next_current);
            let mut after: Vec<Screen> = moved.collect();
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
    fn a_floating_window_keeps_floating_and_its_place_on_another_workspace() {
        let mut set = set();
        set.insert_floating(w(1)).expect("new window");
        set.update_focused_float(|float| float.moved(30, 40));
        let placed = set.current().float_of(w(1));

        set.move_focused_to(&tag("2")).expect("workspace exists");
        let target = set.workspaces().nth(1).expect("second workspace");
        assert!(target.is_floating(w(1)));
        assert_eq!(target.float_of(w(1)), placed);
        assert!(!set.current().is_floating(w(1)));
    }

    #[test]
    fn toggling_floats_the_focused_window_and_tiles_it_again() {
        let mut set = set();
        set.toggle_float();
        assert!(set.workspaces().all(|ws| ws.floats().count() == 0));

        set.insert(w(1)).expect("new window");
        set.insert(w(2)).expect("new window");
        set.toggle_float();
        assert!(set.current().is_floating(w(2)));
        assert!(!set.current().is_floating(w(1)));

        set.toggle_float();
        assert!(!set.current().is_floating(w(2)));
    }

    #[test]
    fn a_tiled_window_ignores_float_changes() {
        let mut set = set();
        set.insert(w(1)).expect("new window");
        let before = set.clone();
        set.update_focused_float(|float| float.moved(5, 5));
        assert_eq!(set, before);
    }

    #[test]
    fn insert_on_fills_another_workspace_and_leaves_the_view_alone() {
        let mut set = set();
        set.insert(w(1)).expect("new window");
        set.insert_on(w(2), &tag("3")).expect("new window");

        assert_eq!(windows_of(&set, "3"), [2]);
        assert_eq!(set.current().tag(), &tag("1"));
        assert_eq!(set.focused(), Some(w(1)));
    }

    #[test]
    fn insert_on_refuses_a_managed_window_and_an_unknown_tag() {
        let mut set = set();
        set.insert(w(1)).expect("new window");
        assert_eq!(
            set.insert_on(w(1), &tag("2")),
            Err(Error::DuplicateWindow(w(1)))
        );
        assert_eq!(
            set.insert_on(w(2), &tag("9")),
            Err(Error::UnknownTag(tag("9")))
        );
        assert!(!set.contains(w(2)));
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

    fn two_screens() -> StackSet {
        let mut set = set();
        set.set_outputs(&[OutputId::new(0), OutputId::new(1)]);
        set
    }

    fn shown_on(set: &StackSet, output: u32) -> String {
        set.screens()
            .find(|(o, _)| o.get() == output)
            .map(|(_, w)| w.tag().to_string())
            .expect("the output has a screen")
    }

    #[test]
    fn viewing_a_workspace_another_screen_shows_moves_the_focus_there() {
        let mut s = two_screens();
        s.focus_tag(&tag("2")).expect("tag exists");
        assert_eq!(s.current_output(), OutputId::new(1));
        assert_eq!(shown_on(&s, 0), "1");
        assert_eq!(shown_on(&s, 1), "2");
    }

    #[test]
    fn viewing_a_hidden_workspace_replaces_only_the_focused_screens() {
        let mut s = two_screens();
        s.focus_tag(&tag("3")).expect("tag exists");
        assert_eq!(s.current_output(), OutputId::new(0));
        assert_eq!(shown_on(&s, 0), "3");
        assert_eq!(shown_on(&s, 1), "2");
    }

    #[test]
    fn the_focus_wraps_round_the_outputs_in_both_directions() {
        let mut s = two_screens();
        s.focus_next_output();
        assert_eq!(s.current_output(), OutputId::new(1));
        s.focus_next_output();
        assert_eq!(s.current_output(), OutputId::new(0));
        s.focus_prev_output();
        assert_eq!(s.current_output(), OutputId::new(1));
    }

    #[test]
    fn sending_a_window_to_the_next_output_lands_on_that_screens_workspace() {
        let mut s = two_screens();
        s.insert(w(1)).expect("new window");
        s.send_focused_to_next_output();
        assert_eq!(s.tag_of(w(1)), Some(&tag("2")));
        assert_eq!(s.current_output(), OutputId::new(0));
        assert_eq!(s.focused(), None);
    }

    #[test]
    fn with_one_screen_there_is_nowhere_to_send_a_window() {
        let mut s = set();
        s.insert(w(1)).expect("new window");
        let before = s.clone();
        s.send_focused_to_next_output();
        s.focus_next_output();
        assert_eq!(s, before);
    }

    #[test]
    fn focusing_a_window_on_another_screen_moves_the_focus_to_that_screen() {
        let mut s = two_screens();
        s.insert_on(w(4), &tag("2")).expect("tag exists");
        s.focus_window(w(4)).expect("window is managed");
        assert_eq!(s.current_output(), OutputId::new(1));
        assert_eq!(s.focused(), Some(w(4)));
        assert_eq!(shown_on(&s, 0), "1");
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
        SetOutputs(Vec<u32>),
        FocusNextOutput,
        FocusPrevOutput,
        SendToNextOutput,
        SendToPrevOutput,
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
            proptest::collection::vec(0u32..5, 0..6).prop_map(Op::SetOutputs),
            Just(Op::FocusNextOutput),
            Just(Op::FocusPrevOutput),
            Just(Op::SendToNextOutput),
            Just(Op::SendToPrevOutput),
        ]
    }

    fn shown_tags(set: &StackSet) -> Vec<(OutputId, Tag)> {
        set.screens().map(|(o, w)| (o, w.tag().clone())).collect()
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
            Op::SetOutputs(raw) => {
                let outputs: Vec<OutputId> = raw.iter().map(|n| OutputId::new(*n)).collect();
                let before = shown_tags(set);
                let focused = set.current().tag().clone();
                set.set_outputs(&outputs);
                let after = shown_tags(set);
                if outputs.is_empty() {
                    prop_assert_eq!(&after, &before);
                } else {
                    // The first distinct outputs, as many as there are workspaces.
                    let mut wanted: Vec<OutputId> = Vec::new();
                    for output in outputs {
                        if wanted.len() < all_tags.len() && !wanted.contains(&output) {
                            wanted.push(output);
                        }
                    }
                    prop_assert_eq!(after.iter().map(|(o, _)| *o).collect::<Vec<_>>(), wanted);
                    // An output that stays keeps its workspace.
                    for (output, tag) in &after {
                        if let Some((_, old)) = before.iter().find(|(o, _)| o == output) {
                            prop_assert_eq!(tag, old);
                        }
                    }
                    // The focused workspace stays focused while some screen shows it.
                    if after.iter().any(|(_, tag)| tag == &focused) {
                        prop_assert_eq!(set.current().tag(), &focused);
                    }
                }
            }
            Op::FocusNextOutput | Op::FocusPrevOutput => {
                let outputs: Vec<OutputId> = set.screens().map(|(o, _)| o).collect();
                let here = outputs
                    .iter()
                    .position(|o| *o == set.current_output())
                    .expect("the focused screen is listed");
                let forward = matches!(op, Op::FocusNextOutput);
                let expected = if forward {
                    (here + 1) % outputs.len()
                } else {
                    (here + outputs.len() - 1) % outputs.len()
                };
                let screens = shown_tags(set);
                if forward {
                    set.focus_next_output();
                } else {
                    set.focus_prev_output();
                }
                prop_assert_eq!(set.current_output(), outputs[expected]);
                // Only the focus moves: the screens show what they showed.
                prop_assert_eq!(shown_tags(set), screens);
            }
            Op::SendToNextOutput | Op::SendToPrevOutput => {
                let screens = shown_tags(set);
                let here = set.current_output();
                let target = {
                    let at = screens
                        .iter()
                        .position(|(o, _)| *o == here)
                        .expect("listed");
                    let n = screens.len();
                    let to = if matches!(op, Op::SendToNextOutput) {
                        at + 1
                    } else {
                        at + n - 1
                    };
                    screens[to % n].1.clone()
                };
                let moving = set.focused();
                if matches!(op, Op::SendToNextOutput) {
                    set.send_focused_to_next_output();
                } else {
                    set.send_focused_to_prev_output();
                }
                // The screens and the focused screen stay as they were.
                prop_assert_eq!(shown_tags(set), screens);
                prop_assert_eq!(set.current_output(), here);
                if let Some(id) = moving {
                    prop_assert_eq!(set.tag_of(id), Some(&target));
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

        // Each screen has its own output and its own workspace, and the focused screen
        // is one of them.
        let screens = shown_tags(set);
        prop_assert!(!screens.is_empty() && screens.len() <= tags().len());
        let outputs: HashSet<OutputId> = screens.iter().map(|(o, _)| *o).collect();
        let shown: HashSet<&Tag> = screens.iter().map(|(_, t)| t).collect();
        prop_assert_eq!(outputs.len(), screens.len());
        prop_assert_eq!(shown.len(), screens.len());
        prop_assert!(outputs.contains(&set.current_output()));
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
