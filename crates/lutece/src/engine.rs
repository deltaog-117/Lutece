//! The engine: turns the events a backend reports into window-manager state, and that
//! state into the [`Scene`](crate::Scene) the backend should show.
//!
//! The scene is never stored. [`Engine::scene`] derives it from the current state, so it
//! cannot drift from what the engine knows.

mod external;
mod handle;
mod scene_builder;

use crate::input::Keymap;
use crate::layout::Layout;
use crate::rules::Rules;
use crate::shared::contracts::event::KeyPress;
use crate::shared::contracts::event::Output;
use crate::shared::contracts::event::PropertyTarget;
use crate::shared::contracts::style::Style;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::{Insets, Rect};
use crate::shared::ids::{OutputId, WinId};
use crate::shared::names::{PropertyKey, PropertyValue};
use crate::stack::stackset::StackSet;
use std::collections::HashMap;

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
    /// What bars reserve on each output, as the backend last reported it. An output with
    /// nothing reserved has no entry. Kept when an output disappears, so a bar that
    /// outlives a monitor change is still honoured when its output returns.
    reserved: HashMap<OutputId, Insets>,
    /// The style each managed window was given; a window with none has the default.
    /// Kept only for managed windows, and only while it differs from the default.
    styles: HashMap<WinId, Style>,
    /// The properties plugins set, by what they belong to. A target with none has no entry,
    /// so two engines that went through different sets and removals still compare equal.
    properties: HashMap<PropertyTarget, std::collections::BTreeMap<PropertyKey, PropertyValue>>,
    /// The external layout of each workspace that has used it.
    external: external::Slots,
    /// Whether a new window is offered to the manage hook.
    manage_hook: bool,
    /// The manage requests nobody has answered, by serial.
    manage_pending:
        std::collections::BTreeMap<u64, (WinId, crate::shared::contracts::event::WindowInfo)>,
    /// The serial of the next layout request. Part of the state, so a replay numbers its
    /// requests the same way.
    next_serial: u64,
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
            reserved: HashMap::new(),
            styles: HashMap::new(),
            properties: HashMap::new(),
            manage_hook: false,
            manage_pending: std::collections::BTreeMap::new(),
            external: external::Slots::new(),
            next_serial: 0,
        }
    }

    /// Offers every new window to a plugin: the engine asks with
    /// [`Effect::ManageRequested`](crate::Effect::ManageRequested) and applies the answer
    /// [`Event::ManageDecided`](crate::Event::ManageDecided). The window is placed by the
    /// rules at once; the engine does not wait.
    #[must_use]
    pub fn with_manage_hook(mut self, on: bool) -> Self {
        self.manage_hook = on;
        self
    }

    /// Uses `keymap` for key presses. Without one, key presses do nothing.
    #[must_use]
    pub fn with_keymap(mut self, keymap: Keymap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Changes the keymap and the manage hook of a running engine, so a configuration
    /// can be reloaded without losing the windows. Only these two can change: the
    /// workspaces, the layouts and the rules are state that windows already live in, so
    /// changing them is left to whoever builds a new engine.
    ///
    /// Requests the manage hook has not been answered for stay open and can still be
    /// answered after the hook is turned off. The caller must ask the backend to grab
    /// [`grabs`](Engine::grabs) again, since the keys may differ.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Engine, Keymap, Layout, StackSet, Tag};
    ///
    /// let mut engine = Engine::new(StackSet::new([Tag::try_new("1")?])?, Layout::default());
    /// assert_eq!(engine.grabs().count(), 0);
    /// engine.reconfigure(Keymap::new(), true);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn reconfigure(&mut self, keymap: Keymap, manage_hook: bool) {
        self.keymap = keymap;
        self.manage_hook = manage_hook;
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

    /// The properties of `target`, by key.
    pub fn properties_of(
        &self,
        target: &PropertyTarget,
    ) -> impl Iterator<Item = (&PropertyKey, &PropertyValue)> + '_ {
        self.properties.get(target).into_iter().flatten()
    }

    /// Every property there is, with what it belongs to, in no fixed order between targets.
    pub fn properties(
        &self,
    ) -> impl Iterator<Item = (&PropertyTarget, &PropertyKey, &PropertyValue)> + '_ {
        self.properties.iter().flat_map(|(target, entries)| {
            entries.iter().map(move |(key, value)| (target, key, value))
        })
    }

    /// The area `output` covers, if the backend reported it. A fullscreen window fills it.
    fn area_of(&self, output: OutputId) -> Option<Rect> {
        self.outputs
            .iter()
            .find(|reported| reported.id == output)
            .map(|reported| reported.rect)
    }

    /// The area of `output` left for tiled and floating windows once bars took theirs.
    fn work_area_of(&self, output: OutputId) -> Option<Rect> {
        let area = self.area_of(output)?;
        let reserved = self.reserved.get(&output).copied().unwrap_or_default();
        Some(area.inset(reserved.limited_to(area)))
    }
}
