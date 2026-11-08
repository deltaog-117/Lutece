//! What the backend remembers about client windows between X events.
//!
//! X reports a window we unmapped ourselves exactly like one the client withdrew. Telling
//! them apart needs this bookkeeping, and both [`translate`](crate::translate) and
//! [`apply`](crate::apply) read and write it.

use lutece::WindowInfo;
use std::collections::{HashMap, HashSet};
use x11rb::protocol::xproto::Window;

/// The windows we manage, which of them are on screen, and the unmap events our own
/// hides will still produce.
#[derive(Debug, Default)]
pub(crate) struct Windows {
    managed: HashSet<Window>,
    mapped: HashSet<Window>,
    expected_unmaps: HashMap<Window, u32>,
    /// The windows whose `_NET_WM_STATE` says fullscreen because we put it there.
    fullscreen: HashSet<Window>,
    /// What the core was last told about each window's title and class, so a property change
    /// that leaves them as they were is not reported.
    infos: HashMap<Window, WindowInfo>,
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
        self.fullscreen.remove(&window);
        self.infos.remove(&window);
        self.managed.remove(&window)
    }

    /// Records that `window` is, or is no longer, shown as fullscreen. `true` if that
    /// changed what we had recorded, which is when its property needs writing.
    pub(crate) fn set_fullscreen(&mut self, window: Window, on: bool) -> bool {
        if on {
            self.fullscreen.insert(window)
        } else {
            self.fullscreen.remove(&window)
        }
    }

    /// Records what the core now knows about `window`. `true` if that differs from what it
    /// knew before, which is when a change is worth reporting.
    pub(crate) fn note_info(&mut self, window: Window, info: WindowInfo) -> bool {
        self.infos.insert(window, info.clone()).as_ref() != Some(&info)
    }

    pub(crate) fn is_fullscreen(&self, window: Window) -> bool {
        self.fullscreen.contains(&window)
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
    fn fullscreen_is_recorded_once_and_forgotten_with_the_window() {
        let mut windows = Windows::default();
        windows.manage(7);
        assert!(windows.set_fullscreen(7, true));
        assert!(!windows.set_fullscreen(7, true));
        assert!(windows.is_fullscreen(7));

        windows.forget(7);
        assert!(!windows.is_fullscreen(7));
        assert!(!windows.set_fullscreen(7, false));
    }

    fn titled(title: &str) -> WindowInfo {
        WindowInfo {
            title: title.to_owned(),
            ..WindowInfo::default()
        }
    }

    #[test]
    fn a_description_is_news_only_when_it_differs_from_the_last() {
        let mut windows = Windows::default();
        windows.manage(7);
        assert!(windows.note_info(7, titled("a")));
        assert!(!windows.note_info(7, titled("a")));
        assert!(windows.note_info(7, titled("b")));
        assert!(windows.note_info(7, titled("a")));
    }

    #[test]
    fn a_forgotten_window_starts_without_a_description() {
        let mut windows = Windows::default();
        windows.manage(7);
        windows.note_info(7, titled("a"));
        windows.forget(7);
        assert!(windows.note_info(7, titled("a")));
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
