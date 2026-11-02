//! Matching the screens to the outputs a backend reports.

use super::stackset::{Screen, StackSet};
use super::workspace::Workspace;
use crate::shared::ids::OutputId;
use std::collections::HashMap;
use std::iter;

impl StackSet {
    /// Shows workspaces on exactly these outputs, in this order, when the backend
    /// reports that the screens changed.
    ///
    /// An output that already has a screen keeps its workspace. A new output gets a
    /// workspace nobody shows: first those that vanished outputs showed, so replacing a
    /// monitor keeps its workspace, then the hidden ones in tag order. Windows never
    /// move. A workspace whose output vanished and that no new output takes is hidden,
    /// with its windows, and comes back when a screen shows it again.
    ///
    /// The focused workspace stays focused while some screen shows it; otherwise the
    /// first screen gets the focus.
    ///
    /// A workspace cannot be shown twice, so outputs past the number of workspaces get no
    /// screen, and an output listed twice counts once. An empty list changes nothing: a
    /// backend that loses every output keeps the screens it had for when one returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{OutputId, StackSet, Tag};
    ///
    /// let mut set = StackSet::new(["1", "2", "3"].map(|t| Tag::try_new(t).unwrap()))?;
    /// set.set_outputs(&[OutputId::new(0), OutputId::new(1)]);
    ///
    /// let shown: Vec<_> = set.screens().map(|(_, ws)| ws.tag().as_str()).collect();
    /// assert_eq!(shown, ["1", "2"]);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn set_outputs(&mut self, outputs: &[OutputId]) {
        let capacity = self.workspace_count();
        let mut wanted: Vec<OutputId> = Vec::with_capacity(outputs.len().min(capacity));
        for &output in outputs {
            if wanted.len() < capacity && !wanted.contains(&output) {
                wanted.push(output);
            }
        }
        if wanted.is_empty() {
            return;
        }

        // Rebuilt from a copy and swapped in at the end, so that no early exit can leave
        // a workspace out of the set. Screens change rarely, so the copy costs nothing
        // that matters.
        let Self {
            before,
            current,
            after,
            mut hidden,
        } = self.clone();
        let focused = current.workspace.tag().clone();

        let mut kept: HashMap<OutputId, Workspace> = HashMap::new();
        let mut orphans = Vec::new();
        for screen in before.into_iter().chain(iter::once(current)).chain(after) {
            if wanted.contains(&screen.output) {
                kept.insert(screen.output, screen.workspace);
            } else {
                orphans.push(screen.workspace);
            }
        }
        hidden.sort_by_key(Workspace::rank);

        let mut spare = orphans.into_iter().chain(hidden);
        let mut screens = Vec::with_capacity(wanted.len());
        for output in wanted {
            // `wanted` is capped at the number of workspaces, so a new output always
            // finds a spare one.
            let Some(workspace) = kept.remove(&output).or_else(|| spare.next()) else {
                continue;
            };
            screens.push(Screen { output, workspace });
        }
        let hidden: Vec<Workspace> = spare.collect();

        let index = screens
            .iter()
            .position(|screen| screen.workspace.tag() == &focused)
            .unwrap_or(0);
        let mut right = screens.split_off(index).into_iter();
        let Some(current) = right.next() else {
            return;
        };
        *self = Self {
            before: screens,
            current,
            after: right.collect(),
            hidden,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::{Tag, WinId};

    fn out(raw: u32) -> OutputId {
        OutputId::new(raw)
    }

    fn tag(name: &str) -> Tag {
        Tag::try_new(name).expect("test tags have text")
    }

    fn set() -> StackSet {
        StackSet::new(["1", "2", "3"].map(tag)).expect("three distinct tags")
    }

    /// The tag each screen shows, in screen order.
    fn shown(set: &StackSet) -> Vec<(u32, String)> {
        set.screens()
            .map(|(output, ws)| (output.get(), ws.tag().to_string()))
            .collect()
    }

    #[test]
    fn a_new_output_shows_the_first_hidden_workspace() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        assert_eq!(shown(&set), [(0, "1".into()), (1, "2".into())]);
        assert_eq!(set.current_output(), out(0));
    }

    #[test]
    fn the_first_report_keeps_the_current_workspace_on_a_renamed_output() {
        let mut set = set();
        set.focus_tag(&tag("3")).expect("workspace exists");
        set.set_outputs(&[out(7)]);
        assert_eq!(shown(&set), [(7, "3".into())]);
        assert_eq!(set.current().tag(), &tag("3"));
    }

    #[test]
    fn an_unplugged_output_hides_its_workspace_and_keeps_its_windows() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        set.focus_next_output();
        set.insert(WinId::new(5)).expect("new window");

        set.set_outputs(&[out(0)]);

        assert_eq!(shown(&set), [(0, "1".into())]);
        assert_eq!(set.tag_of(WinId::new(5)), Some(&tag("2")));
        assert!(set.contains(WinId::new(5)));
    }

    #[test]
    fn a_returning_output_shows_the_workspace_that_was_hidden() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        set.set_outputs(&[out(0)]);
        set.set_outputs(&[out(0), out(1)]);
        assert_eq!(shown(&set), [(0, "1".into()), (1, "2".into())]);
    }

    #[test]
    fn a_replaced_output_takes_over_the_workspace_of_the_one_it_replaces() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        set.focus_next_output();

        set.set_outputs(&[out(0), out(9)]);

        assert_eq!(shown(&set), [(0, "1".into()), (9, "2".into())]);
        assert_eq!(set.current_output(), out(9));
    }

    #[test]
    fn the_focus_stays_on_its_workspace_when_the_screens_reorder() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        set.focus_next_output();

        set.set_outputs(&[out(1), out(0)]);

        assert_eq!(shown(&set), [(1, "2".into()), (0, "1".into())]);
        assert_eq!(set.current().tag(), &tag("2"));
    }

    #[test]
    fn focus_goes_to_the_first_screen_when_the_focused_workspace_is_hidden() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        set.focus_next_output();

        set.set_outputs(&[out(0)]);

        assert_eq!(set.current_output(), out(0));
    }

    #[test]
    fn outputs_past_the_number_of_workspaces_and_repeats_get_no_screen() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1), out(1), out(2), out(3)]);
        assert_eq!(
            shown(&set),
            [(0, "1".into()), (1, "2".into()), (2, "3".into())]
        );
    }

    #[test]
    fn an_empty_report_changes_nothing() {
        let mut set = set();
        set.set_outputs(&[out(0), out(1)]);
        let before = set.clone();
        set.set_outputs(&[]);
        assert_eq!(set, before);
    }
}
