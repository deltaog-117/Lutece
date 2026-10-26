//! Opaque identifiers handed out by a backend.
//!
//! The core never interprets the number inside: an X11 backend wraps a window id, a
//! Wayland backend wraps a counter. Newtypes keep a window id from being used as an
//! output id.

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
    }
}
