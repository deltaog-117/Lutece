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

/// Which of a window's changes glide with the window's [`Transition`] instead of jumping.
///
/// A set of independent kinds. The default is [`Glide::MOVES`] alone, which is what a window
/// did before this type existed: it glides to a new rectangle and everything else jumps. A
/// duration of 0 turns every kind off, whatever the set holds.
///
/// # Examples
///
/// ```
/// use lutece::Glide;
///
/// let glide = Glide::MOVES.with(Glide::OPACITY);
/// assert!(glide.contains(Glide::OPACITY));
/// assert!(!glide.contains(Glide::RADIUS));
/// assert_eq!(Glide::default(), Glide::MOVES);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Glide(u8);

impl Glide {
    /// Nothing glides.
    pub const NONE: Self = Self(0);
    /// A move or resize to another rectangle glides.
    pub const MOVES: Self = Self(1);
    /// A change of opacity glides.
    pub const OPACITY: Self = Self(1 << 1);
    /// A change of corner radius glides.
    pub const RADIUS: Self = Self(1 << 2);
    /// A window that appears (opens, or comes back with its workspace) fades in from
    /// invisible to its opacity.
    pub const APPEARS: Self = Self(1 << 3);
    /// A window that is hidden slides off its screen, and one that is shown again slides
    /// back in, instead of vanishing and popping up. A window that has never been shown does
    /// not slide: it is new, not returning.
    pub const SLIDES: Self = Self(1 << 4);

    /// Every kind this version knows.
    const KNOWN: u8 = 0b1_1111;

    /// This set with the kinds of `other` added.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every kind of `other` is in this set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The set as a number, one bit per kind, for a file or a message.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The set from a number written by [`bits`](Self::bits). A bit this version does not
    /// know is dropped, so a file from a newer version still reads.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & Self::KNOWN)
    }
}

impl Default for Glide {
    fn default() -> Self {
        Self::MOVES
    }
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
    glide: Glide,
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
            glide: Glide::default(),
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

    /// Sets which changes of this window glide with its transition.
    #[must_use]
    pub const fn with_glide(mut self, glide: Glide) -> Self {
        self.glide = glide;
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

    /// Which changes glide with the animation.
    #[must_use]
    pub const fn glide(&self) -> Glide {
        self.glide
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
        assert_eq!(style.glide(), Glide::MOVES);
        assert_eq!(style.title_bar(), TitleBar::default());
        assert_eq!(style.shadow(), Shadow::default());
    }

    #[test]
    fn a_glide_set_holds_what_was_put_in_and_nothing_else() {
        let set = Glide::NONE.with(Glide::OPACITY).with(Glide::APPEARS);
        assert!(set.contains(Glide::OPACITY) && set.contains(Glide::APPEARS));
        assert!(!set.contains(Glide::MOVES) && !set.contains(Glide::RADIUS));
        assert!(set.contains(Glide::NONE));
        assert_eq!(Glide::NONE.bits(), 0);
    }

    #[test]
    fn bits_a_newer_version_wrote_are_dropped_on_reading() {
        let all = Glide::MOVES
            .with(Glide::OPACITY)
            .with(Glide::RADIUS)
            .with(Glide::APPEARS)
            .with(Glide::SLIDES);
        assert_eq!(Glide::from_bits(all.bits()), all);
        assert_eq!(Glide::from_bits(0b1110_0000), Glide::NONE);
        assert_eq!(Glide::from_bits(0b1110_0010), Glide::OPACITY);
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
