//! What the backend remembers about client windows between X events.
//!
//! X reports a window we unmapped ourselves exactly like one the client withdrew. Telling
//! them apart needs this bookkeeping, and both [`translate`](crate::translate) and
//! [`apply`](crate::apply) read and write it.

use std::collections::{HashMap, HashSet};
use x11rb::protocol::xproto::Window;

/// The windows we manage, which of them are on screen, and the unmap events our own
/// hides will still produce.
#[derive(Debug, Default)]
pub(crate) struct Windows {
    managed: HashSet<Window>,
    mapped: HashSet<Window>,
    expected_unmaps: HashMap<Window, u32>,
}

impl Windows {
    /// Starts managing `window`; `false` if it already was.
    pub(crate) fn manage(&mut self, window: Window) -> bool {
        self.managed.insert(window)
    }

    pub(crate) fn is_managed(&self, window: Window) -> bool {
        self.managed.contains(&window)
    }

    /// Stops managing `window` and drops everything known about it, so a reused X id
    /// starts clean. `false` if it was not managed.
    pub(crate) fn forget(&mut self, window: Window) -> bool {
        self.mapped.remove(&window);
        self.expected_unmaps.remove(&window);
        self.managed.remove(&window)
    }

    pub(crate) fn mark_mapped(&mut self, window: Window) {
        self.mapped.insert(window);
    }

    /// Records that we are about to unmap `window`. `false` if it was not on screen, in
    /// which case there is nothing to unmap.
    pub(crate) fn mark_hidden(&mut self, window: Window) -> bool {
        if !self.mapped.remove(&window) {
            return false;
        }
        *self.expected_unmaps.entry(window).or_default() += 1;
        true
    }

    /// Consumes one expected unmap event; `false` if the unmap came from the client.
    pub(crate) fn take_expected_unmap(&mut self, window: Window) -> bool {
        let Some(count) = self.expected_unmaps.get_mut(&window) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.expected_unmaps.remove(&window);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hide_is_expected_once_per_unmap() {
        let mut windows = Windows::default();
        windows.manage(7);
        windows.mark_mapped(7);
        assert!(windows.mark_hidden(7));
        assert!(windows.take_expected_unmap(7));
        assert!(!windows.take_expected_unmap(7));
    }

    #[test]
    fn hiding_a_window_that_is_not_on_screen_needs_no_unmap() {
        let mut windows = Windows::default();
        windows.manage(7);
        assert!(!windows.mark_hidden(7));
        assert!(!windows.take_expected_unmap(7));
    }

    #[test]
    fn forgetting_a_window_drops_its_pending_unmaps() {
        let mut windows = Windows::default();
        windows.manage(7);
        windows.mark_mapped(7);
        windows.mark_hidden(7);
        assert!(windows.forget(7));
        assert!(!windows.take_expected_unmap(7));
        assert!(!windows.is_managed(7));
    }
}
