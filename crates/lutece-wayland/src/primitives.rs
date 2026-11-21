//! What a window's decorations are drawn from: a short list of primitives.
//!
//! A look is a list of rounded rectangles and pieces of text, each placed relative to the
//! window, drawn from the front one to the back one. The built-in title bar is such a list
//! (see [`title_bar`]), so a program that gives a window a list of its own replaces it by the
//! same road, with no second way to draw. The border and the shadow stay as the window's
//! style makes them.
//!
//! A list is checked when it is built: how many primitives, how long a text, how far from the
//! window. Anything past those limits is refused whole, so what reaches the drawer is always
//! small and bounded, and a hung or hostile program cannot make a frame expensive.

use lutece::Rgba;
use smithay::utils::{Logical, Rectangle};

/// The most primitives a look may hold.
pub const MAX_PRIMITIVES: usize = 16;

/// The longest text a primitive may hold, in bytes.
pub const MAX_TEXT_BYTES: usize = 256;

/// The furthest an area may be from the window, and the largest it may be, in logical pixels.
pub const MAX_EXTENT: u16 = 4096;

/// Where a primitive goes, measured from the top left corner of the window.
///
/// A negative `x` or `y` is outside the window to the left or above it, which is where a title
/// bar sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    /// How far right of the window's left edge the area starts.
    pub x: i16,
    /// How far below the window's top edge the area starts.
    pub y: i16,
    /// The width in pixels, or `None` to span the window: from `x` to as far from the window's
    /// right edge as `x` is from its left, so a bar as wide as the window has `x` of 0.
    pub width: Option<u16>,
    /// The height in pixels.
    pub height: u16,
}

impl Area {
    /// The area as a rectangle for the window at `window`. A window narrower than twice a
    /// positive `x` gives an area of no width, which is never drawn.
    #[must_use]
    pub fn resolve(self, window: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let x = i32::from(self.x);
        let width = self
            .width
            .map_or_else(|| window.size.w - 2 * x, i32::from)
            .max(0);
        Rectangle::new(
            (window.loc.x + x, window.loc.y + i32::from(self.y)).into(),
            (width, i32::from(self.height)).into(),
        )
    }
}

/// The text of a [`Primitive::Text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSource {
    /// The window's own title. The program that sets the look does not learn it from this.
    Title,
    /// This text.
    Literal(String),
}

/// One thing to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Primitive {
    /// A filled rectangle with rounded corners.
    Rect {
        /// Where it goes.
        area: Area,
        /// The corner radius in pixels; one past half the area's height is cut to it.
        radius: u16,
        /// The fill, with its alpha.
        color: Rgba,
    },
    /// A line of text, centred in its area, as large as the area's height allows.
    Text {
        /// Where it goes.
        area: Area,
        /// What it says.
        source: TextSource,
        /// The colour the text is drawn over. The text is light or dark to read on it; its
        /// own colour is not chosen.
        over: Rgba,
    },
}

impl Primitive {
    fn area(&self) -> Area {
        match self {
            Self::Rect { area, .. } | Self::Text { area, .. } => *area,
        }
    }
}

/// Why a list of primitives was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookError {
    /// More than [`MAX_PRIMITIVES`].
    #[error("a look holds at most {MAX_PRIMITIVES} primitives, not {0}")]
    TooMany(usize),
    /// A text longer than [`MAX_TEXT_BYTES`].
    #[error("a text holds at most {MAX_TEXT_BYTES} bytes, not {0}")]
    TextTooLong(usize),
    /// An area beyond [`MAX_EXTENT`] pixels from the window or in size.
    #[error("an area lies within {MAX_EXTENT} pixels of the window and is at most that large")]
    OutOfRange,
}

/// A checked list of primitives, front to back.
///
/// Invariant: at most [`MAX_PRIMITIVES`] of them, no text over [`MAX_TEXT_BYTES`], and every
/// area within [`MAX_EXTENT`] pixels. The only way to make one is [`WindowLook::try_new`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowLook(Vec<Primitive>);

impl WindowLook {
    /// Checks `primitives` and keeps them.
    ///
    /// # Errors
    ///
    /// Returns the first limit the list breaks.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::Rgba;
    /// use lutece_wayland::{Area, LookError, Primitive, WindowLook};
    ///
    /// let bar = Primitive::Rect {
    ///     area: Area { x: 0, y: -24, width: None, height: 24 },
    ///     radius: 6,
    ///     color: Rgba::new(30, 30, 30, 255),
    /// };
    /// assert!(WindowLook::try_new(vec![bar.clone()]).is_ok());
    /// assert_eq!(WindowLook::try_new(vec![bar; 17]), Err(LookError::TooMany(17)));
    /// ```
    pub fn try_new(primitives: Vec<Primitive>) -> Result<Self, LookError> {
        if primitives.len() > MAX_PRIMITIVES {
            return Err(LookError::TooMany(primitives.len()));
        }
        for primitive in &primitives {
            if let Primitive::Text {
                source: TextSource::Literal(text),
                ..
            } = primitive
                && text.len() > MAX_TEXT_BYTES
            {
                return Err(LookError::TextTooLong(text.len()));
            }
            let area = primitive.area();
            let far = area.x.unsigned_abs() > MAX_EXTENT || area.y.unsigned_abs() > MAX_EXTENT;
            let large =
                area.width.is_some_and(|width| width > MAX_EXTENT) || area.height > MAX_EXTENT;
            if far || large {
                return Err(LookError::OutOfRange);
            }
        }
        Ok(Self(primitives))
    }

    /// The primitives, front to back.
    #[must_use]
    pub fn primitives(&self) -> &[Primitive] {
        &self.0
    }

    /// Whether any primitive writes the window's title, so the drawer must be given it.
    #[must_use]
    pub fn reads_title(&self) -> bool {
        self.0.iter().any(|primitive| {
            matches!(
                primitive,
                Primitive::Text {
                    source: TextSource::Title,
                    ..
                }
            )
        })
    }
}

/// The built-in title bar as primitives: the title in front of a band as wide as the window and
/// its border, above both. `radius` is the window's corner radius, which the band's is cut from.
///
/// This is what a window with a bar in its style, and no look of its own, is drawn from.
#[must_use]
pub(crate) fn title_bar(height: u16, color: Rgba, border: u16, radius: u16) -> [Primitive; 2] {
    // A border or bar past what an `i16` holds is far past any screen; it saturates.
    let border = i16::try_from(border).unwrap_or(i16::MAX);
    let area = Area {
        x: border.saturating_neg(),
        y: border
            .saturating_add(i16::try_from(height).unwrap_or(i16::MAX))
            .saturating_neg(),
        width: None,
        height,
    };
    [
        Primitive::Text {
            area,
            source: TextSource::Title,
            over: color,
        },
        Primitive::Rect {
            area,
            radius,
            color,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn window() -> impl Strategy<Value = Rectangle<i32, Logical>> {
        (-3000i32..3000, -3000i32..3000, 1i32..4000, 1i32..4000)
            .prop_map(|(x, y, w, h)| Rectangle::new((x, y).into(), (w, h).into()))
    }

    proptest! {
        /// An area that spans the window is as wide as the window grown by `x` on each side,
        /// and sits `x` and `y` from its corner.
        #[test]
        fn a_spanning_area_is_the_window_grown_by_its_offset(
            window in window(),
            x in -200i16..200,
            y in -200i16..200,
            height in 0u16..200,
        ) {
            let area = Area { x, y, width: None, height };
            let found = area.resolve(window);
            prop_assert_eq!(found.loc.x, window.loc.x + i32::from(x));
            prop_assert_eq!(found.loc.y, window.loc.y + i32::from(y));
            prop_assert_eq!(found.size.w, (window.size.w - 2 * i32::from(x)).max(0));
            prop_assert_eq!(found.size.h, i32::from(height));
        }

        /// The built-in bar sits directly above the bordered window and is as wide as it.
        #[test]
        fn the_built_in_bar_sits_above_the_bordered_window(
            window in window(),
            border in 0u16..40,
            height in 1u16..120,
        ) {
            let [_, band] = title_bar(height, Rgba::new(1, 2, 3, 255), border, 0);
            let Primitive::Rect { area, .. } = band else {
                panic!("the second primitive is the band");
            };
            let bar = area.resolve(window);
            prop_assert_eq!(bar.loc.y + bar.size.h, window.loc.y - i32::from(border));
            prop_assert_eq!(bar.loc.x, window.loc.x - i32::from(border));
            prop_assert_eq!(bar.size.w, window.size.w + 2 * i32::from(border));
            prop_assert_eq!(bar.size.h, i32::from(height));
        }

        /// A fixed width is that width, wherever the window is.
        #[test]
        fn a_fixed_area_does_not_depend_on_the_window_size(
            window in window(),
            width in 0u16..4000,
        ) {
            let area = Area { x: 3, y: 4, width: Some(width), height: 10 };
            prop_assert_eq!(area.resolve(window).size.w, i32::from(width));
        }

        /// A list is accepted exactly when every limit holds, and what is accepted is what was
        /// given.
        #[test]
        fn a_list_within_the_limits_is_kept_as_it_is(count in 0usize..24, text in 0usize..400) {
            let primitives: Vec<Primitive> = (0..count)
                .map(|_| Primitive::Text {
                    area: Area { x: 0, y: 0, width: None, height: 10 },
                    source: TextSource::Literal("x".repeat(text)),
                    over: Rgba::new(0, 0, 0, 255),
                })
                .collect();
            let built = WindowLook::try_new(primitives.clone());
            if count > MAX_PRIMITIVES {
                prop_assert_eq!(built, Err(LookError::TooMany(count)));
            } else if count > 0 && text > MAX_TEXT_BYTES {
                prop_assert_eq!(built, Err(LookError::TextTooLong(text)));
            } else {
                let kept = built.expect("within the limits");
                prop_assert_eq!(kept.primitives(), &primitives[..]);
            }
        }
    }

    #[test]
    fn an_area_far_from_the_window_is_refused() {
        let far = Primitive::Rect {
            area: Area {
                x: 5000,
                y: 0,
                width: None,
                height: 10,
            },
            radius: 0,
            color: Rgba::new(0, 0, 0, 255),
        };
        assert_eq!(WindowLook::try_new(vec![far]), Err(LookError::OutOfRange));
        let large = Primitive::Rect {
            area: Area {
                x: 0,
                y: 0,
                width: Some(MAX_EXTENT + 1),
                height: 10,
            },
            radius: 0,
            color: Rgba::new(0, 0, 0, 255),
        };
        assert_eq!(WindowLook::try_new(vec![large]), Err(LookError::OutOfRange));
    }

    #[test]
    fn the_built_in_bar_sits_directly_above_the_bordered_window() {
        let window = Rectangle::new((100, 200).into(), (300, 150).into());
        let [text, rect] = title_bar(24, Rgba::new(10, 10, 10, 255), 2, 8);

        let Primitive::Rect { area, radius, .. } = rect else {
            panic!("the second primitive is the band");
        };
        let found = area.resolve(window);

        assert_eq!(radius, 8);
        assert_eq!(found.loc.x, 98);
        assert_eq!(found.size.w, 304);
        assert_eq!(found.loc.y + found.size.h, window.loc.y - 2);
        assert!(matches!(
            text,
            Primitive::Text {
                source: TextSource::Title,
                ..
            }
        ));
    }

    #[test]
    fn a_look_that_writes_the_title_says_so() {
        let [text, rect] = title_bar(20, Rgba::new(0, 0, 0, 255), 0, 0);
        assert!(
            WindowLook::try_new(vec![text])
                .expect("one primitive")
                .reads_title()
        );
        assert!(
            !WindowLook::try_new(vec![rect])
                .expect("one primitive")
                .reads_title()
        );
    }
}
