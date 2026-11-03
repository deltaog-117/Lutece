//! Drawing windows, their popups and the cursor, and telling clients when a frame was
//! shown.

use crate::scale::Scale;
use crate::windows::Layer;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::utils::CropRenderElement;
use smithay::backend::renderer::element::{Kind, render_elements};
use smithay::backend::renderer::{ImportAll, Renderer};
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::utils::{IsAlive, Logical, Point};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use std::sync::Mutex;

render_elements! {
    /// One thing to draw: a surface as its client made it, or a window's surface cut to the
    /// rectangle its window was given.
    pub(crate) SceneElement<R> where R: ImportAll;
    Surface=WaylandSurfaceRenderElement<R>,
    Cropped=CropRenderElement<WaylandSurfaceRenderElement<R>>,
}

/// What to draw for every placed window and its popups, front to back, which is the
/// order Smithay's drawing helpers expect. A hidden window is left out.
///
/// A window is drawn only inside the rectangle the core gave it, so a client that paints
/// a shadow or a frame of its own around its window cannot spill over its neighbours.
/// Popups are not cut, since a menu may reach past its window.
pub(crate) fn surface_elements<R>(
    layers: &[Layer],
    renderer: &mut R,
    scale: Scale,
) -> Vec<SceneElement<R>>
where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
    layers
        .iter()
        .flat_map(|layer| {
            let drawn: Vec<WaylandSurfaceRenderElement<R>> = render_elements_from_surface_tree(
                renderer,
                &layer.surface,
                scale.point_to_physical(layer.origin),
                scale.factor(),
                1.0,
                Kind::Unspecified,
            );
            let clip = layer.clip.map(|clip| scale.rect_to_physical(clip));
            drawn.into_iter().filter_map(move |element| match clip {
                Some(clip) => CropRenderElement::from_element(element, scale.factor(), clip)
                    .map(SceneElement::Cropped),
                None => Some(SceneElement::Surface(element)),
            })
        })
        .collect()
}

/// What to draw for the cursor when a client gave it an image of its own. Named and
/// hidden cursors are the window system's business.
pub(crate) fn cursor_elements<R>(
    status: &CursorImageStatus,
    location: Point<f64, Logical>,
    renderer: &mut R,
    scale: Scale,
) -> Vec<SceneElement<R>>
where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
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
