//! Applying one event to the engine's state.

use super::Engine;
use crate::input::Action;
use crate::rules::Outcome;
use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::event::{Event, WindowInfo};
use crate::shared::ids::WinId;

/// How far one key press moves the master column, in thousandths of the width.
const MASTER_STEP: i16 = 50;

impl Engine {
    /// Applies `event` and returns the effects the backend must carry out.
    ///
    /// Events that no longer apply are ignored, not reported: a backend sees close and
    /// focus events for windows it has already lost, and a duplicate open is harmless.
    /// A window that opens is placed by the engine's rules: on the current workspace by
    /// default, on another one if a rule sends it there, or released to its client with
    /// [`Effect::Release`] if a rule ignores it.
    /// A key press runs the action bound to it in the engine's keymap, and a press with
    /// no binding does nothing. Only some actions produce effects: quitting, closing the
    /// focused window and spawning a program.
    ///
    /// The first output is the screen windows are laid out on. An empty set of outputs
    /// leaves nothing to show.
    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::WindowOpened { id, info } => self.open(id, &info, &mut effects),
            Event::WindowClosed(id) => {
                // Already gone: nothing left to remove.
                let _ = self.stacks.remove(id);
            }
            Event::FocusRequested(id) => {
                // Not managed (never opened, or just closed): nothing to focus.
                let _ = self.stacks.focus_window(id);
            }
            Event::KeyPressed(press) => {
                if let Some(action) = self.keymap.action(press).cloned() {
                    self.run(action, &mut effects);
                }
            }
            Event::OutputsChanged(outputs) => {
                self.screen = outputs.first().map(|output| output.rect);
            }
        }
        effects
    }

    fn open(&mut self, id: WinId, info: &WindowInfo, effects: &mut Vec<Effect>) {
        // A window that is already managed stays where it is, whatever the rules say.
        match self.rules.outcome_for(info) {
            None => {
                let _ = self.stacks.insert(id);
            }
            // `with_rules` checked that every tag exists.
            Some(Outcome::SendTo(tag)) => {
                let _ = self.stacks.insert_on(id, tag);
            }
            Some(Outcome::Ignore) => {
                if !self.stacks.contains(id) {
                    effects.push(Effect::Release(id));
                }
            }
        }
    }

    fn run(&mut self, action: Action, effects: &mut Vec<Effect>) {
        match action {
            Action::Quit => effects.push(Effect::Quit),
            Action::Close => effects.extend(self.stacks.focused().map(Effect::Close)),
            Action::Spawn(command) => effects.push(Effect::Spawn(command)),
            Action::FocusNext => self.stacks.focus_next(),
            Action::FocusPrev => self.stacks.focus_prev(),
            Action::SwapNext => self.stacks.swap_next(),
            Action::SwapPrev => self.stacks.swap_prev(),
            Action::NextLayout => self.stacks.next_layout(),
            Action::GrowMaster => self.stacks.resize_master(MASTER_STEP),
            Action::ShrinkMaster => self.stacks.resize_master(-MASTER_STEP),
            // A tag nobody has (a binding written for another set of workspaces) is
            // ignored, like any other request that no longer applies.
            Action::ViewTag(tag) => {
                let _ = self.stacks.focus_tag(&tag);
            }
            Action::MoveToTag(tag) => {
                let _ = self.stacks.move_focused_to(&tag);
            }
        }
    }
}
