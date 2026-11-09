//! Outlines drawn over everything else: a way to show where something was or would be,
//! without moving a window or touching a client.

use crate::look::Shaders;
use crate::render::SceneElement;
use crate::scale::Scale;
use lutece::{Rect, Rgba};
use smithay::utils::{Logical, Rectangle};

/// A rectangle's edge, drawn above the windows and the cursor's image.
///
/// An outline is only a picture: it is not a window, takes no input and changes nothing a
/// client sees. The window manager can use it to show a layout that is not the one on screen.
///
/// # Examples
///
/// ```
/// use lutece::{Rect, Rgba};
/// use lutece_wayland::Outline;
///
/// let outline = Outline {
///     rect: Rect::new(0, 0, 400, 300)?,
///     width: 3,
///     color: Rgba { r: 250, g: 179, b: 135, a: 255 },
/// };
/// assert_eq!(outline.width, 3);
/// # Ok::<(), lutece::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outline {
    /// The rectangle, in the output's logical pixels. The edge is drawn inside it.
    pub rect: Rect,
    /// How thick the edge is, in logical pixels.
    pub width: u8,
    /// The colour, with its alpha.
    pub color: Rgba,
}

/// The outline's rectangle as Smithay's type, clamped so a huge rectangle cannot overflow.
fn area(rect: Rect) -> Rectangle<i32, Logical> {
    let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    Rectangle::new(
        (rect.x(), rect.y()).into(),
        (clamp(rect.width()), clamp(rect.height())).into(),
    )
}

/// What to draw for `outlines`, first on top. An empty outline, or a transparent or
/// zero-width one, draws nothing.
pub(crate) fn elements(outlines: &[Outline], shaders: &Shaders, scale: Scale) -> Vec<SceneElement> {
    outlines
        .iter()
        .filter(|outline| outline.width > 0 && outline.color.a > 0)
        .map(|outline| {
            let c = outline.color;
            SceneElement::Border(shaders.border(
                area(outline.rect),
                0.0,
                (f64::from(outline.width) * scale.factor()) as f32,
                [
                    f32::from(c.r) / 255.0,
                    f32::from(c.g) / 255.0,
                    f32::from(c.b) / 255.0,
                    f32::from(c.a) / 255.0,
                ],
                1.0,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_area_is_the_rectangle() {
        let rect = Rect::new(-10, 20, 300, 200).expect("non-zero size");

        let area = area(rect);

        assert_eq!((area.loc.x, area.loc.y), (-10, 20));
        assert_eq!((area.size.w, area.size.h), (300, 200));
    }

    #[test]
    fn a_rectangle_too_large_for_the_range_is_clamped_not_wrapped() {
        let rect = Rect::new(0, 0, u32::MAX, u32::MAX).expect("non-zero size");

        let area = area(rect);

        assert_eq!((area.size.w, area.size.h), (i32::MAX, i32::MAX));
    }
}
