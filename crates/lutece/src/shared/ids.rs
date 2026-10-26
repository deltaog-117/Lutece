//! Opaque identifiers handed out by a backend, and the names users give workspaces.
//!
//! The core never interprets the number inside a window or output id: an X11 backend
//! wraps a window id, a Wayland backend wraps a counter. Newtypes keep a window id
//! from being used as an output id.

use crate::shared::error::{Error, Result};
use std::fmt;

/// Identifies one client window for as long as it exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WinId(u64);

impl WinId {
    /// Wraps a backend-specific window number.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the backend-specific window number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies one screen (monitor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputId(u32);

impl OutputId {
    /// Wraps a backend-specific output number.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the backend-specific output number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The name of a workspace. Never empty or blank.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Tag(String);

impl Tag {
    /// Builds a tag.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyTag`] if `name` is empty or only whitespace.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::Tag;
    ///
    /// assert_eq!(Tag::try_new("web")?.as_str(), "web");
    /// assert!(Tag::try_new("  ").is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Error::EmptyTag);
        }
        Ok(Self(name))
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn win_id_round_trips(raw in any::<u64>()) {
            prop_assert_eq!(WinId::new(raw).get(), raw);
        }

        #[test]
        fn output_id_round_trips(raw in any::<u32>()) {
            prop_assert_eq!(OutputId::new(raw).get(), raw);
        }

        #[test]
        fn tag_accepts_exactly_the_names_with_visible_text(name in ".{0,12}") {
            let accepted = Tag::try_new(name.clone()).is_ok();
            prop_assert_eq!(accepted, !name.trim().is_empty());
        }
    }
}
