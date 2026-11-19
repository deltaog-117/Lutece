//! The name a workspace holds to say which layout arranges it.

use crate::shared::error::{Error, Result};
use std::fmt;

/// The longest layout name, in bytes.
pub const MAX_LAYOUT_NAME_BYTES: usize = 32;

const EXTERNAL: &str = "external";
const FALLBACK: &str = "columns";

/// The name of a layout: a lowercase ASCII letter, then lowercase letters, digits and
/// hyphens, 32 bytes at most.
///
/// The strict shape keeps a name usable as one word in a configuration file and on a
/// command line, and means two names are equal only when they are the same text. A
/// workspace holds a name and not the layout itself, so a snapshot or a log can be read
/// back by an engine whose [`LayoutSet`](crate::LayoutSet) is another one: a name no layout
/// has is arranged by the fallback instead of failing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LayoutName(String);

impl LayoutName {
    /// Builds a layout name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLayoutName`] unless `name` has the shape described above.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::LayoutName;
    ///
    /// assert_eq!(LayoutName::try_new("spiral-2")?.as_str(), "spiral-2");
    /// assert!(LayoutName::try_new("Spiral").is_err());
    /// assert!(LayoutName::try_new("2up").is_err());
    /// assert!(LayoutName::try_new("").is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let mut bytes = name.bytes();
        let valid = name.len() <= MAX_LAYOUT_NAME_BYTES
            && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if valid {
            Ok(Self(name))
        } else {
            Err(Error::InvalidLayoutName)
        }
    }

    /// The name of the layout a plugin supplies. The engine asks for its rectangles
    /// instead of computing them, so no [`Layout`](crate::Layout) is registered under it.
    #[must_use]
    pub fn external() -> Self {
        Self(EXTERNAL.to_owned())
    }

    /// The name of the fallback layout every engine has.
    #[must_use]
    pub fn fallback() -> Self {
        Self(FALLBACK.to_owned())
    }

    /// Whether this is the name of the external layout.
    #[must_use]
    pub fn is_external(&self) -> bool {
        self.0 == EXTERNAL
    }

    /// Whether this is the name of the fallback layout.
    #[must_use]
    pub fn is_fallback(&self) -> bool {
        self.0 == FALLBACK
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for LayoutName {
    /// The fallback layout, the one an engine always has.
    fn default() -> Self {
        Self::fallback()
    }
}

impl fmt::Display for LayoutName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn shaped(text: &str) -> bool {
        // Spelled out, so the check does not repeat the code it checks.
        const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
        let mut chars = text.chars();
        text.len() <= MAX_LAYOUT_NAME_BYTES
            && chars.next().is_some_and(|c| LETTERS.contains(c))
            && chars.all(|c| LETTERS.contains(c) || "0123456789-".contains(c))
    }

    proptest! {
        #[test]
        fn a_name_is_accepted_exactly_when_it_has_the_shape(text in "\\PC{0,40}") {
            prop_assert_eq!(LayoutName::try_new(text.clone()).is_ok(), shaped(&text));
        }

        #[test]
        fn a_name_built_from_the_alphabet_is_accepted(text in "[a-z][a-z0-9-]{0,31}") {
            let name = LayoutName::try_new(text.clone()).expect("has the shape");
            prop_assert_eq!(name.as_str(), text.as_str());
        }
    }

    #[test]
    fn the_reserved_names_are_valid_and_tell_themselves_apart() {
        assert!(LayoutName::try_new("external").is_ok());
        assert!(LayoutName::external().is_external());
        assert!(!LayoutName::external().is_fallback());
        assert!(LayoutName::default().is_fallback());
    }
}
