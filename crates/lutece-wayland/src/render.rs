//! Drawing windows, their popups and the cursor, and telling clients when a frame was
//! shown.

use crate::effect::{EffectInput, Phase};
use crate::ghost::{Ghost, faded, shrunk};
use crate::look::{Shaders, Styled, bar_radius, shadow_area};
use crate::primitives::{Primitive, TextSource};
use crate::scale::Scale;
use crate::title::TitleCache;
use crate::windows::{Layer, Look};
use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::{CropRenderElement, RescaleRenderElement};
use smithay::backend::renderer::element::{Id as ElementId, Kind, RenderElement, render_elements};
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::utils::Scale as ElementScale;
use smithay::utils::{IsAlive, Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use std::sync::Mutex;
use std::time::Instant;

/// A window's surface as drawn: cut to its rectangle, scaled while it moves, and with its
/// corners cut if its style asks for them.
type WindowSurface =
    Styled<CropRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>>;

/// A closed window's last picture as drawn while it goes: the same cut as a live window's.
type GhostSurface =
    Styled<CropRenderElement<RescaleRenderElement<TextureRenderElement<GlesTexture>>>>;

render_elements! {
    /// One thing to draw: a surface as its client made it, a window's surface in its
    /// style, the border around a window, or a picture the compositor made itself (a title,
    /// the pointer).
    pub(crate) SceneElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Window=WindowSurface,
    Ghost=GhostSurface,
    Border=PixelShaderElement,
    Picture=MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// The part of the compositor's space one screen shows: where its top left corner is, in
/// the logical pixels the core uses, and how many physical pixels each logical one is there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct View {
    pub(crate) origin: Point<i32, Logical>,
    pub(crate) scale: Scale,
}

impl View {
    /// The view of a screen that starts at the corner of the space.
    pub(crate) fn whole(scale: Scale) -> Self {
        Self {
            origin: Point::from((0, 0)),
            scale,
        }
    }
}

/// What to draw for every placed window and its popups, front to back, which is the
/// order Smithay's drawing helpers expect. A hidden window is left out.
///
/// A window is drawn only inside the rectangle it is drawn in, so a client that paints a
/// shadow or a frame of its own around its window cannot spill over its neighbours. While
/// a window moves, that rectangle is between the old and the new one, and the window's
/// surface, which its client sized for the new one, is scaled to fit. Popups are not cut,
/// since a menu may reach past its window.
///
/// `frame` is the size of what is drawn on, and `flipped` whether it is drawn flipped.
pub(crate) fn surface_elements(
    layers: &[Layer],
    renderer: &mut GlesRenderer,
    scale: Scale,
    shaders: &Shaders,
    titles: &mut TitleCache,
    frame: Size<i32, Physical>,
    flipped: bool,
) -> Vec<SceneElement> {
    let mut elements = Vec::new();
    for layer in layers {
        match &layer.look {
            Some(look) => {
                elements.extend(window_elements(
                    layer, look, renderer, scale, shaders, titles, frame, flipped,
                ));
            }
            None => elements.extend(plain_elements(layer, renderer, scale)),
        }
    }
    elements
}

/// A popup, a menu or a layer surface: as the client drew it.
fn plain_elements(layer: &Layer, renderer: &mut GlesRenderer, scale: Scale) -> Vec<SceneElement> {
    render_elements_from_surface_tree(
        renderer,
        &layer.surface,
        scale.point_to_physical(layer.origin),
        scale.factor(),
        1.0,
        Kind::Unspecified,
    )
    .into_iter()
    .map(SceneElement::Surface)
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn window_elements(
    layer: &Layer,
    look: &Look,
    renderer: &mut GlesRenderer,
    scale: Scale,
    shaders: &Shaders,
    titles: &mut TitleCache,
    frame: Size<i32, Physical>,
    flipped: bool,
) -> Vec<SceneElement> {
    let opacity = f32::from(look.style.opacity()) / f32::from(u8::MAX);
    decorated(
        look,
        renderer,
        scale,
        shaders,
        titles,
        frame,
        flipped,
        |renderer, out| {
            for element in render_elements_from_surface_tree(
                renderer,
                &layer.surface,
                scale.point_to_physical(layer.origin),
                scale.factor(),
                opacity,
                Kind::Unspecified,
            ) {
                out.extend(
                    style_surface(element, look, scale, shaders, frame, flipped)
                        .map(SceneElement::Window),
                );
            }
        },
    )
}

/// What to draw for the ghosts of closed windows, in front of everything else, on the screen
/// `view` describes. Each is the window's last picture and its decorations, faded and shrunk
/// by how far its close has gone.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ghost_elements(
    ghosts: &[Ghost],
    now: Instant,
    renderer: &mut GlesRenderer,
    shaders: &Shaders,
    titles: &mut TitleCache,
    frame: Size<i32, Physical>,
    flipped: bool,
    view: View,
) -> Vec<SceneElement> {
    let scale = view.scale;
    // Newest first: elements run front to back.
    ghosts
        .iter()
        .rev()
        .flat_map(|ghost| {
            let progress = ghost.timing.progress_at(now);
            let snapshot = &ghost.snapshot;
            let mut look = snapshot.look.shifted(view.origin);
            look.style = look
                .style
                .with_opacity(faded(look.style.opacity(), progress));
            look.drawn = shrunk(look.drawn, progress);
            look.effect = EffectInput {
                // Within 0 to 1 by the clamp in `progress_at`.
                progress: progress as f32,
                phase: Phase::Close,
            };
            let opacity = f32::from(look.style.opacity()) / f32::from(u8::MAX);
            let picture = &snapshot.picture;
            let origin = picture.origin - view.origin;
            let id: &ElementId = &ghost.id;
            decorated(
                &look,
                renderer,
                scale,
                shaders,
                titles,
                frame,
                flipped,
                |renderer, out| {
                    let element = TextureRenderElement::from_static_texture(
                        id.clone(),
                        renderer.context_id(),
                        scale.point_to_physical(origin).to_f64(),
                        picture.texture.clone(),
                        picture.scale,
                        picture.transform,
                        Some(opacity),
                        None,
                        Some(picture.size),
                        None,
                        Kind::Unspecified,
                    );
                    out.extend(
                        style_surface(element, &look, scale, shaders, frame, flipped)
                            .map(SceneElement::Ghost),
                    );
                },
            )
        })
        .collect()
}

/// A window's decorations around its surface: the border and title bar in front of it, the
/// shadow behind. `surfaces` pushes the surface itself, which is what differs between a live
/// window and a ghost of one.
#[allow(clippy::too_many_arguments)]
fn decorated(
    look: &Look,
    renderer: &mut GlesRenderer,
    scale: Scale,
    shaders: &Shaders,
    titles: &mut TitleCache,
    frame: Size<i32, Physical>,
    flipped: bool,
    surfaces: impl FnOnce(&mut GlesRenderer, &mut Vec<SceneElement>),
) -> Vec<SceneElement> {
    let style = look.style;
    let opacity = f32::from(style.opacity()) / f32::from(u8::MAX);
    let drawn = scale.rect_to_physical(look.drawn);
    let factor = scale.factor();
    let radius = f64::from(style.radius()) * factor;
    let border = style.border();
    let border_width = f64::from(border.width) * factor;

    let mut elements = Vec::new();
    if border.width > 0 && border.color.a > 0 {
        let grown = grow(look.drawn, i32::from(border.width));
        let c = border.color;
        elements.push(SceneElement::Border(shaders.border(
            grown,
            radius as f32,
            border_width as f32,
            [
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
                f32::from(c.a) / 255.0,
            ],
            opacity,
        )));
    }

    // A look a program gave replaces the style's bar; otherwise the bar is drawn from the same
    // primitives, so there is one way to draw it.
    let title_bar = style.title_bar();
    let builtin;
    let primitives: &[Primitive] = match &look.primitives {
        Some(own) => own.primitives(),
        None if title_bar.height > 0 && title_bar.color.a > 0 => {
            builtin = crate::primitives::title_bar(
                title_bar.height,
                title_bar.color,
                border.width,
                style.radius(),
            );
            &builtin
        }
        None => &[],
    };
    for primitive in primitives {
        primitive_elements(
            primitive,
            look,
            renderer,
            scale,
            titles,
            opacity,
            &mut elements,
            shaders,
        );
    }

    surfaces(renderer, &mut elements);

    // Last, so it is behind the window and the bar, in front of what is below.
    let shadow = style.shadow();
    if shadow.blur > 0 && shadow.color.a > 0 {
        let area = shadow_area(
            look.drawn,
            i32::from(shadow.blur),
            i32::from(shadow.dx),
            i32::from(shadow.dy),
        );
        elements.push(SceneElement::Border(shaders.shadow(
            area,
            drawn,
            radius as f32,
            [
                (f64::from(shadow.dx) * factor) as f32,
                (f64::from(shadow.dy) * factor) as f32,
            ],
            (f64::from(shadow.blur) * factor) as f32,
            channels(shadow.color),
            opacity,
            frame.h,
            flipped,
        )));
    }
    elements
}

/// One element of a window's surface, scaled from the size its client drew it for to the
/// rectangle it is drawn in, cut to that rectangle, and with its corners cut if the style asks
/// for them. `None` when nothing of it falls inside.
fn style_surface<E: RenderElement<GlesRenderer>>(
    element: E,
    look: &Look,
    scale: Scale,
    shaders: &Shaders,
    frame: Size<i32, Physical>,
    flipped: bool,
) -> Option<Styled<CropRenderElement<RescaleRenderElement<E>>>> {
    let drawn = scale.rect_to_physical(look.drawn);
    let factor = scale.factor();
    let radius = f64::from(look.style.radius()) * factor;
    // The client sized the surface for `target`; it is drawn at `drawn`.
    let shown_size_over_asked = ElementScale::from((
        ratio(look.drawn.size.w, look.target.size.w),
        ratio(look.drawn.size.h, look.target.size.h),
    ));
    let drawn_origin = scale.point_to_physical(look.drawn.loc);
    let rescaled = RescaleRenderElement::from_element(element, drawn_origin, shown_size_over_asked);
    let cropped = CropRenderElement::from_element(rescaled, factor, drawn)?;
    // A user's effect draws every window, even one with square corners.
    Some(if radius > 0.0 || shaders.has_effect() {
        shaders.rounded(cropped, drawn, radius as f32, frame.h, flipped, look.effect)
    } else {
        Styled::plain(cropped)
    })
}

/// Draws one primitive of a window's look, in front of what was pushed after it.
#[allow(clippy::too_many_arguments)]
fn primitive_elements(
    primitive: &Primitive,
    look: &Look,
    renderer: &mut GlesRenderer,
    scale: Scale,
    titles: &mut TitleCache,
    opacity: f32,
    out: &mut Vec<SceneElement>,
    shaders: &Shaders,
) {
    let factor = scale.factor();
    match primitive {
        Primitive::Rect {
            area,
            radius,
            color,
        } => {
            let area = area.resolve(look.drawn);
            if area.size.w <= 0 || area.size.h <= 0 || color.a == 0 {
                return;
            }
            // Never rounder than half the height, so a band is a band and not a blob.
            let rounded = bar_radius(
                (f64::from(*radius) * factor) as f32,
                (f64::from(area.size.h) * factor) as f32,
            );
            out.push(SceneElement::Border(shaders.bar(
                area,
                rounded,
                channels(*color),
                opacity,
            )));
        }
        Primitive::Text { area, source, over } => {
            let text = match source {
                TextSource::Title => look.title.as_str(),
                TextSource::Literal(text) => text.as_str(),
            };
            out.extend(title_element(
                text,
                area.resolve(look.drawn),
                *over,
                renderer,
                scale,
                titles,
                opacity,
            ));
        }
    }
}

/// The title written on a bar of `area` (logical), if there is a title and room for it.
fn title_element(
    title: &str,
    area: Rectangle<i32, Logical>,
    bar_color: lutece::Rgba,
    renderer: &mut GlesRenderer,
    scale: Scale,
    titles: &mut TitleCache,
    opacity: f32,
) -> Option<SceneElement> {
    let physical = scale.rect_to_physical(area);
    let width = u32::try_from(physical.size.w).ok()?;
    let height = u32::try_from(physical.size.h).ok()?;
    let buffer = titles.buffer(title, width, height, bar_color)?;
    // A title that cannot be uploaded leaves the bar a plain band; it must not cost the frame.
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        physical.loc.to_f64(),
        &buffer,
        Some(opacity),
        None,
        Some(area.size),
        Kind::Unspecified,
    )
    .ok()
    .map(SceneElement::Picture)
}

/// A colour's channels as the shaders take them, each from 0 to 1.
fn channels(color: lutece::Rgba) -> [f32; 4] {
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    ]
}

/// `shown / asked` as a number; one for an empty size, which is never drawn anyway.
fn ratio(shown: i32, asked: i32) -> f64 {
    if asked <= 0 {
        1.0
    } else {
        f64::from(shown) / f64::from(asked)
    }
}

/// `area` grown by `by` pixels on every side.
fn grow(area: Rectangle<i32, Logical>, by: i32) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (area.loc.x.saturating_sub(by), area.loc.y.saturating_sub(by)).into(),
        (
            area.size.w.saturating_add(by.saturating_mul(2)),
            area.size.h.saturating_add(by.saturating_mul(2)),
        )
            .into(),
    )
}

/// What to draw for the cursor when a client gave it an image of its own. Named and
/// hidden cursors are the window system's business.
pub(crate) fn cursor_elements(
    status: &CursorImageStatus,
    location: Point<f64, Logical>,
    renderer: &mut GlesRenderer,
    scale: Scale,
) -> Vec<SceneElement> {
    let CursorImageStatus::Surface(surface) = status else {
        return Vec::new();
    };
    if !surface.alive() {
        return Vec::new();
    }
    let hotspot = with_states(surface, |states| {
        states
            .data_map
            .get::<Mutex<CursorImageAttributes>>()
            .and_then(|attributes| attributes.lock().ok().map(|attributes| attributes.hotspot))
            .unwrap_or_default()
    });
    // The hotspot is the point of the image that sits on the pointer.
    let origin = location.to_i32_round::<i32>() - hotspot;
    render_elements_from_surface_tree(
        renderer,
        surface,
        scale.point_to_physical(origin),
        scale.factor(),
        1.0,
        Kind::Cursor,
    )
    .into_iter()
    .map(SceneElement::Surface)
    .collect()
}

/// Answers the frame callbacks of every window and popup. A client asks for one to
/// learn when it may draw again; without the answer it would draw once and wait forever.
pub(crate) fn send_frame_callbacks(layers: &[Layer], time_ms: u32) {
    for layer in layers {
        with_surface_tree_downward(
            &layer.surface,
            (),
            |_, _, ()| TraversalAction::DoChildren(()),
            |_, states, ()| {
                for callback in states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .frame_callbacks
                    .drain(..)
                {
                    callback.done(time_ms);
                }
            },
            |_, _, ()| true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_drawn_at_its_asked_size_is_not_scaled() {
        assert!((ratio(400, 400) - 1.0).abs() < f64::EPSILON);
        assert!((ratio(200, 400) - 0.5).abs() < f64::EPSILON);
        assert!((ratio(800, 400) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_empty_asked_size_is_not_a_division_by_zero() {
        assert!((ratio(10, 0) - 1.0).abs() < f64::EPSILON);
        assert!((ratio(10, -5) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_border_grows_the_window_on_every_side() {
        let area = Rectangle::new((10, 20).into(), (100, 50).into());

        let grown = grow(area, 3);

        assert_eq!(grown, Rectangle::new((7, 17).into(), (106, 56).into()));
    }

    #[test]
    fn growing_at_the_edge_of_the_range_does_not_overflow() {
        let area = Rectangle::new((i32::MIN, i32::MAX).into(), (i32::MAX, i32::MAX).into());

        let grown = grow(area, i32::MAX);

        assert_eq!(grown.loc.x, i32::MIN);
        assert_eq!(grown.size.w, i32::MAX);
    }
}
