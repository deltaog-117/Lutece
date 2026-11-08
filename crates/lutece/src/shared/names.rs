//! Names a plugin chooses, checked once where they are built: the name of a custom
//! command, and the key and value of a property.
//!
//! The limits keep a plugin from filling the core's memory or the recorded log with one
//! message, and keep a name usable as one word on a command line.

use crate::shared::error::{Error, Result};
use std::fmt;

/// The longest command name or property key, in bytes.
pub const MAX_NAME_BYTES: usize = 64;

/// The longest property value, in bytes.
pub const MAX_PROPERTY_VALUE_BYTES: usize = 1024;

/// How many properties one window or workspace holds. A set beyond it is ignored, so a
/// plugin that invents keys without end cannot grow the core without bound.
pub const MAX_PROPERTIES_PER_TARGET: usize = 32;

/// Whether `text` is one to [`MAX_NAME_BYTES`] visible ASCII characters: no space, no
/// control character.
fn is_word(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_NAME_BYTES
        && text.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The name of a custom command: what a key bound by a plugin triggers, and what the
/// plugin is told when it happens. One to 64 visible ASCII characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CommandName(String);

impl CommandName {
    /// Builds a command name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCommandName`] if `name` is empty, longer than 64 bytes, or
    /// has a space, a control character or a character outside ASCII.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::CommandName;
    ///
    /// assert_eq!(CommandName::try_new("scratchpad.toggle")?.as_str(), "scratchpad.toggle");
    /// assert!(CommandName::try_new("two words").is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if is_word(&name) {
            Ok(Self(name))
        } else {
            Err(Error::InvalidCommandName)
        }
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The key of a property. One to 64 visible ASCII characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PropertyKey(String);

impl PropertyKey {
    /// Builds a property key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPropertyKey`] under the same rules as
    /// [`CommandName::try_new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::PropertyKey;
    ///
    /// assert!(PropertyKey::try_new("bar.hidden").is_ok());
    /// assert!(PropertyKey::try_new("").is_err());
    /// ```
    pub fn try_new(key: impl Into<String>) -> Result<Self> {
        let key = key.into();
        if is_word(&key) {
            Ok(Self(key))
        } else {
            Err(Error::InvalidPropertyKey)
        }
    }

    /// The key as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The value of a property: any text up to 1024 bytes. The empty text is a value;
/// removing a property is done by setting it to nothing, not to the empty text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PropertyValue(String);

impl PropertyValue {
    /// Builds a property value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PropertyValueTooLong`] if `value` is over 1024 bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::PropertyValue;
    ///
    /// assert_eq!(PropertyValue::try_new("any text")?.as_str(), "any text");
    /// assert!(PropertyValue::try_new("x".repeat(2000)).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.len() > MAX_PROPERTY_VALUE_BYTES {
            return Err(Error::PropertyValueTooLong {
                len: value.len(),
                max: MAX_PROPERTY_VALUE_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// The value as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn a_name_is_accepted_exactly_when_it_is_one_visible_word(text in ".{0,80}") {
            let expected = !text.is_empty()
                && text.len() <= MAX_NAME_BYTES
                && text.chars().all(|c| c.is_ascii_graphic());
            prop_assert_eq!(CommandName::try_new(text.clone()).is_ok(), expected);
            prop_assert_eq!(PropertyKey::try_new(text).is_ok(), expected);
        }

        #[test]
        fn a_value_is_accepted_up_to_the_limit(len in 0usize..2048) {
            let accepted = PropertyValue::try_new("x".repeat(len)).is_ok();
            prop_assert_eq!(accepted, len <= MAX_PROPERTY_VALUE_BYTES);
        }
    }
}
