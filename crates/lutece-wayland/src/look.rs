//! Drawing a window's look: the rounded corners, the border and the opacity of its style.
//!
//! Corners are cut by a shader that replaces the one the renderer draws the window's
//! surfaces with, for the window's own draw calls only; the border is a ring drawn by a pixel
//! shader around the window. Both measure in framebuffer pixels, so they follow the
//! window however it is scaled while it moves.

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, GlesTexProgram, Uniform, UniformName,
    UniformType,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Transform};

const WINDOW_SHADER: &str = r"#version 100
//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

uniform vec4 clip_rect;
uniform float clip_radius;
uniform float fb_height;
uniform float flip;

float rounded_distance(vec2 p, vec2 half_size, float radius) {
    vec2 q = abs(p) - half_size + vec2(radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

void main() {
    vec4 color = texture2D(tex, v_coords);

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0) * alpha;
#else
    color = color * alpha;
#endif

    // Windows are placed from the top of the output. Seen on screen: with the frame drawn
    // flipped, gl_FragCoord's y counts from the bottom of the picture, so it is turned over.
    float y = flip > 0.5 ? fb_height - gl_FragCoord.y : gl_FragCoord.y;
    vec2 half_size = clip_rect.zw * 0.5;
    float d = rounded_distance(vec2(gl_FragCoord.x, y) - (clip_rect.xy + half_size), half_size, clip_radius);
    color = color * clamp(0.5 - d, 0.0, 1.0);

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
";

const BORDER_SHADER: &str = r"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

uniform float alpha;
uniform vec2 size;
varying vec2 v_coords;

uniform float radius;
uniform float width;
uniform vec4 color;

float rounded_distance(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    vec2 p = v_coords * size - size * 0.5;
    vec2 half_outer = size * 0.5;
    float outer = rounded_distance(p, half_outer, radius + width);
    float inner = rounded_distance(p, half_outer - vec2(width), radius);
    float ring = clamp(0.5 - outer, 0.0, 1.0) * (1.0 - clamp(0.5 - inner, 0.0, 1.0));
    gl_FragColor = color * ring * alpha;
}
";

/// A filled bar with rounded corners, for the title bar. Every corner is rounded, so it does
/// not depend on which way the framebuffer's y axis runs.
const BAR_SHADER: &str = r"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

uniform float alpha;
uniform vec2 size;
varying vec2 v_coords;

uniform float radius;
uniform vec4 color;

float rounded_distance(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    vec2 p = v_coords * size - size * 0.5;
    float d = rounded_distance(p, size * 0.5, radius);
    gl_FragColor = color * clamp(0.5 - d, 0.0, 1.0) * alpha;
}
";

/// A window's shadow: the window's rounded shape moved by an offset, fading out over `blur`
/// pixels, and cut away under the window itself. Positions are measured in framebuffer
/// pixels from the top, like the window shader's, so an offset down is down on the screen
/// however the frame is drawn.
const SHADOW_SHADER: &str = r"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

uniform float alpha;

uniform vec4 win_rect;
uniform vec2 offset;
uniform float blur;
uniform float radius;
uniform float fb_height;
uniform float flip;
uniform vec4 color;

float rounded_distance(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    float y = flip > 0.5 ? fb_height - gl_FragCoord.y : gl_FragCoord.y;
    vec2 p = vec2(gl_FragCoord.x, y);
    vec2 half_size = win_rect.zw * 0.5;
    vec2 center = win_rect.xy + half_size;
    float d = rounded_distance(p - (center + offset), half_size, radius);
    float t = 1.0 - clamp(d / max(blur, 1.0), 0.0, 1.0);
    float fade = t * t * (3.0 - 2.0 * t);
    float own = rounded_distance(p - center, half_size, radius);
    float outside = clamp(0.5 + own, 0.0, 1.0);
    gl_FragColor = color * fade * outside * alpha;
}
";

/// The shaders, compiled once for a renderer.
#[derive(Debug, Clone)]
pub(crate) struct Shaders {
    window: GlesTexProgram,
    border: GlesPixelProgram,
    bar: GlesPixelProgram,
    shadow: GlesPixelProgram,
}

impl Shaders {
    /// Compiles the shaders.
    ///
    /// # Errors
    ///
    /// Returns the renderer's error if a shader does not compile, as a driver without
    /// GLSL ES 1.00 would.
    pub(crate) fn compile(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let window = renderer.compile_custom_texture_shader(
            WINDOW_SHADER,
            &[
                UniformName::new("clip_rect", UniformType::_4f),
                UniformName::new("clip_radius", UniformType::_1f),
                UniformName::new("fb_height", UniformType::_1f),
                UniformName::new("flip", UniformType::_1f),
            ],
        )?;
        let border = renderer.compile_custom_pixel_shader(
            BORDER_SHADER,
            &[
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("width", UniformType::_1f),
                UniformName::new("color", UniformType::_4f),
            ],
        )?;
        let bar = renderer.compile_custom_pixel_shader(
            BAR_SHADER,
            &[
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("color", UniformType::_4f),
            ],
        )?;
        let shadow = renderer.compile_custom_pixel_shader(
            SHADOW_SHADER,
            &[
                UniformName::new("win_rect", UniformType::_4f),
                UniformName::new("offset", UniformType::_2f),
                UniformName::new("blur", UniformType::_1f),
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("fb_height", UniformType::_1f),
                UniformName::new("flip", UniformType::_1f),
                UniformName::new("color", UniformType::_4f),
            ],
        )?;
        Ok(Self {
            window,
            border,
            bar,
            shadow,
        })
    }

    /// The ring that is a window's border: `area` is the window grown by the border's width
    /// on every side, `radius` the window's corner radius and `color` the border colour
    /// with its alpha, all in physical pixels.
    pub(crate) fn border(
        &self,
        area: Rectangle<i32, Logical>,
        radius: f32,
        width: f32,
        color: [f32; 4],
        alpha: f32,
    ) -> PixelShaderElement {
        let premultiplied = premultiply(color);
        PixelShaderElement::new(
            self.border.clone(),
            area,
            None,
            alpha,
            vec![
                Uniform::new("radius", radius),
                Uniform::new("width", width),
                Uniform::new("color", premultiplied),
            ],
            Kind::Unspecified,
        )
    }

    /// The title bar: `area` is where it goes, `radius` the corner radius it is rounded by
    /// (physical pixels, already limited to half its height) and `color` its colour with its
    /// alpha.
    pub(crate) fn bar(
        &self,
        area: Rectangle<i32, Logical>,
        radius: f32,
        color: [f32; 4],
        alpha: f32,
    ) -> PixelShaderElement {
        PixelShaderElement::new(
            self.bar.clone(),
            area,
            None,
            alpha,
            vec![
                Uniform::new("radius", radius),
                Uniform::new("color", premultiply(color)),
            ],
            Kind::Unspecified,
        )
    }

    /// The shadow of a window: `area` is the box that holds all of it (see [`shadow_area`]),
    /// `window` the window's place and `radius` its corners, `offset` how far the shadow is
    /// moved and `blur` how far it fades, all in physical pixels. `fb_height` and `flip` are
    /// as for [`Self::rounded`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shadow(
        &self,
        area: Rectangle<i32, Logical>,
        window: Rectangle<i32, Physical>,
        radius: f32,
        offset: [f32; 2],
        blur: f32,
        color: [f32; 4],
        alpha: f32,
        fb_height: i32,
        flip: bool,
    ) -> PixelShaderElement {
        PixelShaderElement::new(
            self.shadow.clone(),
            area,
            None,
            alpha,
            vec![
                Uniform::new(
                    "win_rect",
                    [
                        window.loc.x as f32,
                        window.loc.y as f32,
                        window.size.w as f32,
                        window.size.h as f32,
                    ],
                ),
                Uniform::new("offset", offset),
                Uniform::new("blur", blur),
                Uniform::new("radius", radius),
                Uniform::new("fb_height", fb_height as f32),
                Uniform::new("flip", if flip { 1.0f32 } else { 0.0 }),
                Uniform::new("color", premultiply(color)),
            ],
            Kind::Unspecified,
        )
    }

    /// Wraps `inner` so its surfaces are drawn with the corners cut to `radius` (physical
    /// pixels) inside `rect`. `fb_height` is the framebuffer's height, and `flip` is true
    /// when the frame is drawn flipped, which turns the framebuffer's y axis over.
    pub(crate) fn rounded<E>(
        &self,
        inner: E,
        rect: Rectangle<i32, Physical>,
        radius: f32,
        fb_height: i32,
        flip: bool,
    ) -> Styled<E> {
        let uniforms = vec![
            Uniform::new(
                "clip_rect",
                [
                    rect.loc.x as f32,
                    rect.loc.y as f32,
                    rect.size.w as f32,
                    rect.size.h as f32,
                ],
            ),
            Uniform::new("clip_radius", radius),
            Uniform::new("fb_height", fb_height as f32),
            Uniform::new("flip", if flip { 1.0f32 } else { 0.0 }),
        ];
        Styled {
            inner,
            override_with: Some((self.window.clone(), uniforms)),
        }
    }
}

/// The shaders multiply by the colour as it is, so it is premultiplied by its alpha here.
fn premultiply(color: [f32; 4]) -> [f32; 4] {
    [
        color[0] * color[3],
        color[1] * color[3],
        color[2] * color[3],
        color[3],
    ]
}

/// Where the title bar goes: right above `drawn` and its border, as wide as both, `height`
/// tall.
pub(crate) fn bar_area(
    drawn: Rectangle<i32, Logical>,
    border: i32,
    height: i32,
) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (
            drawn.loc.x.saturating_sub(border),
            drawn.loc.y.saturating_sub(border).saturating_sub(height),
        )
            .into(),
        (
            drawn.size.w.saturating_add(border.saturating_mul(2)),
            height,
        )
            .into(),
    )
}

/// The corner radius of a title bar: the window's, but never more than half the bar's height,
/// which would make it more than a half circle at each end.
pub(crate) fn bar_radius(window_radius: f32, height: f32) -> f32 {
    window_radius.min(height * 0.5).max(0.0)
}

/// The box that holds a window's whole shadow: the window grown by the blur and by how far
/// the shadow is moved, on every side. It is larger than it needs to be on the side the
/// shadow is moved away from, which costs a few pixels of drawing and keeps the box simple.
pub(crate) fn shadow_area(
    drawn: Rectangle<i32, Logical>,
    blur: i32,
    dx: i32,
    dy: i32,
) -> Rectangle<i32, Logical> {
    let reach = blur.saturating_add(dx.saturating_abs().max(dy.saturating_abs()));
    Rectangle::new(
        (
            drawn.loc.x.saturating_sub(reach),
            drawn.loc.y.saturating_sub(reach),
        )
            .into(),
        (
            drawn.size.w.saturating_add(reach.saturating_mul(2)),
            drawn.size.h.saturating_add(reach.saturating_mul(2)),
        )
            .into(),
    )
}

/// An element drawn as it is, or with the corner-cutting shader in place of the default one.
#[derive(Debug)]
pub(crate) struct Styled<E> {
    inner: E,
    override_with: Option<(GlesTexProgram, Vec<Uniform<'static>>)>,
}

impl<E> Styled<E> {
    /// An element drawn with no change.
    pub(crate) const fn plain(inner: E) -> Self {
        Self {
            inner,
            override_with: None,
        }
    }
}

impl<E: Element> Element for Styled<E> {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        // Rounded corners are not opaque, so a window with them claims no region that a
        // renderer could skip drawing behind.
        if self.override_with.is_some() {
            OpaqueRegions::default()
        } else {
            self.inner.opaque_regions(scale)
        }
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl<E: RenderElement<GlesRenderer>> RenderElement<GlesRenderer> for Styled<E> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let Some((program, uniforms)) = &self.override_with else {
            return self.inner.draw(frame, src, dst, damage, opaque_regions);
        };
        frame.override_default_tex_program(program.clone(), uniforms.clone());
        let drawn = self.inner.draw(frame, src, dst, damage, opaque_regions);
        // The override would otherwise apply to every element drawn after this one.
        frame.clear_tex_program_override();
        drawn
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn area() -> impl Strategy<Value = Rectangle<i32, Logical>> {
        (-2000i32..2000, -2000i32..2000, 1i32..3000, 1i32..3000)
            .prop_map(|(x, y, w, h)| Rectangle::new((x, y).into(), (w, h).into()))
    }

    proptest! {
        /// The box of a shadow holds the window moved by the offset and grown by the blur,
        /// which is everything the shader can paint.
        #[test]
        fn the_shadow_box_holds_the_whole_shadow(
            drawn in area(),
            blur in 0i32..200,
            dx in -200i32..200,
            dy in -200i32..200,
        ) {
            let held = shadow_area(drawn, blur, dx, dy);
            let (left, top) = (drawn.loc.x + dx - blur, drawn.loc.y + dy - blur);
            let (right, bottom) = (
                drawn.loc.x + drawn.size.w + dx + blur,
                drawn.loc.y + drawn.size.h + dy + blur,
            );
            prop_assert!(held.loc.x <= left && held.loc.y <= top);
            prop_assert!(held.loc.x + held.size.w >= right);
            prop_assert!(held.loc.y + held.size.h >= bottom);
        }

        /// The title bar sits directly above the bordered window and is as wide as it.
        #[test]
        fn the_bar_sits_above_the_bordered_window(
            drawn in area(),
            border in 0i32..40,
            height in 1i32..120,
        ) {
            let bar = bar_area(drawn, border, height);
            prop_assert_eq!(bar.loc.y + bar.size.h, drawn.loc.y - border);
            prop_assert_eq!(bar.loc.x, drawn.loc.x - border);
            prop_assert_eq!(bar.size.w, drawn.size.w + 2 * border);
            prop_assert_eq!(bar.size.h, height);
        }

        /// A bar is never rounded past a half circle at its ends, and never by less than zero.
        #[test]
        fn the_bar_radius_stays_within_half_the_height(radius in 0.0f32..500.0, height in 1.0f32..200.0) {
            let rounded = bar_radius(radius, height);
            prop_assert!(rounded >= 0.0);
            prop_assert!(rounded <= height * 0.5);
            prop_assert!(rounded <= radius);
        }
    }

    #[test]
    fn a_shadow_box_at_the_edge_of_the_range_does_not_overflow() {
        let area = Rectangle::new((i32::MIN, i32::MAX).into(), (i32::MAX, i32::MAX).into());

        let held = shadow_area(area, i32::MAX, i32::MIN, i32::MAX);

        assert_eq!(held.loc.x, i32::MIN);
        assert_eq!(held.size.w, i32::MAX);
    }

    #[test]
    fn premultiplying_scales_the_colour_by_its_alpha() {
        assert_eq!(premultiply([1.0, 0.5, 0.0, 0.5]), [0.5, 0.25, 0.0, 0.5]);
    }
}
