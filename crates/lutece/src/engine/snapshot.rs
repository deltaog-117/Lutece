//! The engine as plain data and back.

use super::Engine;
use super::external::{self, Slot, Slots};
use crate::shared::contracts::event::{Output, PropertyTarget, WindowInfo};
use crate::shared::contracts::style::{Curve, Rgba, Style, Transition};
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Insets;
use crate::shared::ids::{OutputId, Tag, WinId};
use crate::shared::names::{MAX_PROPERTIES_PER_TARGET, PropertyKey, PropertyValue};
use crate::snapshot::{
    ColorState, CurveState, OutputState, PendingState, PropertiesState, PropertyEntry,
    ReservedState, Snapshot, StyleEntry, StyleState, TargetState, VERSION,
};
use std::collections::{BTreeMap, HashMap, HashSet};

fn refused(why: impl std::fmt::Display) -> Error {
    Error::InvalidSnapshot(why.to_string())
}

const fn color_state(color: Rgba) -> ColorState {
    ColorState {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

const fn color_from(color: ColorState) -> Rgba {
    Rgba::new(color.r, color.g, color.b, color.a)
}

fn style_state(style: &Style) -> StyleState {
    let border = style.border();
    let (bar, shadow) = (style.title_bar(), style.shadow());
    StyleState {
        radius: style.radius(),
        border_width: border.width,
        border_color: color_state(border.color),
        opacity: style.opacity(),
        // Matched without a catch-all: a new curve must be given a name here.
        curve: match style.transition().curve {
            Curve::Linear => CurveState::Linear,
            Curve::EaseIn => CurveState::EaseIn,
            Curve::EaseOut => CurveState::EaseOut,
            Curve::EaseInOut => CurveState::EaseInOut,
        },
        duration_ms: style.transition().duration_ms,
        title_bar_height: bar.height,
        title_bar_color: color_state(bar.color),
        shadow_blur: shadow.blur,
        shadow_dx: shadow.dx,
        shadow_dy: shadow.dy,
        shadow_color: color_state(shadow.color),
    }
}

fn style_from(state: &StyleState) -> Style {
    let curve = match state.curve {
        CurveState::Linear => Curve::Linear,
        CurveState::EaseIn => Curve::EaseIn,
        CurveState::EaseOut => Curve::EaseOut,
        CurveState::EaseInOut => Curve::EaseInOut,
    };
    Style::default()
        .with_radius(state.radius)
        .with_border(state.border_width, color_from(state.border_color))
        .with_opacity(state.opacity)
        .with_title_bar(state.title_bar_height, color_from(state.title_bar_color))
        .with_shadow(
            state.shadow_blur,
            state.shadow_dx,
            state.shadow_dy,
            color_from(state.shadow_color),
        )
        .with_transition(Transition {
            curve,
            duration_ms: state.duration_ms,
        })
}

impl Engine {
    /// What this engine knows about the session, as plain data: the windows and workspaces
    /// and where they are, the outputs, the styles and properties, and the layout and manage
    /// requests that wait for a plugin. The configuration is not part of it (see
    /// [`snapshot`](crate::snapshot)).
    ///
    /// Restoring the snapshot onto an engine with the same configuration gives an engine
    /// equal to this one.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let (screens, current, hidden) = self.stacks.to_state();

        let mut reserved: Vec<ReservedState> = self
            .reserved
            .iter()
            .map(|(output, insets)| ReservedState {
                output: output.get(),
                left: insets.left,
                top: insets.top,
                right: insets.right,
                bottom: insets.bottom,
            })
            .collect();
        reserved.sort_by_key(|entry| entry.output);

        let mut styles: Vec<StyleEntry> = self
            .styles
            .iter()
            .map(|(id, style)| StyleEntry {
                window: id.get(),
                style: style_state(style),
            })
            .collect();
        styles.sort_by_key(|entry| entry.window);

        let mut properties: Vec<PropertiesState> = self
            .properties
            .iter()
            .map(|(target, entries)| PropertiesState {
                target: match target {
                    PropertyTarget::Window(id) => TargetState::Window(id.get()),
                    PropertyTarget::Workspace(tag) => {
                        TargetState::Workspace(tag.as_str().to_owned())
                    }
                },
                entries: entries
                    .iter()
                    .map(|(key, value)| PropertyEntry {
                        key: key.as_str().to_owned(),
                        value: value.as_str().to_owned(),
                    })
                    .collect(),
            })
            .collect();
        properties.sort_by(|a, b| format!("{:?}", a.target).cmp(&format!("{:?}", b.target)));

        let mut external: Vec<_> = self
            .external
            .iter()
            .map(|(tag, slot)| slot.to_state(tag))
            .collect();
        external.sort_by(|a, b| a.tag.cmp(&b.tag));

        Snapshot {
            version: VERSION,
            screens,
            current,
            hidden,
            outputs: self
                .outputs
                .iter()
                .map(|output| OutputState {
                    id: output.id.get(),
                    rect: external::rect_state(output.rect),
                })
                .collect(),
            reserved,
            styles,
            properties,
            external,
            manage_pending: self
                .manage_pending
                .iter()
                .map(|(serial, (window, info))| PendingState {
                    serial: *serial,
                    window: window.get(),
                    title: info.title.clone(),
                    class: info.class.clone(),
                    transient_for: info.transient_for.map(WinId::get),
                })
                .collect(),
            next_serial: self.next_serial,
        }
    }

    /// Builds an engine from `snapshot`, with the keys, rules, manage hook and workspace
    /// names of `template`, which must be the configuration the snapshot was taken under.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSnapshot`] if the snapshot has another version, does not fit
    /// the template's workspaces, or breaks a rule the engine keeps: a window on two
    /// workspaces, a style or property for a window that is not managed, a list entry that
    /// stands for the default (which the engine never stores), or a value that its checked
    /// constructor refuses.
    pub fn restore(template: &Self, snapshot: &Snapshot) -> Result<Self> {
        if snapshot.version != VERSION {
            return Err(refused(format!(
                "version {} cannot be read by version {VERSION}",
                snapshot.version
            )));
        }
        let stacks = crate::stack::stackset::StackSet::from_state(
            &template.stacks,
            &snapshot.screens,
            snapshot.current,
            &snapshot.hidden,
        )?;
        let tags: HashSet<&Tag> = template.stacks.tags().collect();

        // Not checked for repeats: the engine keeps the outputs as the backend reported them,
        // and only the screens built from them are told apart.
        let outputs = snapshot
            .outputs
            .iter()
            .map(|output| {
                Ok(Output {
                    id: OutputId::new(output.id),
                    rect: external::rect_from(output.rect)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut reserved = HashMap::new();
        for entry in &snapshot.reserved {
            let insets = Insets::new(entry.left, entry.top, entry.right, entry.bottom);
            if insets.is_none() {
                return Err(refused(format!(
                    "output {} reserves nothing, which the engine does not store",
                    entry.output
                )));
            }
            if reserved
                .insert(OutputId::new(entry.output), insets)
                .is_some()
            {
                return Err(refused(format!("output {} reserves twice", entry.output)));
            }
        }

        let mut styles = HashMap::new();
        for entry in &snapshot.styles {
            let id = WinId::new(entry.window);
            if !stacks.contains(id) {
                return Err(refused(format!(
                    "a style for window {}, which is not managed",
                    entry.window
                )));
            }
            let style = style_from(&entry.style);
            if style == Style::default() {
                return Err(refused(format!(
                    "window {} has the default style, which the engine does not store",
                    entry.window
                )));
            }
            if styles.insert(id, style).is_some() {
                return Err(refused(format!("window {} is styled twice", entry.window)));
            }
        }

        let mut properties = HashMap::new();
        for held in &snapshot.properties {
            let target = match &held.target {
                TargetState::Window(raw) => {
                    let id = WinId::new(*raw);
                    if !stacks.contains(id) {
                        return Err(refused(format!(
                            "properties of window {raw}, which is not managed"
                        )));
                    }
                    PropertyTarget::Window(id)
                }
                TargetState::Workspace(name) => {
                    let tag = Tag::try_new(name.as_str()).map_err(refused)?;
                    if !tags.contains(&tag) {
                        return Err(refused(format!(
                            "properties of {name}, which is not a workspace"
                        )));
                    }
                    PropertyTarget::Workspace(tag)
                }
            };
            if held.entries.is_empty() || held.entries.len() > MAX_PROPERTIES_PER_TARGET {
                return Err(refused(format!(
                    "{target:?} has {} properties, which is none or too many",
                    held.entries.len()
                )));
            }
            let mut entries = BTreeMap::new();
            for entry in &held.entries {
                let key = PropertyKey::try_new(entry.key.as_str()).map_err(refused)?;
                let value = PropertyValue::try_new(entry.value.as_str()).map_err(refused)?;
                if entries.insert(key, value).is_some() {
                    return Err(refused(format!(
                        "{target:?} has the key {:?} twice",
                        entry.key
                    )));
                }
            }
            if properties.insert(target, entries).is_some() {
                return Err(refused("a target is listed twice in the properties"));
            }
        }

        let mut external = Slots::new();
        for slot in &snapshot.external {
            let (tag, slot) = Slot::from_state(slot)?;
            if !tags.contains(&tag) {
                return Err(refused(format!(
                    "an external layout for {tag}, which is not a workspace"
                )));
            }
            if external.insert(tag.clone(), slot).is_some() {
                return Err(refused(format!("{tag} has two external layout states")));
            }
        }

        let mut manage_pending = BTreeMap::new();
        for pending in &snapshot.manage_pending {
            let info = WindowInfo {
                title: pending.title.clone(),
                class: pending.class.clone(),
                transient_for: pending.transient_for.map(WinId::new),
            };
            if manage_pending
                .insert(pending.serial, (WinId::new(pending.window), info))
                .is_some()
            {
                return Err(refused(format!(
                    "manage request {} is listed twice",
                    pending.serial
                )));
            }
        }

        Ok(Self {
            stacks,
            keymap: template.keymap.clone(),
            rules: template.rules.clone(),
            outputs,
            reserved,
            styles,
            properties,
            external,
            manage_hook: template.manage_hook,
            manage_pending,
            next_serial: snapshot.next_serial,
        })
    }
}
