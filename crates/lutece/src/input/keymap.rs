//! The table from key combinations to actions.

use super::binding::Action;
use crate::shared::contracts::event::KeyPress;
use crate::shared::error::{Error, Result};

/// Key combinations and the action each one triggers. A combination has at most one
/// action.
///
/// The keysym in a [`KeyPress`] is the key without Shift: Super+Shift+q is the keysym
/// of `q` with [`Modifiers::SUPER`](crate::Modifiers::SUPER) and
/// [`Modifiers::SHIFT`](crate::Modifiers::SHIFT), not the keysym of `Q`.
///
/// # Examples
///
/// ```
/// use lutece::{Action, KeyPress, Keymap, Keysym, Modifiers};
///
/// let quit = KeyPress { keysym: Keysym::from_char('q').expect("q is printable"), modifiers: Modifiers::SUPER };
/// let mut keymap = Keymap::new();
/// keymap.bind(quit, Action::Quit)?;
///
/// assert_eq!(keymap.action(quit), Some(&Action::Quit));
/// assert!(keymap.bind(quit, Action::Close).is_err());
/// # Ok::<(), lutece::Error>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keymap {
    /// In the order they were bound, so grabs happen in a stable order. A window manager
    /// has a few dozen bindings at most, so a scan beats a hash map.
    bindings: Vec<(KeyPress, Action)>,
}

impl Keymap {
    /// An empty keymap.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds `press` to `action`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateBinding`] if `press` is already bound.
    pub fn bind(&mut self, press: KeyPress, action: Action) -> Result<()> {
        if self.action(press).is_some() {
            return Err(Error::DuplicateBinding(press));
        }
        self.bindings.push((press, action));
        Ok(())
    }

    /// The action bound to `press`, if any.
    #[must_use]
    pub fn action(&self, press: KeyPress) -> Option<&Action> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == press)
            .map(|(_, action)| action)
    }

    /// Every bound key combination, in the order they were bound. These are the keys a
    /// backend has to grab.
    pub fn presses(&self) -> impl Iterator<Item = KeyPress> + '_ {
        self.bindings.iter().map(|(press, _)| *press)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::contracts::event::{Keysym, Modifiers};
    use proptest::prelude::*;

    fn press(keysym: u32, modifiers: u8) -> KeyPress {
        KeyPress {
            keysym: Keysym(keysym),
            modifiers: Modifiers::from_bits(modifiers).expect("generated within the defined bits"),
        }
    }

    proptest! {
        /// Whatever is bound, each combination finds its own action and nothing else
        /// does, and a second binding of the same combination is refused.
        #[test]
        fn each_press_finds_the_action_it_was_bound_to(
            keys in proptest::collection::vec((0u32..6, 0u8..16), 0..30),
        ) {
            let mut keymap = Keymap::new();
            let mut bound = Vec::new();
            for (n, (keysym, modifiers)) in keys.into_iter().enumerate() {
                let key = press(keysym, modifiers);
                let action = if n % 2 == 0 { Action::FocusNext } else { Action::FocusPrev };
                match keymap.bind(key, action.clone()) {
                    Ok(()) => {
                        prop_assert!(!bound.iter().any(|(k, _)| *k == key));
                        bound.push((key, action));
                    }
                    Err(err) => {
                        prop_assert_eq!(err, Error::DuplicateBinding(key));
                        prop_assert!(bound.iter().any(|(k, _)| *k == key));
                    }
                }
            }
            for (key, action) in &bound {
                prop_assert_eq!(keymap.action(*key), Some(action));
            }
            let listed: Vec<KeyPress> = keymap.presses().collect();
            let expected: Vec<KeyPress> = bound.iter().map(|(k, _)| *k).collect();
            prop_assert_eq!(listed, expected);
        }
    }

    #[test]
    fn an_unbound_press_has_no_action() {
        let mut keymap = Keymap::new();
        keymap
            .bind(press(0x71, 8), Action::Quit)
            .expect("first binding");
        assert_eq!(keymap.action(press(0x71, 0)), None);
        assert_eq!(keymap.action(press(0x72, 8)), None);
    }
}
