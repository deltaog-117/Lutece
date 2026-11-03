//! Bars and panels: windows that reserve space along a screen's edge.
//!
//! A bar announces the space it takes with `_NET_WM_STRUT_PARTIAL` (or the older
//! `_NET_WM_STRUT`), measured from the edges of the whole X screen. The core wants the
//! space per output, so [`Reserve`] works out, for each output, how far each strut reaches
//! into it and reports the result as [`Event::ReservedChanged`].

use lutece::{Event, Insets, Output, OutputId, Rect};
use std::collections::HashMap;
use x11rb::protocol::xproto::Window;

/// The inclusive range of pixels along an edge that a strut covers.
type Span = (u32, u32);

/// A strut covers the whole edge unless it says otherwise.
const WHOLE_EDGE: Span = (0, u32::MAX);

/// What one bar window reserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Strut {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
    /// Where along the left edge (as `y`) the strut is.
    left_span: Span,
    right_span: Span,
    /// Where along the top edge (as `x`) the strut is.
    top_span: Span,
    bottom_span: Span,
}

impl Strut {
    /// A bar that reserves nothing, which is what one with no usable property does.
    pub(crate) const NONE: Self = Self {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
        left_span: WHOLE_EDGE,
        right_span: WHOLE_EDGE,
        top_span: WHOLE_EDGE,
        bottom_span: WHOLE_EDGE,
    };

    /// Reads the strut from the two properties. `partial` holds the twelve values of
    /// `_NET_WM_STRUT_PARTIAL`, `plain` the four of `_NET_WM_STRUT`; the partial one wins
    /// because it says which part of the edge is taken. Values too short to read count as
    /// no strut.
    pub(crate) fn from_properties(partial: &[u32], plain: &[u32]) -> Self {
        if let [
            left,
            right,
            top,
            bottom,
            left_start,
            left_end,
            right_start,
            right_end,
            top_start,
            top_end,
            bottom_start,
            bottom_end,
            ..,
        ] = *partial
        {
            return Self {
                left,
                right,
                top,
                bottom,
                left_span: (left_start, left_end),
                right_span: (right_start, right_end),
                top_span: (top_start, top_end),
                bottom_span: (bottom_start, bottom_end),
            };
        }
        if let [left, right, top, bottom, ..] = *plain {
            return Self {
                left,
                right,
                top,
                bottom,
                ..Self::NONE
            };
        }
        Self::NONE
    }

    /// How far this strut reaches into `output`, on a screen `screen` pixels wide and
    /// high. An edge of the screen that the output does not touch, or a part of the edge
    /// that lies beside the output, reserves nothing there.
    pub(crate) fn insets_in(&self, output: Rect, screen: (u32, u32)) -> Insets {
        let (left, top) = (i64::from(output.x()), i64::from(output.y()));
        let right = left + i64::from(output.width());
        let bottom = top + i64::from(output.height());
        let (screen_width, screen_height) = (i64::from(screen.0), i64::from(screen.1));

        // Whether the inclusive span overlaps the output's range along that edge.
        let overlaps =
            |span: Span, from: i64, to: i64| i64::from(span.0) < to && i64::from(span.1) + 1 > from;
        let depth = |value: i64| u32::try_from(value.max(0)).unwrap_or(u32::MAX);

        let mut insets = Insets::NONE;
        if self.left > 0 && overlaps(self.left_span, top, bottom) {
            insets.left = depth(i64::from(self.left) - left);
        }
        if self.right > 0 && overlaps(self.right_span, top, bottom) {
            insets.right = depth(right - (screen_width - i64::from(self.right)));
        }
        if self.top > 0 && overlaps(self.top_span, left, right) {
            insets.top = depth(i64::from(self.top) - top);
        }
        if self.bottom > 0 && overlaps(self.bottom_span, left, right) {
            insets.bottom = depth(bottom - (screen_height - i64::from(self.bottom)));
        }
        // A strut on the far side of the screen can only reach this output if it is deep
        // enough; a negative depth was cut to zero above.
        insets
    }
}

/// The bars on the screen, the outputs, and what has been told to the core about them.
#[derive(Debug, Default)]
pub(crate) struct Reserve {
    docks: HashMap<Window, Strut>,
    outputs: Vec<Output>,
    /// The reserve last reported for each output; an output with none has no entry.
    reported: HashMap<OutputId, Insets>,
}

impl Reserve {
    pub(crate) fn is_dock(&self, window: Window) -> bool {
        self.docks.contains_key(&window)
    }

    /// Starts tracking `window` as a bar, or updates what it reserves.
    pub(crate) fn set_dock(&mut self, window: Window, strut: Strut) {
        self.docks.insert(window, strut);
    }

    /// Stops tracking `window`; `false` if it was not a bar.
    pub(crate) fn remove_dock(&mut self, window: Window) -> bool {
        self.docks.remove(&window).is_some()
    }

    pub(crate) fn set_outputs(&mut self, outputs: &[Output]) {
        self.outputs = outputs.to_vec();
    }

    /// The events that bring the core up to date with the bars, for a screen `screen`
    /// pixels wide and high. Only outputs whose reserve differs from the last report are
    /// mentioned, and an output that is gone is reset, since the core keeps what it was
    /// told.
    ///
    /// Two bars on one edge both start at the screen's edge, so an output's reserve is
    /// the largest of its bars' on each edge.
    pub(crate) fn changes(&mut self, screen: (u32, u32)) -> Vec<Event> {
        let now: Vec<(OutputId, Insets)> = self
            .outputs
            .iter()
            .map(|output| {
                let insets = self
                    .docks
                    .values()
                    .map(|strut| strut.insets_in(output.rect, screen))
                    .fold(Insets::NONE, Insets::max);
                (output.id, insets)
            })
            .collect();

        let mut events: Vec<Event> = now
            .iter()
            .filter(|(id, insets)| self.reported.get(id).copied().unwrap_or_default() != *insets)
            .map(|&(output, insets)| Event::ReservedChanged { output, insets })
            .collect();

        let mut gone: Vec<OutputId> = self
            .reported
            .keys()
            .copied()
            .filter(|id| now.iter().all(|(present, _)| present != id))
            .collect();
        gone.sort_by_key(|id| id.get());
        events.extend(gone.into_iter().map(|output| Event::ReservedChanged {
            output,
            insets: Insets::NONE,
        }));

        self.reported = now
            .into_iter()
            .filter(|(_, insets)| !insets.is_none())
            .collect();
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect::new(x, y, width, height).expect("non-zero size")
    }

    fn output(id: u32, x: i32, width: u32) -> Output {
        Output {
            id: OutputId::new(id),
            rect: rect(x, 0, width, 600),
        }
    }

    /// A strut on one edge, covering `span` along it.
    fn top_bar(depth: u32, span: Span) -> Strut {
        Strut {
            top: depth,
            top_span: span,
            ..Strut::NONE
        }
    }

    #[test]
    fn the_partial_property_wins_and_a_short_one_is_no_strut() {
        let partial = [1, 2, 3, 4, 10, 20, 30, 40, 50, 60, 70, 80];
        let strut = Strut::from_properties(&partial, &[9, 9, 9, 9]);
        assert_eq!(
            (strut.left, strut.right, strut.top, strut.bottom),
            (1, 2, 3, 4)
        );
        assert_eq!(strut.top_span, (50, 60));

        let plain = Strut::from_properties(&[1, 2, 3], &[5, 6, 7, 8]);
        assert_eq!((plain.left, plain.bottom), (5, 8));
        assert_eq!(plain.left_span, WHOLE_EDGE);

        assert_eq!(Strut::from_properties(&[1, 2], &[3]), Strut::NONE);
    }

    #[test]
    fn a_top_bar_reserves_its_depth_on_a_single_screen() {
        let strut = top_bar(30, WHOLE_EDGE);
        assert_eq!(
            strut.insets_in(rect(0, 0, 1280, 720), (1280, 720)),
            Insets::new(0, 30, 0, 0)
        );
    }

    #[test]
    fn a_bar_over_one_monitor_reserves_nothing_on_the_other() {
        // The bar spans the left monitor's 0..=639 only.
        let strut = top_bar(30, (0, 639));
        let screen = (1280, 600);
        assert_eq!(
            strut.insets_in(rect(0, 0, 640, 600), screen),
            Insets::new(0, 30, 0, 0)
        );
        assert_eq!(
            strut.insets_in(rect(640, 0, 640, 600), screen),
            Insets::NONE
        );
    }

    #[test]
    fn a_bar_below_a_stacked_monitor_does_not_reach_the_one_under_it() {
        // Two monitors stacked; a top strut of 30 touches only the upper one.
        let strut = top_bar(30, WHOLE_EDGE);
        let screen = (800, 1200);
        assert_eq!(
            strut.insets_in(rect(0, 0, 800, 600), screen),
            Insets::new(0, 30, 0, 0)
        );
        assert_eq!(
            strut.insets_in(rect(0, 600, 800, 600), screen),
            Insets::NONE
        );
    }

    #[test]
    fn a_right_and_bottom_strut_are_measured_from_the_far_edges() {
        let strut = Strut {
            right: 40,
            bottom: 25,
            ..Strut::NONE
        };
        assert_eq!(
            strut.insets_in(rect(0, 0, 1000, 600), (1000, 600)),
            Insets::new(0, 0, 40, 25)
        );
        // On a left monitor of a 2000-wide screen the right strut does not arrive.
        assert_eq!(
            strut.insets_in(rect(0, 0, 1000, 600), (2000, 600)),
            Insets::new(0, 0, 0, 25)
        );
    }

    #[test]
    fn a_strut_deeper_than_the_monitors_offset_reaches_into_it() {
        // A left strut of 1100 on a screen whose second monitor starts at x = 1000.
        let strut = Strut {
            left: 1100,
            ..Strut::NONE
        };
        assert_eq!(
            strut.insets_in(rect(1000, 0, 1000, 600), (2000, 600)),
            Insets::new(100, 0, 0, 0)
        );
    }

    proptest! {
        #[test]
        fn a_whole_edge_strut_on_a_lone_output_reserves_exactly_its_depth(
            left in 0_u32..200, right in 0_u32..200, top in 0_u32..200, bottom in 0_u32..200,
        ) {
            let strut = Strut { left, right, top, bottom, ..Strut::NONE };
            prop_assert_eq!(
                strut.insets_in(rect(0, 0, 1000, 800), (1000, 800)),
                Insets::new(left, top, right, bottom)
            );
        }

        #[test]
        fn a_strut_never_reserves_more_than_it_is_deep_beyond_the_output(
            x in 0_i32..3000, depth in 0_u32..5000,
        ) {
            let strut = Strut { left: depth, ..Strut::NONE };
            let insets = strut.insets_in(rect(x, 0, 500, 500), (4000, 500));
            prop_assert!(i64::from(insets.left) <= i64::from(depth));
        }
    }

    #[test]
    fn a_new_bar_is_reported_once_and_removing_it_resets_the_output() {
        let mut reserve = Reserve::default();
        reserve.set_outputs(&[output(0, 0, 1280)]);
        assert!(reserve.changes((1280, 600)).is_empty());

        reserve.set_dock(7, top_bar(30, WHOLE_EDGE));
        assert_eq!(
            reserve.changes((1280, 600)),
            [Event::ReservedChanged {
                output: OutputId::new(0),
                insets: Insets::new(0, 30, 0, 0),
            }]
        );
        assert!(
            reserve.changes((1280, 600)).is_empty(),
            "nothing new to say"
        );

        assert!(reserve.remove_dock(7));
        assert!(!reserve.remove_dock(7));
        assert_eq!(
            reserve.changes((1280, 600)),
            [Event::ReservedChanged {
                output: OutputId::new(0),
                insets: Insets::NONE,
            }]
        );
    }

    #[test]
    fn two_bars_on_one_edge_reserve_the_larger_not_the_sum() {
        let mut reserve = Reserve::default();
        reserve.set_outputs(&[output(0, 0, 1280)]);
        reserve.set_dock(1, top_bar(30, WHOLE_EDGE));
        reserve.set_dock(2, top_bar(50, WHOLE_EDGE));
        assert_eq!(
            reserve.changes((1280, 600)),
            [Event::ReservedChanged {
                output: OutputId::new(0),
                insets: Insets::new(0, 50, 0, 0),
            }]
        );
    }

    #[test]
    fn an_output_that_disappears_is_reset_for_the_core() {
        let mut reserve = Reserve::default();
        reserve.set_outputs(&[output(0, 0, 640), output(1, 640, 640)]);
        reserve.set_dock(1, top_bar(30, WHOLE_EDGE));
        let _ = reserve.changes((1280, 600));

        reserve.set_outputs(&[output(0, 0, 640)]);
        let events = reserve.changes((640, 600));
        assert_eq!(
            events,
            [Event::ReservedChanged {
                output: OutputId::new(1),
                insets: Insets::NONE,
            }]
        );
    }
}
