//! The last picture of a window that has closed, kept so it can fade and shrink away.
//!
//! A client that destroys its window takes its surface with it, so the window cannot be drawn
//! from then on. The backend therefore keeps, for each window whose style closes with a
//! transition ([`Glide::CLOSES`]), a snapshot of the last frame it drew: the texture (a cheap
//! shared handle, not a copy of the pixels) and where and how it was drawn. When the window is
//! destroyed the snapshot becomes a ghost, drawn on top of the other windows until the
//! transition ends, and then dropped.
//!
//! Only the window's own surface is kept, not its subsurfaces or popups. The timing and the
//! shape of the fade are pure functions, tested without a display.

use crate::animation::ease;
use crate::windows::{Layer, Look};
use lutece::{Glide, Transition, WinId};
use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How big the picture is when it is gone, as a share of its size.
const SHRINK_TO: f64 = 0.85;

/// A window's surface as it was last drawn.
#[derive(Clone)]
pub(crate) struct Picture {
    pub(crate) texture: GlesTexture,
    pub(crate) scale: i32,
    pub(crate) transform: Transform,
    /// The surface's size in logical pixels.
    pub(crate) size: Size<i32, Logical>,
    /// Where the surface's own corner was, in the compositor's space.
    pub(crate) origin: Point<i32, Logical>,
}

impl Picture {
    /// The picture of `surface` as the renderer holds it, or `None` when the surface has no
    /// texture yet (nothing was drawn) or the renderer holds none for this context.
    fn of(
        surface: &WlSurface,
        origin: Point<i32, Logical>,
        renderer: &GlesRenderer,
    ) -> Option<Self> {
        let context = renderer.context_id();
        with_renderer_surface_state(surface, |state| {
            Some(Self {
                texture: state.texture::<GlesTexture>(context)?.clone(),
                scale: state.buffer_scale(),
                transform: state.buffer_transform(),
                size: state.surface_size()?,
                origin,
            })
        })
        .flatten()
    }
}

/// A window's last drawn frame: its picture and the look it was drawn with.
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub(crate) picture: Picture,
    pub(crate) look: Look,
}

/// When a ghost started and how long it lasts: all the timing, apart from the picture, so it
/// can be tested without a texture.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    started: Instant,
    transition: Transition,
}

impl Timing {
    /// How far along it is at `now`, eased, from 0 to 1.
    pub(crate) fn progress_at(&self, now: Instant) -> f64 {
        ease(self.transition.curve, self.linear(now)).clamp(0.0, 1.0)
    }

    /// Whether it is over by `now`.
    pub(crate) fn finished(&self, now: Instant) -> bool {
        self.linear(now) >= 1.0
    }

    /// Time passed over the transition's length, from 0 to 1; a length of zero is over.
    fn linear(&self, now: Instant) -> f64 {
        let total = Duration::from_millis(u64::from(self.transition.duration_ms));
        if total.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started);
        (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
    }
}

/// A closed window on its way out.
pub(crate) struct Ghost {
    pub(crate) snapshot: Snapshot,
    /// What smithay tells the pictures of one element apart by.
    pub(crate) id: Id,
    pub(crate) timing: Timing,
}

/// The snapshots of windows that would close with a transition, and the ghosts of those that
/// did.
#[derive(Default)]
pub(crate) struct Ghosts {
    snapshots: HashMap<WinId, Snapshot>,
    running: Vec<Ghost>,
}

impl Ghosts {
    /// Takes a snapshot of every placed window whose style closes with a transition, from the
    /// layers just drawn, and forgets the snapshot of a window not drawn in them: one that is
    /// hidden has no picture worth showing if it closes later.
    pub(crate) fn capture(&mut self, layers: &[Layer], renderer: &GlesRenderer) {
        let mut seen = Vec::new();
        for layer in layers.iter().filter(|layer| !layer.popup) {
            let (Some(id), Some(look)) = (layer.id, &layer.look) else {
                continue;
            };
            if !look.style.glide().contains(Glide::CLOSES) {
                continue;
            }
            if let Some(picture) = Picture::of(&layer.surface, layer.origin, renderer) {
                self.snapshots.insert(
                    id,
                    Snapshot {
                        picture,
                        look: look.clone(),
                    },
                );
                seen.push(id);
            }
        }
        self.snapshots.retain(|id, _| seen.contains(id));
    }

    /// Turns the snapshot of window `id` into a ghost that lasts for `transition`. Nothing
    /// happens when there is no snapshot or the transition has no length.
    pub(crate) fn start(&mut self, id: WinId, transition: Transition, now: Instant) {
        let snapshot = self.snapshots.remove(&id);
        if transition.duration_ms == 0 {
            return;
        }
        if let Some(snapshot) = snapshot {
            self.running.push(Ghost {
                snapshot,
                id: Id::new(),
                timing: Timing {
                    started: now,
                    transition,
                },
            });
        }
    }

    /// Drops the ghosts that are over at `now`.
    pub(crate) fn prune(&mut self, now: Instant) {
        self.running.retain(|ghost| !ghost.timing.finished(now));
    }

    /// The ghosts still going, oldest first.
    pub(crate) fn running(&self) -> &[Ghost] {
        &self.running
    }

    /// Whether a ghost is still going at `now`, so the screen must keep being drawn.
    #[cfg(feature = "drm")]
    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.running.iter().any(|ghost| !ghost.timing.finished(now))
    }
}

/// `area` shrunk towards its centre: at `progress` 0 it is as it was, at 1 it is
/// [`SHRINK_TO`] of its size. Never smaller than one pixel each way unless it was empty.
pub(crate) fn shrunk(area: Rectangle<i32, Logical>, progress: f64) -> Rectangle<i32, Logical> {
    let factor = 1.0 - (1.0 - SHRINK_TO) * progress.clamp(0.0, 1.0);
    let shrink = |length: i32| {
        if length <= 0 {
            return length;
        }
        // A rounded share of a positive `i32` is within `i32`; one pixel is the floor.
        ((f64::from(length) * factor).round() as i32).clamp(1, length)
    };
    let size = (shrink(area.size.w), shrink(area.size.h));
    let loc = (
        area.loc.x + (area.size.w - size.0) / 2,
        area.loc.y + (area.size.h - size.1) / 2,
    );
    Rectangle::new(loc.into(), size.into())
}

/// `opacity` faded out by `progress`: all of it at 0, none at 1.
pub(crate) fn faded(opacity: u8, progress: f64) -> u8 {
    let left = f64::from(opacity) * (1.0 - progress.clamp(0.0, 1.0));
    // Between 0 and `opacity` by the clamp, so the cast cannot lose anything.
    left.round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn area() -> impl Strategy<Value = Rectangle<i32, Logical>> {
        (-5000i32..5000, -5000i32..5000, 1i32..4000, 1i32..4000)
            .prop_map(|(x, y, w, h)| Rectangle::new((x, y).into(), (w, h).into()))
    }

    proptest! {
        /// A shrunk window lies inside the one it came from and is never empty.
        #[test]
        fn a_shrunk_window_stays_inside_the_original(area in area(), progress in 0.0f64..=1.0) {
            let inner = shrunk(area, progress);
            prop_assert!(inner.loc.x >= area.loc.x && inner.loc.y >= area.loc.y);
            prop_assert!(inner.loc.x + inner.size.w <= area.loc.x + area.size.w);
            prop_assert!(inner.loc.y + inner.size.h <= area.loc.y + area.size.h);
            prop_assert!(inner.size.w >= 1 && inner.size.h >= 1);
        }

        /// It stays centred, to within the pixel that rounding costs.
        #[test]
        fn a_shrunk_window_stays_centred(area in area(), progress in 0.0f64..=1.0) {
            let inner = shrunk(area, progress);
            let left = inner.loc.x - area.loc.x;
            let right = (area.loc.x + area.size.w) - (inner.loc.x + inner.size.w);
            prop_assert!((left - right).abs() <= 1, "{left} {right}");
        }

        /// The further along, the smaller it is and the fainter.
        #[test]
        fn later_is_smaller_and_fainter(
            area in area(),
            opacity in any::<u8>(),
            a in 0.0f64..=1.0,
            b in 0.0f64..=1.0,
        ) {
            let (early, late) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(shrunk(area, late).size.w <= shrunk(area, early).size.w);
            prop_assert!(shrunk(area, late).size.h <= shrunk(area, early).size.h);
            prop_assert!(faded(opacity, late) <= faded(opacity, early));
        }

        /// Whatever the progress, even past the range, the opacity never grows.
        #[test]
        fn fading_never_brightens(opacity in any::<u8>(), progress in -10.0f64..10.0) {
            prop_assert!(faded(opacity, progress) <= opacity);
        }
    }

    #[test]
    fn the_start_is_the_window_as_it_was_and_the_end_is_nothing_visible() {
        let area = Rectangle::new((10, 20).into(), (400, 300).into());
        assert_eq!(shrunk(area, 0.0), area);
        assert_eq!(faded(230, 0.0), 230);
        assert_eq!(faded(230, 1.0), 0);
    }

    #[test]
    fn an_empty_window_is_not_given_a_size() {
        let area = Rectangle::new((0, 0).into(), (0, 10).into());
        assert_eq!(shrunk(area, 1.0).size.w, 0);
    }

    fn timing(duration_ms: u16, started: Instant) -> Timing {
        Timing {
            started,
            transition: Transition {
                curve: lutece::Curve::Linear,
                duration_ms,
            },
        }
    }

    #[test]
    fn a_ghost_is_over_when_its_transition_is() {
        let start = Instant::now();
        let timing = timing(200, start);
        assert!(!timing.finished(start));
        assert!(!timing.finished(start + Duration::from_millis(199)));
        assert!(timing.finished(start + Duration::from_millis(200)));
        assert!((timing.progress_at(start + Duration::from_millis(100)) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_transition_of_no_length_is_over_at_once() {
        let start = Instant::now();
        assert!(timing(0, start).finished(start));
    }

    proptest! {
        /// Progress is within 0 to 1 and never goes back as time passes.
        #[test]
        fn progress_only_goes_forward(ms in 1u16..2000, a in 0u64..4000, b in 0u64..4000) {
            let start = Instant::now();
            let timing = timing(ms, start);
            let (early, late) = if a <= b { (a, b) } else { (b, a) };
            let first = timing.progress_at(start + Duration::from_millis(early));
            let second = timing.progress_at(start + Duration::from_millis(late));
            prop_assert!((0.0..=1.0).contains(&first) && (0.0..=1.0).contains(&second));
            prop_assert!(first <= second);
        }
    }
}
