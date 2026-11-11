//! The keys the session keeps for itself, whatever the window manager is doing.
//!
//! On a screen of its own there is no window system to fall back on: a window manager that
//! hangs, or whose configuration has no key to quit, would leave a machine with a picture
//! and no way out. So two chords are handled before any key reaches the core or a client:
//! Ctrl+Alt+Backspace ends the session, and Ctrl+Alt+F1 to F12 switch to that virtual
//! terminal. They are the same ones X servers have long used, so they are already in the
//! fingers.

/// Keycodes as the backend receives them: the kernel's key numbers plus 8, which is what the
/// keyboard layout library counts in.
const OFFSET: u32 = 8;
const LEFT_CTRL: u32 = 29 + OFFSET;
const RIGHT_CTRL: u32 = 97 + OFFSET;
const LEFT_ALT: u32 = 56 + OFFSET;
const RIGHT_ALT: u32 = 100 + OFFSET;
const BACKSPACE: u32 = 14 + OFFSET;
/// F1 to F10 are consecutive keys; F11 and F12 sit apart.
const F1: u32 = 59 + OFFSET;
const F10: u32 = 68 + OFFSET;
const F11: u32 = 87 + OFFSET;
const F12: u32 = 88 + OFFSET;

/// What a chord asks the session to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chord {
    /// End the session.
    Quit,
    /// Switch to the virtual terminal with this number, from 1.
    SwitchVt(i32),
}

/// What to do with a key that was pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Not the session's: hand it on as usual.
    Pass,
    /// The release of a key whose press was a chord: nobody saw the press, so nobody sees
    /// this.
    Swallow,
    /// A chord: do what it says, and hand the key on to nobody.
    Chord(Chord),
}

/// Which modifiers are down, and which chord keys were taken, so a release is matched with
/// its press.
#[derive(Debug, Default)]
pub(crate) struct Chords {
    ctrl: [bool; 2],
    alt: [bool; 2],
    swallowed: Vec<u32>,
}

impl Chords {
    /// Takes note of a key going down (`pressed`) or up and says what to do with it.
    pub(crate) fn key(&mut self, code: u32, pressed: bool) -> Verdict {
        match code {
            LEFT_CTRL => self.ctrl[0] = pressed,
            RIGHT_CTRL => self.ctrl[1] = pressed,
            LEFT_ALT => self.alt[0] = pressed,
            RIGHT_ALT => self.alt[1] = pressed,
            _ => {}
        }
        if !pressed {
            return match self.swallowed.iter().position(|taken| *taken == code) {
                Some(at) => {
                    self.swallowed.swap_remove(at);
                    Verdict::Swallow
                }
                None => Verdict::Pass,
            };
        }
        if !(self.ctrl.contains(&true) && self.alt.contains(&true)) {
            return Verdict::Pass;
        }
        let chord = match code {
            BACKSPACE => Chord::Quit,
            F1..=F10 => Chord::SwitchVt(vt(code - F1 + 1)),
            F11 => Chord::SwitchVt(11),
            F12 => Chord::SwitchVt(12),
            _ => return Verdict::Pass,
        };
        if !self.swallowed.contains(&code) {
            self.swallowed.push(code);
        }
        Verdict::Chord(chord)
    }

    /// Forgets every key, for when the session is paused: the releases happen while nobody is
    /// listening, so a modifier would otherwise stay down for good.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A virtual terminal number from a position in the F1 to F10 run, which is at most ten.
fn vt(number: u32) -> i32 {
    i32::try_from(number).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn hold(chords: &mut Chords, keys: &[u32]) {
        for key in keys {
            let _ = chords.key(*key, true);
        }
    }

    #[test]
    fn ctrl_alt_backspace_quits() {
        let mut chords = Chords::default();
        hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);

        assert_eq!(chords.key(BACKSPACE, true), Verdict::Chord(Chord::Quit));
    }

    #[test]
    fn either_ctrl_and_either_alt_will_do() {
        let mut chords = Chords::default();
        hold(&mut chords, &[RIGHT_CTRL, RIGHT_ALT]);

        assert_eq!(chords.key(BACKSPACE, true), Verdict::Chord(Chord::Quit));
    }

    #[test]
    fn the_function_keys_switch_to_their_own_terminal() {
        let mut chords = Chords::default();
        hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);

        assert_eq!(chords.key(F1, true), Verdict::Chord(Chord::SwitchVt(1)));
        assert_eq!(chords.key(F1 + 2, true), Verdict::Chord(Chord::SwitchVt(3)));
        assert_eq!(chords.key(F10, true), Verdict::Chord(Chord::SwitchVt(10)));
        assert_eq!(chords.key(F11, true), Verdict::Chord(Chord::SwitchVt(11)));
        assert_eq!(chords.key(F12, true), Verdict::Chord(Chord::SwitchVt(12)));
    }

    #[test]
    fn a_chord_key_without_both_modifiers_is_an_ordinary_key() {
        let mut chords = Chords::default();
        assert_eq!(chords.key(BACKSPACE, true), Verdict::Pass);

        hold(&mut chords, &[LEFT_CTRL]);
        assert_eq!(chords.key(BACKSPACE, true), Verdict::Pass);
        assert_eq!(chords.key(F1, true), Verdict::Pass);
    }

    #[test]
    fn the_release_of_a_chord_key_is_swallowed_once() {
        let mut chords = Chords::default();
        hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);
        let _ = chords.key(BACKSPACE, true);

        assert_eq!(chords.key(BACKSPACE, false), Verdict::Swallow);
        assert_eq!(chords.key(BACKSPACE, false), Verdict::Pass);
    }

    #[test]
    fn a_modifier_let_go_ends_the_chord() {
        let mut chords = Chords::default();
        hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);
        let _ = chords.key(LEFT_ALT, false);

        assert_eq!(chords.key(BACKSPACE, true), Verdict::Pass);
    }

    #[test]
    fn forgetting_the_keys_lets_go_of_the_modifiers() {
        let mut chords = Chords::default();
        hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);

        chords.reset();

        assert_eq!(chords.key(BACKSPACE, true), Verdict::Pass);
    }

    proptest! {
        /// Without a Ctrl and an Alt down, no key is ever a chord, whatever else is pressed.
        #[test]
        fn nothing_is_a_chord_without_both_modifiers(
            keys in proptest::collection::vec((1u32..300, any::<bool>()), 0..60),
        ) {
            let mut chords = Chords::default();
            for (code, pressed) in keys {
                if [LEFT_CTRL, RIGHT_CTRL, LEFT_ALT, RIGHT_ALT].contains(&code) {
                    continue;
                }
                prop_assert!(!matches!(chords.key(code, pressed), Verdict::Chord(_)));
            }
        }

        /// A key that is not part of a chord is never held back, so typing is never lost.
        #[test]
        fn an_ordinary_key_always_passes(code in 1u32..300, pressed in any::<bool>()) {
            prop_assume!(![BACKSPACE, F11, F12].contains(&code) && !(F1..=F10).contains(&code));
            let mut chords = Chords::default();
            hold(&mut chords, &[LEFT_CTRL, LEFT_ALT]);

            prop_assert_eq!(chords.key(code, pressed), Verdict::Pass);
        }
    }
}
