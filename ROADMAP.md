# Roadmap

This document outlines the future direction of lutece, a minimal Rust library for building window managers that run on both X11 and Wayland.
It is the shared foundation for two separate follow-up projects: lucarne, a suckless-style window manager with dwm-like configuration, and a plugin-capable window manager (unnamed) with plugins in any language.
Items are organized by priority, not by timeline.
Author: deltaog-117

---

## ✅ Completed (Milestones Achieved)

- ✅ Design decision: pure core with events in and a desired scene out; backends apply the scene
- ✅ Feature-first Cargo workspace scaffold (`lutece`, `lutece-x11`, `lutece-wayland`)
- ✅ Core contracts – `Event`, `Effect`, `Scene`, `diff`, geometry, ids and errors, with property tests and a `diff` benchmark
- ✅ `scripts/check` – format, clippy and tests
- ✅ Workspace and focus state – `StackSet` with zipper workspaces, focus, swap, tag switching and moving windows between workspaces, checked by model-based property tests
- ✅ Layouts – closed `Layout` enum with tile (master ratio in whole thousandths) and monocle, checked by property tests for exact coverage
- ✅ X11 backend – `lutece-x11` translates X events into `Event` and applies `Scene` and `Effect` as X requests, with the caller owning the loop; existing windows and several outputs come later
- ✅ Engine and display-free tests – `Engine::handle` applies events, `Engine::scene` derives the scene, checked by event-replay tests and a property test over random event streams
- ✅ Minimal example WM – `crates/lutece-x11/examples/minimal.rs` wires `Engine` to `X11Backend` in a short `main.rs`
- ✅ Key bindings – `Keymap` and a closed `Action` enum in `input`; the engine owns the keymap, lists its keys with `Engine::grabs` and runs the action on `KeyPressed`; the X11 backend grabs them (also with CapsLock and NumLock held); the example quits, spawns, moves focus and switches workspaces
- ✅ Adopt existing windows – `X11Backend::connect` queries the root's children after taking over and reports each showing, non-override-redirect window as `WindowOpened`, so the WM can replace a running one without a restart
- ✅ Layout and master-ratio keys – each workspace keeps its own `Layout`; `NextLayout`, `GrowMaster` and `ShrinkMaster` actions cycle it and move the master column in 5% steps; the example binds Super+space, h and l
- ✅ Manage rules – `Rules` of `ManageRule`s match a new window by class, title or dialog status; the first match either ignores the window (`Effect::Release` hands it back to its client) or opens it on another workspace; `Engine::with_rules` refuses an unknown workspace; float waits for floating windows
- ✅ Nested dev environment – `scripts/dev-x11` builds the example, starts Xephyr on a spare display and runs the WM in it, so testing needs no logout

---

## 🔥 High Priority (Critical)

- None at the moment.

---

## 🟡 Medium Priority (Important)

- **Multi-output support** – several screens in the scene, with focus moving between them.
- **Scripted X11 integration tests** – drive the nested X server with `xdotool` and assert window geometry.
- **Headless CI check** – run the core tests and X11 integration tests with `Xvfb`.

---

## 🟢 Low Priority (Nice‑to‑Have)

- **Keyboard remapping** – refresh the X11 keycode table on `MappingNotify` and re-grab.
- **More layouts** – grid, centered master, and similar, as separate small modules.
- **Floating windows** – move and resize in the scene model, with a `Float` outcome for manage rules.
- **Documentation site** – guide on writing a WM with the library.

---

## 🔭 Long‑Term Vision

- **Wayland backend** – `lutece-wayland` on Smithay, tested nested inside Hyprland through its winit backend, sharing the same core and scene diff.
- **XWayland windows** – treat X11 clients under the Wayland backend as one more window source.
- **Plugin protocol** – serializable `Event` and `Effect` over a socket, so the plugin-capable WM can host plugins in any language (lives in that WM, not this library).

---

## 🎯 Next Actions (Immediate)

1. **Record a benchmark baseline** – `cargo bench -p lutece`, so the 5% regression gate has something to compare against.
2. **Floating windows** – scene model and a `Float` manage-rule outcome.
