//! Drawing windows, their popups and the cursor, and telling clients when a frame was
//! shown.

use crate::look::{Shaders, Styled};
use crate::scale::Scale;
use crate::windows::{Layer, Look};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::utils::{CropRenderElement, RescaleRenderElement};
use smithay::backend::renderer::element::{Kind, render_elements};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::utils::Scale as ElementScale;
use smithay::utils::{IsAlive, Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use std::sync::Mutex;

/// A window's surface as drawn: cut to its rectangle, scaled while it moves, and with its
/// corners cut if its style asks for them.
type WindowSurface =
    Styled<CropRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>>;

render_elements! {
    /// One thing to draw: a surface as its client made it, a window's surface in its
    /// style, or the border around a window.
    pub(crate) SceneElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Window=WindowSurface,
    Border=PixelShaderElement,
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
    frame: Size<i32, Physical>,
    flipped: bool,
) -> Vec<SceneElement> {
    let mut elements = Vec::new();
    for layer in layers {
        match &layer.look {
            Some(look) => {
                elements.extend(window_elements(
                    layer, look, renderer, scale, shaders, frame, flipped,
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

fn window_elements(
    layer: &Layer,
    look: &Look,
    renderer: &mut GlesRenderer,
    scale: Scale,
    shaders: &Shaders,
    frame: Size<i32, Physical>,
    flipped: bool,
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

    // The client sized the surface for `target`; it is drawn at `drawn`.
    let shown_size_over_asked = ElementScale::from((
        ratio(look.drawn.size.w, look.target.size.w),
        ratio(look.drawn.size.h, look.target.size.h),
    ));
    let drawn_origin = scale.point_to_physical(look.drawn.loc);
    for element in render_elements_from_surface_tree(
        renderer,
        &layer.surface,
        scale.point_to_physical(layer.origin),
        factor,
        opacity,
        Kind::Unspecified,
    ) {
        let rescaled =
            RescaleRenderElement::from_element(element, drawn_origin, shown_size_over_asked);
        let Some(cropped) = CropRenderElement::from_element(rescaled, factor, drawn) else {
            continue;
        };
        let element = if radius > 0.0 {
            shaders.rounded(cropped, drawn, radius as f32, frame.h, flipped)
        } else {
            Styled::plain(cropped)
        };
        elements.push(SceneElement::Window(element));
    }
    elements
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
