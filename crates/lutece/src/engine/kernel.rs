//! The engine as a [`Kernel`](crate::Kernel): each call forwards to the engine's own method
//! of the same name, so nothing about how the engine behaves changes.

use super::Engine;
use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::event::Event;
use crate::shared::contracts::kernel::Kernel;
use crate::shared::contracts::scene::Scene;
use crate::shared::error::Result;
use crate::snapshot::{Snapshot, VERSION};

impl Kernel for Engine {
    type Snapshot = Snapshot;

    const STATE_VERSION: u32 = VERSION;

    fn handle(&mut self, event: Event) -> Vec<Effect> {
        Self::handle(self, event)
    }

    fn scene(&self) -> Result<Scene> {
        Self::scene(self)
    }

    fn snapshot(&self) -> Snapshot {
        Self::snapshot(self)
    }

    fn restore(template: &Self, snapshot: &Snapshot) -> Result<Self> {
        Self::restore(template, snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::shared::contracts::event::{Output, WindowInfo};
    use crate::shared::geometry::Rect;
    use crate::shared::ids::{OutputId, Tag, WinId};
    use crate::stack::stackset::StackSet;
    use proptest::prelude::*;

    fn fresh() -> Engine {
        let tags = ["1", "2"].map(|name| Tag::try_new(name).expect("has text"));
        let mut engine = Engine::new(StackSet::new(tags).expect("distinct"), Layout::default());
        engine.handle(Event::OutputsChanged(vec![Output {
            id: OutputId::new(0),
            rect: Rect::new(0, 0, 1000, 600).expect("a rectangle"),
        }]));
        engine
    }

    fn event() -> impl Strategy<Value = Event> {
        (0u8..3, 1u64..6).prop_map(|(kind, id)| {
            let id = WinId::new(id);
            match kind {
                0 => Event::WindowOpened {
                    id,
                    info: WindowInfo::default(),
                },
                1 => Event::WindowClosed(id),
                _ => Event::FocusRequested(id),
            }
        })
    }

    /// Everything a replay does, written against the trait alone.
    fn drive<K: Kernel>(mut kernel: K, events: &[Event]) -> (K, Vec<Vec<Effect>>) {
        let effects = events
            .iter()
            .map(|event| kernel.handle(event.clone()))
            .collect();
        (kernel, effects)
    }

    proptest! {
        /// Going through the trait is the same as calling the engine: same state, same
        /// effects, same scene.
        #[test]
        fn the_trait_changes_nothing(events in prop::collection::vec(event(), 0..40)) {
            let mut direct = fresh();
            let direct_effects: Vec<_> =
                events.iter().map(|event| direct.handle(event.clone())).collect();
            let (through, effects) = drive(fresh(), &events);

            prop_assert_eq!(&through, &direct);
            prop_assert_eq!(effects, direct_effects);
            prop_assert_eq!(through.scene().expect("a scene"), direct.scene().expect("a scene"));
        }

        /// A snapshot restored onto the template gives back the engine it came from.
        #[test]
        fn a_snapshot_restores_to_an_equal_kernel(events in prop::collection::vec(event(), 0..40)) {
            let (kernel, _) = drive(fresh(), &events);
            let restored = <Engine as Kernel>::restore(&fresh(), &kernel.snapshot())
                .expect("a snapshot of this engine restores");
            prop_assert_eq!(restored, kernel);
        }

        /// Two kernels that took the same events are equal: replay does not depend on
        /// anything but the events.
        #[test]
        fn the_same_events_give_equal_kernels(events in prop::collection::vec(event(), 0..40)) {
            let (first, _) = drive(fresh(), &events);
            let (second, _) = drive(fresh(), &events);
            prop_assert_eq!(first, second);
        }
    }

    #[test]
    fn the_state_version_is_the_snapshots() {
        assert_eq!(<Engine as Kernel>::STATE_VERSION, crate::snapshot::VERSION);
    }
}
