//! Applying one event to the engine's state.

use super::Engine;
use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::event::Event;

impl Engine {
    /// Applies `event` and returns the effects the backend must carry out.
    ///
    /// Events that no longer apply are ignored, not reported: a backend sees close and
    /// focus events for windows it has already lost, and a duplicate open is harmless.
    /// Key presses do nothing until key bindings exist, so no event produces an effect
    /// yet; the return type is here so adding bindings does not change this signature.
    ///
    /// The first output is the screen windows are laid out on. An empty set of outputs
    /// leaves nothing to show.
    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::WindowOpened { id, .. } => {
                // A window that is already managed stays where it is.
                let _ = self.stacks.insert(id);
            }
            Event::WindowClosed(id) => {
                // Already gone: nothing left to remove.
                let _ = self.stacks.remove(id);
            }
            Event::FocusRequested(id) => {
                // Not managed (never opened, or just closed): nothing to focus.
                let _ = self.stacks.focus_window(id);
            }
            Event::KeyPressed(_) => {}
            Event::OutputsChanged(outputs) => {
                self.screen = outputs.first().map(|output| output.rect);
            }
        }
        Vec::new()
    }
}
