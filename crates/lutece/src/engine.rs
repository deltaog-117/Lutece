//! The engine: turns the events a backend reports into window-manager state, and that
//! state into the [`Scene`](crate::Scene) the backend should show.
//!
//! The scene is never stored. [`Engine::scene`] derives it from the current state, so it
//! cannot drift from what the engine knows.

mod handle;
mod scene_builder;

use crate::input::Keymap;
use crate::layout::Layout;
use crate::rules::Rules;
use crate::shared::contracts::event::KeyPress;
use crate::shared::contracts::event::Output;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::OutputId;
use crate::stack::stackset::StackSet;

/// The window manager's state and the rules that change it.
///
/// Feed it [`Event`](crate::Event)s with [`handle`](Engine::handle), then ask for the
/// [`scene`](Engine::scene) and compare it with what is on screen using
/// [`diff`](crate::diff).
///
/// # Examples
///
/// ```
/// use lutece::{Engine, Event, Layout, Output, OutputId, Rect, StackSet, Tag, WinId, WindowInfo};
///
/// let stacks = StackSet::new([Tag::try_new("1")?])?;
/// let mut engine = Engine::new(stacks, Layout::default());
///
/// let screen = Output { id: OutputId::new(0), rect: Rect::new(0, 0, 1000, 600)? };
/// engine.handle(Event::OutputsChanged(vec![screen]));
/// engine.handle(Event::WindowOpened { id: WinId::new(1), info: WindowInfo::default() });
///
/// assert_eq!(engine.scene()?.focus(), Some(WinId::new(1)));
/// # Ok::<(), lutece::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    stacks: StackSet,
    keymap: Keymap,
    rules: Rules,
    /// The outputs the backend reported, and the area each one covers; empty until the
    /// backend reports any.
    outputs: Vec<Output>,
}

impl Engine {
    /// Builds an engine with no outputs yet. Every workspace starts with `layout` and can
    /// change it on its own afterwards.
    #[must_use]
    pub fn new(stacks: StackSet, layout: Layout) -> Self {
        Self {
            stacks: stacks.with_layout(layout),
            keymap: Keymap::new(),
            rules: Rules::new(),
            outputs: Vec::new(),
        }
    }

    /// Uses `keymap` for key presses. Without one, key presses do nothing.
    #[must_use]
    pub fn with_keymap(mut self, keymap: Keymap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Uses `rules` to decide how newly opened windows are managed. Without any, every
    /// window opens on the current workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownTag`] if a rule sends windows to
    /// a workspace this engine does not have.
    pub fn with_rules(mut self, rules: Rules) -> Result<Self> {
        if let Some(tag) = rules
            .tags()
            .find(|tag| !self.stacks.tags().any(|t| t == *tag))
        {
            return Err(Error::UnknownTag(tag.clone()));
        }
        self.rules = rules;
        Ok(self)
    }

    /// The key combinations the backend must grab so their presses reach
    /// [`handle`](Engine::handle).
    pub fn grabs(&self) -> impl Iterator<Item = KeyPress> + '_ {
        self.keymap.presses()
    }

    /// The workspaces and the windows on them.
    #[must_use]
    pub fn stacks(&self) -> &StackSet {
        &self.stacks
    }

    /// The area `output` covers, if the backend reported it.
    fn area_of(&self, output: OutputId) -> Option<Rect> {
        self.outputs
            .iter()
            .find(|reported| reported.id == output)
            .map(|reported| reported.rect)
    }
}
