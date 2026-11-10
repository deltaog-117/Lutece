//! How a window should look and how it should get there: corner radius, border,
//! opacity, and the curve and duration of the animation that carries it to its next
//! place.
//!
//! All of it is plain integers and a fixed set of curves, so a style can be compared,
//! logged and replayed exactly. Every combination is meaningful, so there is nothing to
//! reject at construction.

/// An 8-bit-per-channel colour with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha: 0 is transparent, 255 is opaque.
    pub a: u8,
}

impl Rgba {
    /// A colour from its four channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// A frame drawn around a window, inside the area the core gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Border {
    /// Thickness in pixels; 0 draws no border.
    pub width: u16,
    /// Its colour.
    pub color: Rgba,
}

/// A band drawn above a window, outside the area the core gave it, as wide as the window with
/// its border.
///
/// It is only a coloured band: no title is written in it yet. It is drawn as a rounded
/// bar, whose corners follow the window's radius but never more than half its height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TitleBar {
    /// Height in pixels; 0 draws no bar.
    pub height: u16,
    /// Its colour.
    pub color: Rgba,
}

/// A soft shadow drawn behind a window, outside the area the core gave it.
///
/// It fades out over `blur` pixels from the window's edge, is moved by `dx` and `dy` (a
/// positive `dy` is down on the screen), and is not drawn under the window itself, so a
/// translucent window does not show it through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Shadow {
    /// How far the shadow fades out, in pixels; 0 draws no shadow.
    pub blur: u16,
    /// How far it is moved to the right, in pixels; negative is left.
    pub dx: i16,
    /// How far it is moved down, in pixels; negative is up.
    pub dy: i16,
    /// Its colour.
    pub color: Rgba,
}

/// How an animation's progress relates to elapsed time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Curve {
    /// Constant speed.
    #[default]
    Linear,
    /// Starts slowly and speeds up.
    EaseIn,
    /// Starts fast and slows down.
    EaseOut,
    /// Slow at both ends.
    EaseInOut,
}

/// The animation a window's next change of place or look should use.
///
/// A duration of 0 means no animation: the window jumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Transition {
    /// The timing curve.
    pub curve: Curve,
    /// How long it lasts, in milliseconds.
    pub duration_ms: u16,
}

/// The look of one window.
///
/// The default is a plain, fully opaque window with no border and no animation, which is
/// also what a window with no style gets.
///
/// # Examples
///
/// ```
/// use lutece::{Curve, Rgba, Style, Transition};
///
/// let style = Style::default()
///     .with_radius(12)
///     .with_border(2, Rgba::new(255, 255, 255, 255))
///     .with_opacity(230)
///     .with_transition(Transition { curve: Curve::EaseOut, duration_ms: 200 });
/// assert_eq!(style.radius(), 12);
/// assert_ne!(style, Style::default());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct Style {
    radius: u16,
    border: Border,
    opacity: u8,
    transition: Transition,
    title_bar: TitleBar,
    shadow: Shadow,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            radius: 0,
            border: Border::default(),
            // Opaque, not zero: a window with no style must stay visible.
            opacity: u8::MAX,
            transition: Transition::default(),
            title_bar: TitleBar::default(),
            shadow: Shadow::default(),
        }
    }
}

impl Style {
    /// Rounds the window's corners by `radius` pixels.
    #[must_use]
    pub const fn with_radius(mut self, radius: u16) -> Self {
        self.radius = radius;
        self
    }

    /// Draws a border `width` pixels thick in `color`.
    #[must_use]
    pub const fn with_border(mut self, width: u16, color: Rgba) -> Self {
        self.border = Border { width, color };
        self
    }

    /// Sets opacity: 0 is invisible, 255 is opaque.
    #[must_use]
    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.opacity = opacity;
        self
    }

    /// Sets the animation used when this window moves or changes look.
    #[must_use]
    pub const fn with_transition(mut self, transition: Transition) -> Self {
        self.transition = transition;
        self
    }

    /// Draws a band `height` pixels tall in `color` above the window.
    #[must_use]
    pub const fn with_title_bar(mut self, height: u16, color: Rgba) -> Self {
        self.title_bar = TitleBar { height, color };
        self
    }

    /// Draws a shadow that fades out over `blur` pixels, moved by (`dx`, `dy`), in `color`.
    #[must_use]
    pub const fn with_shadow(mut self, blur: u16, dx: i16, dy: i16, color: Rgba) -> Self {
        self.shadow = Shadow {
            blur,
            dx,
            dy,
            color,
        };
        self
    }

    /// Corner radius in pixels.
    #[must_use]
    pub const fn radius(&self) -> u16 {
        self.radius
    }

    /// The border.
    #[must_use]
    pub const fn border(&self) -> Border {
        self.border
    }

    /// Opacity, 0 to 255.
    #[must_use]
    pub const fn opacity(&self) -> u8 {
        self.opacity
    }

    /// The band above the window.
    #[must_use]
    pub const fn title_bar(&self) -> TitleBar {
        self.title_bar
    }

    /// The shadow behind the window.
    #[must_use]
    pub const fn shadow(&self) -> Shadow {
        self.shadow
    }

    /// The animation.
    #[must_use]
    pub const fn transition(&self) -> Transition {
        self.transition
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_style_is_opaque_and_plain() {
        let style = Style::default();
        assert_eq!(style.opacity(), u8::MAX);
        assert_eq!(style.radius(), 0);
        assert_eq!(style.border(), Border::default());
        assert_eq!(style.transition().duration_ms, 0);
        assert_eq!(style.title_bar(), TitleBar::default());
        assert_eq!(style.shadow(), Shadow::default());
    }

    #[test]
    fn a_title_bar_and_a_shadow_are_set_apart_from_the_rest() {
        let color = Rgba::new(1, 2, 3, 4);
        let style = Style::default()
            .with_title_bar(20, color)
            .with_shadow(12, -3, 5, color);

        assert_eq!(style.title_bar(), TitleBar { height: 20, color });
        assert_eq!(
            style.shadow(),
            Shadow {
                blur: 12,
                dx: -3,
                dy: 5,
                color
            }
        );
        assert_eq!(style.radius(), 0);
        assert_ne!(style, Style::default());
    }

    #[test]
    fn builders_set_only_their_own_field() {
        let style = Style::default().with_radius(8);
        assert_eq!(style.radius(), 8);
        assert_eq!(style.opacity(), u8::MAX);
    }
}
