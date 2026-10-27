//! Key grabs: which X keycodes and modifier masks stand for a lutece key combination.
//!
//! The core names keys by keysym, X reports them by keycode. [`Keyboard`] is the keycode
//! table fetched once from the server; reading it needs no connection, so the lookups
//! can be checked without a server.

use crate::error::{Error, Result};
use lutece::{KeyPress, Keysym, Modifiers};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::ErrorKind;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, KeyButMask, Keycode, ModMask, Window};

/// CapsLock and NumLock (Mod2 on practically every setup) must not stop a binding from
/// matching, so each key is grabbed with every combination of them held.
fn lock_variants() -> [ModMask; 4] {
    [
        ModMask::from(0u16),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ]
}

/// The server's keycode to keysym table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Keyboard {
    min_keycode: Keycode,
    per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Keyboard {
    pub(crate) fn new(min_keycode: Keycode, per_keycode: u8, keysyms: Vec<u32>) -> Self {
        Self {
            min_keycode,
            per_keycode: usize::from(per_keycode),
            keysyms,
        }
    }

    /// Fetches the table for every keycode the server has.
    pub(crate) fn fetch(conn: &impl Connection) -> Result<Self> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = conn
            .get_keyboard_mapping(min, max.saturating_sub(min).saturating_add(1))?
            .reply()?;
        Ok(Self::new(min, reply.keysyms_per_keycode, reply.keysyms))
    }

    /// The keysym a key produces without any modifier. That is the one a binding names:
    /// Super+Shift+q is `q` with Shift, so the shifted keysym is never looked up.
    pub(crate) fn keysym_of(&self, keycode: Keycode) -> Option<Keysym> {
        let row = usize::from(keycode.checked_sub(self.min_keycode)?);
        let raw = *self.keysyms.get(row.checked_mul(self.per_keycode)?)?;
        (raw != 0).then_some(Keysym(raw))
    }

    /// The keycodes that produce `keysym` without a modifier. Keysym 0 means "no symbol",
    /// which unused keys carry, so it names no key.
    pub(crate) fn keycodes_of(&self, keysym: Keysym) -> impl Iterator<Item = Keycode> + '_ {
        self.keysyms
            .iter()
            .step_by(self.per_keycode.max(1))
            .enumerate()
            .filter(move |(_, raw)| keysym.0 != 0 && **raw == keysym.0)
            .filter_map(|(row, _)| Keycode::try_from(row).ok()?.checked_add(self.min_keycode))
    }
}

/// The lutece modifiers held in an X key state. Lock keys and mouse buttons are left
/// out.
pub(crate) fn modifiers_of(state: KeyButMask) -> Modifiers {
    [
        (KeyButMask::SHIFT, Modifiers::SHIFT),
        (KeyButMask::CONTROL, Modifiers::CTRL),
        (KeyButMask::MOD1, Modifiers::ALT),
        (KeyButMask::MOD4, Modifiers::SUPER),
    ]
    .into_iter()
    .filter(|(x, _)| state.contains(*x))
    .fold(Modifiers::NONE, |all, (_, ours)| all | ours)
}

/// The X modifier mask for lutece modifiers.
pub(crate) fn mod_mask_of(modifiers: Modifiers) -> ModMask {
    [
        (Modifiers::SHIFT, ModMask::SHIFT),
        (Modifiers::CTRL, ModMask::CONTROL),
        (Modifiers::ALT, ModMask::M1),
        (Modifiers::SUPER, ModMask::M4),
    ]
    .into_iter()
    .filter(|(ours, _)| modifiers.contains(*ours))
    .fold(ModMask::from(0u16), |all, (_, x)| all | x)
}

/// Grabs `presses` on the root window, so their presses reach us whichever window has
/// the focus.
///
/// Every request is sent before any answer is awaited, so the cost is one round trip.
///
/// # Errors
///
/// Returns [`Error::UnmappedKeysym`] if no key produces a keysym, and
/// [`Error::KeyAlreadyGrabbed`] if another client holds the combination.
pub(crate) fn grab_keys(
    conn: &impl Connection,
    keyboard: &Keyboard,
    root: Window,
    presses: impl IntoIterator<Item = KeyPress>,
) -> Result<()> {
    let mut pending = Vec::new();
    for press in presses {
        let mut keycodes = keyboard.keycodes_of(press.keysym).peekable();
        if keycodes.peek().is_none() {
            return Err(Error::UnmappedKeysym(press.keysym));
        }
        for keycode in keycodes {
            for lock in lock_variants() {
                let cookie = conn.grab_key(
                    false,
                    root,
                    mod_mask_of(press.modifiers) | lock,
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?;
                pending.push((press, cookie));
            }
        }
    }
    for (press, cookie) in pending {
        match cookie.check() {
            Err(ReplyError::X11Error(e)) if e.error_kind == ErrorKind::Access => {
                return Err(Error::KeyAlreadyGrabbed(press));
            }
            other => other?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Keycodes 8.., two columns each: the plain keysym, then the shifted one.
    fn keyboard(plain: &[u32]) -> Keyboard {
        let keysyms = plain.iter().flat_map(|k| [*k, *k + 1]).collect();
        Keyboard::new(8, 2, keysyms)
    }

    #[test]
    fn a_key_is_found_by_its_plain_keysym_and_not_its_shifted_one() {
        let kb = keyboard(&[0x61, 0x62, 0x63]);
        assert_eq!(kb.keysym_of(9), Some(Keysym(0x62)));
        assert_eq!(kb.keycodes_of(Keysym(0x62)).collect::<Vec<_>>(), [9]);
        assert_eq!(kb.keycodes_of(Keysym(0x63)).count(), 1);
        // 0x63 + 1 is only ever the shifted column of the last key.
        assert_eq!(kb.keycodes_of(Keysym(0x64)).count(), 0);
    }

    #[test]
    fn keycodes_outside_the_table_have_no_keysym() {
        let kb = keyboard(&[0x61]);
        assert_eq!(kb.keysym_of(7), None);
        assert_eq!(kb.keysym_of(9), None);
        assert_eq!(Keyboard::new(8, 0, Vec::new()).keysym_of(8), None);
    }

    #[test]
    fn an_unused_slot_has_no_keysym() {
        let kb = keyboard(&[0, 0x61]);
        assert_eq!(kb.keysym_of(8), None);
        assert_eq!(kb.keycodes_of(Keysym(0)).count(), 0);
    }

    proptest! {
        /// Looking a keycode up and then asking which keycodes make that keysym always
        /// includes the keycode we started from.
        #[test]
        fn every_keycode_is_among_the_keycodes_of_its_keysym(
            plain in proptest::collection::vec(1u32..200, 1..40),
            pick in any::<prop::sample::Index>(),
        ) {
            let kb = keyboard(&plain);
            let keycode = 8 + u8::try_from(pick.index(plain.len())).expect("short table");
            let keysym = kb.keysym_of(keycode).expect("filled slot");
            prop_assert!(kb.keycodes_of(keysym).any(|k| k == keycode));
        }

        #[test]
        fn modifiers_survive_the_trip_through_an_x_mask(bits in 0u8..16) {
            let modifiers = Modifiers::from_bits(bits).expect("defined bits");
            let mask = mod_mask_of(modifiers);
            // A key state is a mask in another type; its lock and button bits add nothing.
            let state = KeyButMask::from(u16::from(mask) | u16::from(ModMask::LOCK | ModMask::M2));
            prop_assert_eq!(modifiers_of(state), modifiers);
        }
    }
}
