//! Applying one event to the engine's state.

use super::Engine;
use crate::input::Action;
use crate::rules::Outcome;
use crate::shared::contracts::effect::Effect;
use crate::shared::contracts::event::{Event, WindowInfo};
use crate::shared::contracts::style::Style;
use crate::shared::ids::WinId;

/// How far one key press moves the master column, in thousandths of the width.
const MASTER_STEP: i16 = 50;

/// How far one key press moves or resizes a floating window, in pixels.
const FLOAT_STEP: i32 = 40;

impl Engine {
    /// Applies `event` and returns the effects the backend must carry out.
    ///
    /// Events that no longer apply are ignored, not reported: a backend sees close and
    /// focus events for windows it has already lost, and a duplicate open is harmless.
    /// A window that opens is placed by the engine's rules: on the current workspace by
    /// default, on another one if a rule sends it there, floating if a rule floats it, or
    /// released to its client with [`Effect::Release`] if a rule ignores it.
    /// A fullscreen request makes the window fill its whole screen, and the bars' reserve
    /// reported with `ReservedChanged` shrinks the area the other windows share.
    /// A style change gives a managed window its look and animation.
    /// A drag of a floating window moves and resizes it, as the move and resize keys do.
    /// An action request runs that action, as a bound key would.
    /// A key press runs the action bound to it in the engine's keymap, and a press with
    /// no binding does nothing. Only some actions produce effects: quitting, closing the
    /// focused window and spawning a program.
    ///
    /// Each reported output is a screen showing one workspace; see
    /// [`StackSet::set_outputs`](crate::StackSet::set_outputs) for which. A report that
    /// lists no outputs leaves nothing to show until the next one.
    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::WindowOpened { id, info } => self.open(id, &info, &mut effects),
            Event::WindowClosed(id) => {
                // Already gone: nothing left to remove.
                let _ = self.stacks.remove(id);
                self.styles.remove(&id);
            }
            Event::FocusRequested(id) => {
                // Not managed (never opened, or just closed): nothing to focus.
                let _ = self.stacks.focus_window(id);
            }
            Event::FloatAdjusted { id, dx, dy, dw, dh } => {
                // A floating size is relative to the screen, so with no screen yet only
                // the movement applies.
                let area = self.work_area_of(self.stacks.current_output());
                self.stacks.update_float(id, |float| {
                    let moved = float.moved(dx, dy);
                    area.map_or(moved, |area| moved.resized(area, dw, dh))
                });
            }
            Event::KeyPressed(press) => {
                if let Some(action) = self.keymap.action(press).cloned() {
                    self.run(action, &mut effects);
                }
            }
            Event::ActionRequested(action) => self.run(action, &mut effects),
            Event::FullscreenRequested { id, on } => self.stacks.set_fullscreen(id, on),
            Event::ReservedChanged { output, insets } => {
                if insets.is_none() {
                    self.reserved.remove(&output);
                } else {
                    self.reserved.insert(output, insets);
                }
            }
            Event::StyleChanged { id, style } => {
                if !self.stacks.contains(id) {
                    // Not managed: a style for it could never be shown, and would linger.
                } else if style == Style::default() {
                    self.styles.remove(&id);
                } else {
                    self.styles.insert(id, style);
                }
            }
            Event::OutputsChanged(outputs) => {
                let ids: Vec<_> = outputs.iter().map(|output| output.id).collect();
                self.stacks.set_outputs(&ids);
                self.outputs = outputs;
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
            Some(Outcome::Float) => {
                let _ = self.stacks.insert_floating(id);
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
            Action::FocusNextOutput => self.stacks.focus_next_output(),
            Action::FocusPrevOutput => self.stacks.focus_prev_output(),
            Action::SendToNextOutput => self.stacks.send_focused_to_next_output(),
            Action::SendToPrevOutput => self.stacks.send_focused_to_prev_output(),
            Action::ToggleFloat => self.stacks.toggle_float(),
            Action::ToggleFullscreen => self.stacks.toggle_fullscreen(),
            Action::MoveFloat(direction) => {
                let (dx, dy) = direction.delta(FLOAT_STEP);
                self.stacks
                    .update_focused_float(|float| float.moved(dx, dy));
            }
            Action::ResizeFloat(direction) => {
                // A floating size is relative to the screen, so with no screen yet there
                // is nothing to resize against.
                if let Some(area) = self.work_area_of(self.stacks.current_output()) {
                    let (dw, dh) = direction.delta(FLOAT_STEP);
                    self.stacks
                        .update_focused_float(|float| float.resized(area, dw, dh));
                }
            }
        }
    }
}
