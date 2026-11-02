//! Smithay's view of the keyboard, in the core's terms.

use lutece::{KeyPress, Keysym, Modifiers};
use smithay::input::keyboard::{KeysymHandle, ModifiersState};

/// The core modifier set for the modifiers Smithay reports as held. Caps Lock and Num
/// Lock are left out so a binding works whichever lock is on.
pub(crate) fn modifiers(state: &ModifiersState) -> Modifiers {
    [
        (state.shift, Modifiers::SHIFT),
        (state.ctrl, Modifiers::CTRL),
        (state.alt, Modifiers::ALT),
        (state.logo, Modifiers::SUPER),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(Modifiers::NONE, |all, (_, modifier)| all | modifier)
}

/// The key combination a press stands for.
///
/// The keysym is the one the key produces with no modifier applied, so Shift+`q` is the
/// key `q` with Shift held, the same naming the X11 backend uses.
pub(crate) fn key_press(state: &ModifiersState, handle: &KeysymHandle<'_>) -> Option<KeyPress> {
    let keysym = handle.raw_syms().first().map(|sym| Keysym(sym.raw()))?;
    Some(KeyPress {
        keysym,
        modifiers: modifiers(state),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn modifiers_hold_exactly_the_keys_that_are_down(
            shift in any::<bool>(),
            ctrl in any::<bool>(),
            alt in any::<bool>(),
            logo in any::<bool>(),
            caps_lock in any::<bool>(),
            num_lock in any::<bool>(),
        ) {
            let state = ModifiersState {
                shift, ctrl, alt, logo, caps_lock, num_lock,
                ..ModifiersState::default()
            };
            let got = modifiers(&state);
            prop_assert_eq!(got.contains(Modifiers::SHIFT), shift);
            prop_assert_eq!(got.contains(Modifiers::CTRL), ctrl);
            prop_assert_eq!(got.contains(Modifiers::ALT), alt);
            prop_assert_eq!(got.contains(Modifiers::SUPER), logo);
        }
    }
}
