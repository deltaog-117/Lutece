//! Layer-shell: bars, launchers and wallpapers.
//!
//! A layer surface is not a window. Its client says where it wants to sit (an edge, a
//! size, how much space to keep clear), and the compositor places it, so the core never
//! hears of it as a window. What the core does hear of is the space the surfaces keep
//! clear, as [`Event::ReservedChanged`], so the windows it lays out leave them room.
//!
//! Smithay's layer map does the placing and works out what is left free; this module
//! turns that into insets for the core, decides what is in front of what, and gives the
//! keyboard to a surface that asks for it.

use crate::compositor::State;
use crate::focus::Focus;
use crate::windows::Layer;
use lutece::{Event, Insets, OutputId};
use smithay::delegate_layer_shell;
use smithay::desktop::{
    LayerSurface, PopupKind, PopupManager, WindowSurfaceType, layer_map_for_output,
};
use smithay::output::Output as WlOutput;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput as WlOutputResource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::{get_parent, with_states};
use smithay::wayland::shell::wlr_layer::{
    KeyboardInteractivity, Layer as WlrLayer, LayerSurface as WlrLayerSurface, LayerSurfaceData,
    WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::wayland::shell::xdg::PopupSurface;

/// What the layer surfaces keep clear of an output of `output` pixels, given the area
/// `zone` they leave free. Both are in the output's own coordinates.
///
/// A side on which the zone reaches the output's edge, or passes it, reserves nothing. The
/// core ignores claims on most of an axis anyway.
pub(crate) fn insets_of(zone: Rectangle<i32, Logical>, output: Size<i32, Logical>) -> Insets {
    let edge = |pixels: i32| u32::try_from(pixels.max(0)).unwrap_or(0);
    Insets::new(
        edge(zone.loc.x),
        edge(zone.loc.y),
        edge(output.w - (zone.loc.x + zone.size.w)),
        edge(output.h - (zone.loc.y + zone.size.h)),
    )
}

/// The size of `output` in logical pixels, worked out the way Smithay's layer map does,
/// so the zone it reports and this size are in the same terms.
fn logical_size(output: &WlOutput) -> Size<i32, Logical> {
    output
        .current_mode()
        .map(|mode| {
            let logical = mode
                .size
                .to_f64()
                .to_logical(output.current_scale().fractional_scale())
                .to_i32_round();
            output.current_transform().transform_size(logical)
        })
        .unwrap_or_default()
}

impl State {
    /// Everything on screen from front to back.
    ///
    /// X11 menus come first, then overlay surfaces (lock screens, notifications), then
    /// fullscreen windows, then the top surfaces (bars), then the other windows, then the
    /// bottom surfaces and the wallpaper. A fullscreen window hides the bars, as it does on
    /// other compositors, but not an overlay.
    pub(crate) fn layers(&self) -> Vec<Layer> {
        let mut layers = self.windows.menu_layers();
        layers.extend(self.layer_surfaces(WlrLayer::Overlay));
        layers.extend(self.windows.window_layers(true));
        layers.extend(self.layer_surfaces(WlrLayer::Top));
        layers.extend(self.windows.window_layers(false));
        layers.extend(self.layer_surfaces(WlrLayer::Bottom));
        layers.extend(self.layer_surfaces(WlrLayer::Background));
        layers
    }

    /// The surfaces on `layer` of every output, front to back, each after its popups.
    fn layer_surfaces(&self, layer: WlrLayer) -> Vec<Layer> {
        let mut found = Vec::new();
        for (_, output) in &self.output_ids {
            let origin: Point<i32, Logical> = output.current_location();
            let map = layer_map_for_output(output);
            for surface in map.layers_on(layer).rev() {
                let Some(geometry) = map.layer_geometry(surface) else {
                    continue;
                };
                // The geometry is the box of the whole surface tree, which can reach left
                // of or above the root surface; the root itself is what is placed.
                let root = origin + geometry.loc - surface.bbox().loc;
                let popups = PopupManager::popups_for_surface(surface.wl_surface())
                    .map(|(popup, location)| Layer {
                        id: None,
                        surface: popup.wl_surface().clone(),
                        origin: root + location - popup.geometry().loc,
                        popup: true,
                        look: None,
                    })
                    .collect::<Vec<_>>();
                found.extend(popups.into_iter().rev());
                found.push(Layer {
                    id: None,
                    surface: surface.wl_surface().clone(),
                    origin: root,
                    popup: false,
                    look: None,
                });
            }
        }
        found
    }

    /// Arranges the surfaces of every output again, after the outputs or the scale
    /// changed, and tells the core what they reserve.
    pub(crate) fn arrange_layers(&mut self) {
        for (_, output) in &self.output_ids {
            layer_map_for_output(output).arrange();
        }
        self.layers_changed();
    }

    /// Asks the surfaces on the outputs that are about to go away to close, as the
    /// protocol wants: their clients can open new ones on the outputs that remain.
    pub(crate) fn close_layers_of(&self, gone: &[WlOutput]) {
        for output in gone {
            for surface in layer_map_for_output(output).layers() {
                surface.layer_surface().send_close();
            }
        }
    }

    /// Handles a commit of `surface` if it is a layer surface: arranges, and sends the
    /// first configure that lets it map.
    pub(crate) fn commit_layer(&mut self, surface: &WlSurface) {
        let Some(output) = self.layer_output(surface) else {
            return;
        };
        {
            let mut map = layer_map_for_output(&output);
            // Before the first configure, so it carries the size the client just asked for.
            map.arrange();
            let configured = with_states(surface, |states| {
                states
                    .data_map
                    .get::<LayerSurfaceData>()
                    .and_then(|data| data.lock().ok().map(|data| data.initial_configure_sent))
                    .unwrap_or(true)
            });
            if !configured
                && let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            {
                layer.layer_surface().send_configure();
            }
        }
        self.layers_changed();
    }

    /// Forgets the layers whose clients are gone.
    pub(crate) fn cleanup_layers(&mut self) {
        let mut changed = false;
        for (_, output) in &self.output_ids {
            let mut map = layer_map_for_output(output);
            let before = map.len();
            map.cleanup();
            changed |= map.len() != before;
        }
        if changed {
            self.layers_changed();
        }
    }

    /// The output a layer surface is mapped on.
    fn layer_output(&self, surface: &WlSurface) -> Option<WlOutput> {
        self.output_ids.iter().find_map(|(_, output)| {
            layer_map_for_output(output)
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .map(|_| output.clone())
        })
    }

    /// The layer surface `surface` belongs to, as its root, a subsurface or a popup.
    pub(crate) fn layer_of(&self, surface: &WlSurface) -> Option<LayerSurface> {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if let Some(popup) = self.popups.find_popup(&root)
            && let Ok(parent) = smithay::desktop::find_popup_root_surface(&popup)
        {
            root = parent;
        }
        self.output_ids.iter().find_map(|(_, output)| {
            layer_map_for_output(output)
                .layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                .cloned()
        })
    }

    /// Everything that depends on which layer surfaces exist and where.
    fn layers_changed(&mut self) {
        self.refresh_reserved();
        self.refresh_layer_scale();
        self.refresh_keyboard_focus();
    }

    /// Tells the core what the layer surfaces keep clear on each output, for the outputs
    /// where that differs from what it was last told.
    pub(crate) fn refresh_reserved(&mut self) {
        let now: Vec<(OutputId, Insets)> = self
            .output_ids
            .iter()
            .map(|(id, output)| {
                let zone = layer_map_for_output(output).non_exclusive_zone();
                (*id, insets_of(zone, logical_size(output)))
            })
            .collect();

        let mut events: Vec<Event> = now
            .iter()
            .filter(|(id, insets)| self.reserved.get(id).copied().unwrap_or_default() != *insets)
            .map(|&(output, insets)| Event::ReservedChanged { output, insets })
            .collect();
        // The core keeps what it was told about an output that is gone, so it is reset.
        let mut gone: Vec<OutputId> = self
            .reserved
            .keys()
            .copied()
            .filter(|id| now.iter().all(|(present, _)| present != id))
            .collect();
        gone.sort_by_key(|id| id.get());
        events.extend(gone.into_iter().map(|output| Event::ReservedChanged {
            output,
            insets: Insets::NONE,
        }));

        self.reserved = now
            .into_iter()
            .filter(|(_, insets)| !insets.is_none())
            .collect();
        self.events.extend(events);
    }

    /// Tells every layer surface which integer scale to draw at.
    fn refresh_layer_scale(&self) {
        for (_, output) in &self.output_ids {
            let map = layer_map_for_output(output);
            for layer in map.layers() {
                self.tell_scale(layer.wl_surface());
            }
        }
    }

    /// The layer surface that holds the keyboard whatever else happens: one on the top or
    /// overlay layer that asked for exclusive interactivity and has something to show.
    fn exclusive_layer(&self) -> Option<WlSurface> {
        [WlrLayer::Overlay, WlrLayer::Top]
            .into_iter()
            .find_map(|layer| {
                self.output_ids.iter().find_map(|(_, output)| {
                    layer_map_for_output(output)
                        .layers_on(layer)
                        .find(|surface| {
                            surface.alive()
                                && surface.cached_state().keyboard_interactivity
                                    == KeyboardInteractivity::Exclusive
                                && !surface.bbox().is_empty()
                        })
                        .map(|surface| surface.wl_surface().clone())
                })
            })
    }

    /// Gives the keyboard to the layer surface that should have it, or else to the window
    /// the core last focused.
    ///
    /// An exclusive surface wins over everything. Otherwise a surface the pointer clicked
    /// keeps the keyboard until the pointer enters a window, which is when keyboard focus
    /// goes back to following the core.
    pub(crate) fn refresh_keyboard_focus(&mut self) {
        if self
            .layer_focus
            .as_ref()
            .is_some_and(|surface| !surface.alive())
        {
            self.layer_focus = None;
        }
        let layer = self.exclusive_layer().or_else(|| self.layer_focus.clone());
        let target = layer
            .map(Focus::Wayland)
            .or_else(|| self.window_focus.clone());
        self.set_keyboard_focus(target);
    }

    /// A click on `surface`: a layer surface that takes the keyboard when clicked gets
    /// it. A click anywhere else gives it back.
    pub(crate) fn click_on(&mut self, surface: Option<&WlSurface>) {
        let clicked = surface.and_then(|surface| self.layer_of(surface));
        self.layer_focus = clicked
            .filter(LayerSurface::can_receive_keyboard_focus)
            .map(|layer| layer.wl_surface().clone());
        self.refresh_keyboard_focus();
    }

    /// The pointer entered a window: the keyboard goes back to following the core.
    pub(crate) fn release_layer_focus(&mut self) {
        if self.layer_focus.take().is_some() {
            self.refresh_keyboard_focus();
        }
    }
}

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        output: Option<WlOutputResource>,
        _layer: WlrLayer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(WlOutput::from_resource)
            .or_else(|| self.output_ids.first().map(|(_, output)| output.clone()));
        let Some(output) = output else {
            // With no screen there is nowhere to put it; the client may ask again later.
            surface.send_close();
            return;
        };
        let layer = LayerSurface::new(surface, namespace);
        // A surface that was just made cannot be mapped yet, so this cannot fail.
        let _ = layer_map_for_output(&output).map_layer(&layer);
    }

    fn new_popup(&mut self, _parent: WlrLayerSurface, popup: PopupSurface) {
        // Placed where its client asks, without being kept on the output.
        let geometry = popup.with_pending_state(|state| state.positioner.get_geometry());
        popup.with_pending_state(|state| state.geometry = geometry);
        // A popup whose parent is already gone cannot be shown; its client is told when
        // the surface dies, so there is nothing to do with the error.
        let _ = self.popups.track_popup(PopupKind::Xdg(popup));
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let Some(output) = self.layer_output(surface.wl_surface()) else {
            return;
        };
        {
            let mut map = layer_map_for_output(&output);
            let layer = map
                .layer_for_surface(surface.wl_surface(), WindowSurfaceType::TOPLEVEL)
                .cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
            }
        }
        if self.layer_focus.as_ref() == Some(surface.wl_surface()) {
            self.layer_focus = None;
        }
        self.layers_changed();
    }
}

delegate_layer_shell!(State);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn zone(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn a_free_zone_below_a_bar_reserves_the_bars_height_on_top() {
        let insets = insets_of(zone(0, 30, 1000, 570), (1000, 600).into());
        assert_eq!(insets, Insets::new(0, 30, 0, 0));
    }

    #[test]
    fn bars_on_every_side_reserve_each_side() {
        let insets = insets_of(zone(10, 30, 940, 530), (1000, 600).into());
        assert_eq!(insets, Insets::new(10, 30, 50, 40));
    }

    #[test]
    fn an_untouched_output_reserves_nothing() {
        assert_eq!(
            insets_of(zone(0, 0, 1000, 600), (1000, 600).into()),
            Insets::NONE
        );
    }

    proptest! {
        #[test]
        fn the_reserve_and_the_zone_always_add_up_to_the_output(
            left in 0_i32..300, top in 0_i32..300, right in 0_i32..300, bottom in 0_i32..300,
        ) {
            let (w, h) = (1000, 800);
            let free = zone(left, top, w - left - right, h - top - bottom);
            let insets = insets_of(free, (w, h).into());
            prop_assert_eq!(
                (insets.left, insets.top, insets.right, insets.bottom),
                (left as u32, top as u32, right as u32, bottom as u32)
            );
        }

        #[test]
        fn no_zone_can_make_the_reserve_negative_or_wrap(
            x in -5000_i32..5000, y in -5000_i32..5000, w in 0_i32..5000, h in 0_i32..5000,
        ) {
            let insets = insets_of(zone(x, y, w, h), (1000, 800).into());
            prop_assert!(insets.left <= 5000 && insets.right <= 10_000);
            prop_assert!(insets.top <= 5000 && insets.bottom <= 10_000);
        }
    }
}
