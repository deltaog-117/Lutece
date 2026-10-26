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

---

## 🔥 High Priority (Critical)

- **Display-free tests** – event-replay tests that feed a recorded event stream through `engine` and assert the resulting `Scene`.
- **X11 backend** – `lutece-x11` translates X events into `Event` and applies `Scene` and `Effect` as X requests.
- **Minimal example WM** – `examples/minimal` shows a WM built from the library in a small `main.rs`.
- **Nested dev environment** – script to run the example WM inside Xephyr under a Hyprland session, so testing needs no logout.

---

## 🟡 Medium Priority (Important)

- **Key bindings** – bindings and keymap in `input`, grabbed by the backend.
- **Manage rules** – float, workspace assignment and ignore rules in `rules`.
- **Multi-output support** – several screens in the scene, with focus moving between them.
- **Scripted X11 integration tests** – drive the nested X server with `xdotool` and assert window geometry.
- **Headless CI check** – run the core tests and X11 integration tests with `Xvfb`.

---

## 🟢 Low Priority (Nice‑to‑Have)

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

1. **Wire the engine** – `crates/lutece/src/engine/{handle,scene_builder}.rs` and the event-replay tests.
2. **Record a benchmark baseline** – `cargo bench -p lutece`, so the 5% regression gate has something to compare against.
3. **Add the nested dev script** – `scripts/dev-x11` to launch Xephyr and the example WM (the example moves into `crates/lutece-x11/examples/`).
4. **Start the X11 backend** – `crates/lutece-x11/src/{connection,translate,apply,grabs}.rs`.
