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
- ✅ Nested dev environment – `scripts/dev-x11` builds the example, starts Xephyr on a spare display and runs the WM in it, so testing needs no logout

---

## 🔥 High Priority (Critical)

- None at the moment.

---

## 🟡 Medium Priority (Important)

- **Adopt existing windows** – manage windows that are already mapped when the X11 backend starts.
- **Manage rules** – float, workspace assignment and ignore rules in `rules`.
- **Multi-output support** – several screens in the scene, with focus moving between them.
- **Scripted X11 integration tests** – drive the nested X server with `xdotool` and assert window geometry.
- **Headless CI check** – run the core tests and X11 integration tests with `Xvfb`.

---

## 🟢 Low Priority (Nice‑to‑Have)

- **Layout and master-ratio keys** – actions to switch layout and resize the master column.
- **Keyboard remapping** – refresh the X11 keycode table on `MappingNotify` and re-grab.
- **More layouts** – grid, centered master, and similar, as separate small modules.
- **Floating windows** – move and resize in the scene model.
- **Documentation site** – guide on writing a WM with the library.

---

## 🔭 Long‑Term Vision

- **Wayland backend** – `lutece-wayland` on Smithay, tested nested inside Hyprland through its winit backend, sharing the same core and scene diff.
- **XWayland windows** – treat X11 clients under the Wayland backend as one more window source.
- **Plugin protocol** – serializable `Event` and `Effect` over a socket, so the plugin-capable WM can host plugins in any language (lives in that WM, not this library).

---

## 🎯 Next Actions (Immediate)

1. **Record a benchmark baseline** – `cargo bench -p lutece`, so the 5% regression gate has something to compare against.
2. **Adopt existing windows** – so the WM can take over a running session.
