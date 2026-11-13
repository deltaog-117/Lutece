//! The interface a kernel offers to whatever drives it.
//!
//! A kernel is the part of the core that decides where windows go: it takes
//! [`Event`]s, keeps the state they build, and shows that state as a [`Scene`]. The
//! [`Engine`](crate::Engine) is the kernel that gives each window one workspace; a canvas
//! or multi-tag kernel would be another. Code that only replays, records or rewinds a
//! session needs these four calls and nothing else, so it can be written once for every
//! kernel.
//!
//! What a kernel must keep true, because replay depends on it:
//!
//! - **Determinism.** The same events, applied in order to equal kernels, give equal
//!   kernels and equal scenes. Nothing outside the events (the clock, the environment)
//!   may change the state.
//! - **Snapshots are faithful.** Restoring a snapshot onto a kernel built with the same
//!   configuration gives a kernel equal to the one the snapshot was taken from.
//! - **The configuration is not state.** A snapshot holds what the session did, never the
//!   keys, rules or layouts it was configured with; those come from the template.

use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::event::Event;
use crate::shared::contracts::scene::Scene;
use crate::shared::error::Result;
use std::fmt::Debug;

/// A kernel: events in, effects and a scene out.
///
/// # Examples
///
/// ```
/// use lutece::{Event, Kernel, Layout, Output, OutputId, Rect, StackSet, Tag, WinId, WindowInfo};
///
/// // Written once, usable with any kernel.
/// fn open_one<K: Kernel>(kernel: &mut K) -> Result<usize, lutece::Error> {
///     kernel.handle(Event::WindowOpened { id: WinId::new(1), info: WindowInfo::default() });
///     Ok(kernel.scene()?.placements().len())
/// }
///
/// let stacks = StackSet::new([Tag::try_new("1")?])?;
/// let mut engine = lutece::Engine::new(stacks, Layout::default());
/// let screen = Output { id: OutputId::new(0), rect: Rect::new(0, 0, 800, 600)? };
/// engine.handle(Event::OutputsChanged(vec![screen]));
/// assert_eq!(open_one(&mut engine)?, 1);
/// # Ok::<(), lutece::Error>(())
/// ```
pub trait Kernel: Clone + PartialEq + Debug {
    /// The kernel's state as plain data, for checkpoints.
    type Snapshot: Clone + PartialEq + Debug;

    /// The layout number of [`Snapshot`](Self::Snapshot). A reader that finds another
    /// number must not try to read the data.
    const STATE_VERSION: u32;

    /// Applies `event` and returns the effects a backend must carry out. An event that no
    /// longer applies is ignored, not reported.
    fn handle(&mut self, event: Event) -> Vec<Effect>;

    /// What should be on screen now, derived from the state, never stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the state cannot be shown as a valid scene.
    fn scene(&self) -> Result<Scene>;

    /// The state as plain data. The configuration is not part of it.
    fn snapshot(&self) -> Self::Snapshot;

    /// Builds the kernel `snapshot` was taken from, onto `template`, which supplies the
    /// configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot has another version, does not fit the template, or
    /// breaks a rule the kernel keeps.
    fn restore(template: &Self, snapshot: &Self::Snapshot) -> Result<Self>;
}
