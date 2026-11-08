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

/// The shaders, compiled once for a renderer.
#[derive(Debug, Clone)]
pub(crate) struct Shaders {
    window: GlesTexProgram,
    border: GlesPixelProgram,
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
        Ok(Self { window, border })
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
        // The shader multiplies by the colour as it is, so it is premultiplied here.
        let premultiplied = [
            color[0] * color[3],
            color[1] * color[3],
            color[2] * color[3],
            color[3],
        ];
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
