//! Drawing windows, their popups and the cursor, and telling clients when a frame was
//! shown.

use crate::windows::Windows;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::{ImportAll, Renderer};
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::utils::{IsAlive, Logical, Point};
use smithay::wayland::compositor::{
    SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
};
use std::sync::Mutex;

/// What to draw for every placed window and its popups, front to back, which is the
/// order Smithay's drawing helpers expect. A hidden window is left out.
pub(crate) fn surface_elements<R>(
    windows: &Windows,
    renderer: &mut R,
) -> Vec<WaylandSurfaceRenderElement<R>>
where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
    windows
        .layers()
        .into_iter()
        .flat_map(|layer| {
            render_elements_from_surface_tree(
                renderer,
                &layer.surface,
                (layer.origin.x, layer.origin.y),
                1.0,
                1.0,
                Kind::Unspecified,
            )
        })
        .collect()
}

/// What to draw for the cursor when a client gave it an image of its own. Named and
/// hidden cursors are the window system's business.
pub(crate) fn cursor_elements<R>(
    status: &CursorImageStatus,
    location: Point<f64, Logical>,
    renderer: &mut R,
) -> Vec<WaylandSurfaceRenderElement<R>>
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
        (origin.x, origin.y),
        1.0,
        1.0,
        Kind::Cursor,
    )
}

/// Answers the frame callbacks of every window and popup. A client asks for one to
/// learn when it may draw again; without the answer it would draw once and wait forever.
pub(crate) fn send_frame_callbacks(windows: &Windows, time_ms: u32) {
    for layer in windows.layers() {
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
