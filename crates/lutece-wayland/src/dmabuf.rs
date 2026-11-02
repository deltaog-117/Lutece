//! GPU buffers from clients (`linux-dmabuf`).
//!
//! A client that draws with the GPU can hand its frame over as a dmabuf, a handle to GPU
//! memory, instead of copying pixels through shared memory. The compositor offers this by
//! saying which GPU it draws with and which pixel formats that GPU can read; a client
//! allocates for that GPU and the buffer is used where it is.
//!
//! The backend itself owns no renderer, so the host that has one describes the GPU with a
//! [`DmabufSupport`]. Until then the global does not exist and clients keep using shared
//! memory.

use crate::compositor::State;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Format, Fourcc, Modifier};
use smithay::delegate_dmabuf;
use smithay::wayland::dmabuf::{
    DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
};

/// The GPU the compositor draws with and the buffer formats it can read from it.
///
/// Invariant: every format is one the DRM fourcc list knows, so a client is never offered
/// a code nobody can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmabufSupport {
    device: u64,
    formats: Vec<Format>,
}

impl DmabufSupport {
    /// Describes a GPU by the device number of its render node (`st_rdev`) and the
    /// `(fourcc, modifier)` pairs it can import. A pair whose fourcc is not a known DRM
    /// format is left out.
    pub fn new(device: u64, formats: impl IntoIterator<Item = (u32, u64)>) -> Self {
        let formats = formats
            .into_iter()
            .filter_map(|(code, modifier)| {
                Some(Format {
                    code: Fourcc::try_from(code).ok()?,
                    modifier: Modifier::from(modifier),
                })
            })
            .collect();
        Self { device, formats }
    }

    /// The device number of the render node.
    #[must_use]
    pub const fn device(&self) -> u64 {
        self.device
    }

    /// How many formats are offered.
    #[must_use]
    pub fn format_count(&self) -> usize {
        self.formats.len()
    }
}

/// The dmabuf global.
pub(crate) struct Dmabufs {
    state: DmabufState,
    /// `None` until the host describes its GPU.
    global: Option<DmabufGlobal>,
}

impl Dmabufs {
    pub(crate) fn new() -> Self {
        Self {
            state: DmabufState::new(),
            global: None,
        }
    }
}

impl State {
    /// Offers dmabuf buffers on `support`'s GPU. A global offered before is replaced.
    pub(crate) fn enable_dmabuf(&mut self, support: &DmabufSupport) -> std::io::Result<()> {
        let feedback =
            DmabufFeedbackBuilder::new(support.device, support.formats.iter().copied()).build()?;
        if let Some(old) = self.dmabufs.global.take() {
            self.dmabufs
                .state
                .destroy_global::<Self>(&self.display, old);
        }
        self.dmabufs.global = Some(
            self.dmabufs
                .state
                .create_global_with_default_feedback::<Self>(&self.display, &feedback),
        );
        Ok(())
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabufs.state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        _dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        // Smithay has already refused a format that was not offered. The renderer lives in
        // the host, out of reach while a request is being served, so the GPU is not asked
        // here: a buffer the driver still rejects is skipped when drawn.
        // The only failure is a client that vanished meanwhile.
        let _ = notifier.successful::<Self>();
    }
}

delegate_dmabuf!(State);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const XRGB8888: u32 = 0x3432_5258; // fourcc "XR24"
    const LINEAR: u64 = 0;

    #[test]
    fn a_known_format_is_kept() {
        let support = DmabufSupport::new(7, [(XRGB8888, LINEAR)]);
        assert_eq!(support.format_count(), 1);
        assert_eq!(support.device(), 7);
    }

    #[test]
    fn a_code_nobody_can_name_is_dropped() {
        let support = DmabufSupport::new(7, [(0, LINEAR), (XRGB8888, LINEAR)]);
        assert_eq!(support.format_count(), 1);
    }

    proptest! {
        /// However the formats are described, none is invented: at most the ones listed
        /// are offered.
        #[test]
        fn never_more_formats_than_listed(
            device in any::<u64>(),
            list in proptest::collection::vec((any::<u32>(), any::<u64>()), 0..32),
        ) {
            let support = DmabufSupport::new(device, list.clone());
            prop_assert!(support.format_count() <= list.len());
            prop_assert_eq!(support.device(), device);
        }
    }
}
