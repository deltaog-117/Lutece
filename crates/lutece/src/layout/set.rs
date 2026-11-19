//! The layouts an engine can arrange a workspace with, by name.

use super::{Columns, Layout, LayoutName};
use crate::shared::error::{Error, Result};
use std::sync::Arc;

/// The layouts registered on an engine, in the order the next-layout action cycles
/// through them.
///
/// The set always has the fallback ([`Columns`], named `columns`): the engine cannot be
/// left without a layout. It takes part in the cycle only while nothing else is
/// registered, so registering the built-in layouts of `lutece-layouts` gives a cycle of
/// exactly those. Two sets are equal when they list the same names in the same order; a
/// layout is identified by its name, and the engine's state cannot depend on code.
///
/// # Examples
///
/// ```
/// use lutece::{LayoutName, LayoutSet};
///
/// let set = LayoutSet::new();
/// assert_eq!(set.first(), LayoutName::fallback());
/// assert!(set.get(&LayoutName::try_new("spiral")?).is_none());
/// # Ok::<(), lutece::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct LayoutSet {
    entries: Vec<(LayoutName, Arc<dyn Layout>)>,
    fallback: Arc<dyn Layout>,
}

impl Default for LayoutSet {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for LayoutSet {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(&other.entries)
                .all(|((a, _), (b, _))| a == b)
    }
}

impl Eq for LayoutSet {}

impl LayoutSet {
    /// A set with the fallback only.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            fallback: Arc::new(Columns),
        }
    }

    /// Registers `layout` under `name`, after the ones already there.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LayoutNameTaken`] if a layout has this name, or if it is one the
    /// engine keeps for itself: `columns` is the fallback and `external` is a plugin's.
    pub fn register(&mut self, name: LayoutName, layout: Box<dyn Layout>) -> Result<()> {
        if name.is_external() || name.is_fallback() || self.entries.iter().any(|(n, _)| *n == name)
        {
            return Err(Error::LayoutNameTaken(name.to_string()));
        }
        self.entries.push((name, Arc::from(layout)));
        Ok(())
    }

    /// [`register`](Self::register) in builder form.
    ///
    /// # Errors
    ///
    /// As [`register`](Self::register).
    pub fn with(mut self, name: LayoutName, layout: Box<dyn Layout>) -> Result<Self> {
        self.register(name, layout)?;
        Ok(self)
    }

    /// The layout named `name`: a registered one, or the fallback under its own name.
    /// `None` for any other name, which includes `external`.
    #[must_use]
    pub fn get(&self, name: &LayoutName) -> Option<&dyn Layout> {
        if name.is_fallback() {
            return Some(self.fallback.as_ref());
        }
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, layout)| layout.as_ref())
    }

    /// The layout to arrange with for `name`: [`get`](Self::get), or the fallback when no
    /// layout has that name, so a workspace is always arranged.
    #[must_use]
    pub fn resolve(&self, name: &LayoutName) -> &dyn Layout {
        self.get(name).unwrap_or(self.fallback.as_ref())
    }

    /// The names the next-layout action cycles through, in order.
    #[must_use]
    pub fn names(&self) -> Vec<LayoutName> {
        if self.entries.is_empty() {
            vec![LayoutName::fallback()]
        } else {
            self.entries.iter().map(|(name, _)| name.clone()).collect()
        }
    }

    /// The first layout of the cycle: the one a workspace returns to when it leaves the
    /// external layout.
    #[must_use]
    pub fn first(&self) -> LayoutName {
        self.entries
            .first()
            .map_or_else(LayoutName::fallback, |(name, _)| name.clone())
    }

    /// The layout after `current` in the cycle, wrapping round. A name that is not in the
    /// cycle (the external layout, or a layout this set does not have) gives
    /// [`first`](Self::first).
    #[must_use]
    pub fn next_after(&self, current: &LayoutName) -> LayoutName {
        let names = self.names();
        names
            .iter()
            .position(|name| name == current)
            .map_or_else(|| self.first(), |at| names[(at + 1) % names.len()].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::MasterRatio;
    use crate::shared::contracts::scene::Placement;
    use crate::shared::geometry::Rect;
    use crate::shared::ids::WinId;
    use proptest::prelude::*;

    #[derive(Debug)]
    struct Nothing;

    impl Layout for Nothing {
        fn arrange(
            &self,
            _area: Rect,
            _windows: &[WinId],
            _focus: Option<WinId>,
            _ratio: MasterRatio,
        ) -> Result<Vec<Placement>> {
            Ok(Vec::new())
        }
    }

    fn name(text: &str) -> LayoutName {
        LayoutName::try_new(text).expect("valid name")
    }

    fn set_of(count: usize) -> LayoutSet {
        (0..count).fold(LayoutSet::new(), |set, n| {
            set.with(name(&format!("l{n}")), Box::new(Nothing))
                .expect("distinct names")
        })
    }

    proptest! {
        #[test]
        fn the_cycle_visits_every_name_and_comes_back(count in 0_usize..12) {
            let set = set_of(count);
            let names = set.names();
            prop_assert_eq!(names.len(), count.max(1));
            let mut at = set.first();
            let mut seen = Vec::new();
            for _ in 0..names.len() {
                seen.push(at.clone());
                at = set.next_after(&at);
            }
            prop_assert_eq!(at, set.first());
            prop_assert_eq!(seen, names);
        }

        #[test]
        fn resolving_never_fails_whatever_the_name(count in 0_usize..6, text in "[a-z][a-z0-9-]{0,8}") {
            let set = set_of(count);
            // The result is a layout in every case; asking for its arrangement must not panic.
            let area = Rect::new(0, 0, 100, 100).expect("non-zero size");
            let layout = set.resolve(&name(&text));
            prop_assert!(layout.arrange(area, &[], None, MasterRatio::default()).is_ok());
        }
    }

    #[test]
    fn a_taken_or_reserved_name_is_refused() {
        let mut set = set_of(1);
        for taken in ["l0", "columns", "external"] {
            let err = set
                .register(name(taken), Box::new(Nothing))
                .expect_err("taken");
            assert_eq!(err, Error::LayoutNameTaken(taken.to_owned()));
        }
        assert_eq!(set.names(), [name("l0")]);
    }

    #[test]
    fn a_set_with_nothing_registered_cycles_on_the_fallback() {
        let set = LayoutSet::new();
        assert_eq!(
            set.next_after(&LayoutName::fallback()),
            LayoutName::fallback()
        );
        assert_eq!(
            set.next_after(&LayoutName::external()),
            LayoutName::fallback()
        );
    }

    #[test]
    fn the_fallback_is_known_under_its_own_name_only() {
        let set = set_of(2);
        assert!(set.get(&LayoutName::fallback()).is_some());
        assert!(set.get(&LayoutName::external()).is_none());
        assert!(set.get(&name("missing")).is_none());
        assert!(!set.names().contains(&LayoutName::fallback()));
    }

    #[test]
    fn sets_with_the_same_names_are_equal() {
        assert_eq!(set_of(3), set_of(3));
        assert_ne!(set_of(3), set_of(2));
    }
}
