//! Things that happened, as reported by a backend.

use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::{OutputId, WinId};
use std::ops::BitOr;

/// Something a backend tells the core about. Only events with the same meaning on X11
/// and Wayland exist; anything backend-specific is handled inside that backend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Event {
    /// A client window asked to be shown and is ready to be managed.
    WindowOpened {
        /// The new window.
        id: WinId,
        /// What the backend knows about it.
        info: WindowInfo,
    },
    /// A client window is gone.
    WindowClosed(WinId),
    /// A window asked for focus, or the pointer entered it.
    FocusRequested(WinId),
    /// A key combination the core asked the backend to grab was pressed.
    KeyPressed(KeyPress),
    /// The set of screens changed; this is the complete new set.
    OutputsChanged(Vec<Output>),
}

/// Window details a layout or rule may need.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct WindowInfo {
    /// The window title.
    pub title: String,
    /// The application class (X11 `WM_CLASS`) or app id (Wayland).
    pub class: String,
    /// The window this one is a dialog or popup for, if any.
    pub transient_for: Option<WinId>,
}

/// One screen and the area it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Output {
    /// The screen's identifier.
    pub id: OutputId,
    /// The area the screen covers.
    pub rect: Rect,
}

/// A key symbol. X11 and xkbcommon (Wayland) share the same numbering, so one type
/// serves both backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Keysym(pub u32);

impl Keysym {
    /// The Return (Enter) key.
    pub const RETURN: Self = Self(0xff0d);

    /// The keysym of a printable Latin-1 character, such as `'q'` or `'1'`. For these
    /// characters the keysym number is the character's code point. The lowercase letter
    /// names the key; Shift is a modifier.
    ///
    /// Returns `None` for control characters and anything beyond Latin-1.
    #[must_use]
    pub const fn from_char(c: char) -> Option<Self> {
        match c as u32 {
            raw @ (0x20..=0x7e | 0xa0..=0xff) => Some(Self(raw)),
            _ => None,
        }
    }
}

/// Modifier keys held during a key press. Only defined bits can be set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(1);
    /// Control.
    pub const CTRL: Self = Self(1 << 1);
    /// Alt (Mod1 on X11).
    pub const ALT: Self = Self(1 << 2);
    /// The logo key (Mod4 on X11).
    pub const SUPER: Self = Self(1 << 3);

    const ALL_BITS: u8 = 0b1111;

    /// Builds a set from raw bits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownModifierBits`] if any undefined bit is set.
    pub const fn from_bits(bits: u8) -> Result<Self> {
        if bits & !Self::ALL_BITS == 0 {
            Ok(Self(bits))
        } else {
            Err(Error::UnknownModifierBits(bits))
        }
    }

    /// The raw bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether every modifier in `other` is also in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A key press: which key, with which modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct KeyPress {
    /// The key.
    pub keysym: Keysym,
    /// The modifiers held.
    pub modifiers: Modifiers,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn modifiers_accept_exactly_the_defined_bits(bits in any::<u8>()) {
            prop_assert_eq!(Modifiers::from_bits(bits).is_ok(), bits & !0b1111 == 0);
        }

        #[test]
        fn a_union_contains_both_of_its_parts(a in 0u8..16, b in 0u8..16) {
            let a = Modifiers::from_bits(a).expect("generated within the defined bits");
            let b = Modifiers::from_bits(b).expect("generated within the defined bits");
            let both = a | b;
            prop_assert!(both.contains(a));
            prop_assert!(both.contains(b));
        }
    }

    proptest! {
        #[test]
        fn from_char_accepts_exactly_the_printable_latin_1(c in any::<char>()) {
            let printable = matches!(c as u32, 0x20..=0x7e | 0xa0..=0xff);
            prop_assert_eq!(Keysym::from_char(c).is_some(), printable);
            if let Some(keysym) = Keysym::from_char(c) {
                prop_assert_eq!(keysym.0, c as u32);
            }
        }
    }

    #[test]
    fn nothing_contains_a_modifier_it_lacks() {
        assert!(!Modifiers::SHIFT.contains(Modifiers::SUPER));
        assert!(Modifiers::NONE.contains(Modifiers::NONE));
    }
}
