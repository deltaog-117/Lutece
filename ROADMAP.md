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
- ✅ Manage rules – `Rules` of `ManageRule`s match a new window by class, title or dialog status; the first match either ignores the window (`Effect::Release` hands it back to its client) or opens it on another workspace; `Engine::with_rules` refuses an unknown workspace
- ✅ Floating windows – a floating window keeps a position and size relative to a default rectangle centered in the screen, is left out of the layout and drawn above the tiles; `Outcome::Float` opens a window floating, and `ToggleFloat`, `MoveFloat` and `ResizeFloat` actions change it
- ✅ Keyboard remapping – the X11 backend refetches the keycode table on `MappingNotify` and grabs its keys again, skipping keys the new layout lacks
- ✅ Scripted X11 integration tests – the example WM runs on a private `Xvfb` and the tests press keys through XTEST and assert window geometry
- ✅ Headless CI check – `scripts/ci` requires `Xvfb` and runs the check; a GitHub Actions workflow calls it
- ✅ More layouts – `Grid` and `CenteredMaster` as separate modules; `NextLayout` cycles tile, monocle, grid, centered master, and the master ratio carries across tile and centered master
- ✅ Nested dev environment – `scripts/dev-x11` builds the example, starts Xephyr on a spare display and runs the WM in it, so testing needs no logout
- ✅ Multi-output support – `StackSet` keeps one shared pool of workspaces and each output's screen shows one of them; `Engine::scene` lays out every screen in its own area; `FocusNextOutput`, `FocusPrevOutput`, `SendToNextOutput` and `SendToPrevOutput` actions; unplugging a monitor hides its workspace instead of losing its windows; the X11 backend lists monitors through RandR 1.5 and follows changes
- ✅ Wayland backend, protocol side – `WaylandBackend` is a Smithay compositor the caller drives: it accepts clients, reports each xdg toplevel on its first commit, sizes and focuses windows from the scene, takes grabbed key combinations before clients see them, and spawns programs with `WAYLAND_DISPLAY` pointing at itself; tested without a screen by a real Wayland client stepped in turn with the backend
- ✅ Wayland backend, nested window – `NestedBackend` shows the compositor in a window of the running session (Smithay's winit backend), draws the placed windows and answers frame callbacks; the `minimal` example in `lutece-wayland` is a small keyboard-driven WM on it
- ✅ Wayland pointer input – focus follows the pointer (`Event::FocusRequested` on entering a window), clients get motion, buttons and wheel, and the cursor a client asks for is shown; with the drag modifier held (Alt by default) the left button moves and the right button resizes the window under the pointer
- ✅ Floating drags in the core – `Event::FloatAdjusted` moves and resizes a floating window by pixel amounts, sharing the keyboard actions' float logic; tiled and unknown windows ignore it
- ✅ Mouse drags on X11 – `X11Backend::set_drag_modifier` grabs the left and right buttons on the root with the modifier held, so the click never reaches the window; motion becomes `Event::FloatAdjusted`, as on Wayland; the `minimal` example uses Super
- ✅ Wayland window decorations – `xdg-decoration` always answers server side (the compositor draws no frames, so clients drop their title bars), and a window is placed by the geometry its client declares, so a client that keeps a shadow lines up with its place
- ✅ Wayland popup placement – a popup's own slide, flip and resize rules are applied against the output of its parent window, when it opens and when it is repositioned, also for a menu opened from a menu
- ✅ Wayland outputs – `WaylandBackend::set_outputs` takes the complete set of outputs (added, moved and removed ones become `wl_output` globals or lose them); `NestedBackend::open_with_outputs` cuts the window into several outputs side by side, so the multi-output actions can be tried on Wayland (`LUTECE_OUTPUTS=2` in the example)
- ✅ Wayland popups and clipboard – popups (menus, tooltips) are placed where their client asks, drawn above their window, reachable by the pointer and dismissed by a click elsewhere when they grabbed it; copy and paste work between clients through the clipboard and the primary selection
- ✅ Wayland host scale – `Scale` (a factor kept to a 120th) and `WaylandBackend::set_scale`; the core stays in logical pixels, clients that bind `wp_fractional_scale` are told the exact scale and size their buffers with `wp_viewporter`, `wl_output` says the scale rounded up with the physical mode, and the nested window follows the host's scale and its changes
- ✅ Wayland shadows – a window's surface (subsurfaces included) is cut to the rectangle the core gave it, so a client that draws a shadow or frame of its own cannot paint over its neighbours; popups are not cut
- ✅ Wayland output enter – a window's surface tree and its popups are told with `wl_surface.enter` and `leave` which outputs its rectangle overlaps, and with `preferred_buffer_scale` the host scale rounded up, so clients that read `wl_output.scale` instead of `wp_fractional_scale` draw sharp too; kept current when a window is placed, hidden or moved, an output is added, resized or unplugged, and the scale changes; the compositor global is now version 6
- ✅ Wayland buffer sharing – `linux-dmabuf` (version 4, with default feedback): `WaylandBackend::enable_dmabuf` takes a `DmabufSupport` (the render node's device number and the formats it can import); `NestedBackend` finds both through EGL, so GPU-rendered clients hand over their frames without a copy; a format that was not offered is refused by Smithay, and a buffer the driver still rejects is skipped when drawn
- ✅ Popups and window moves – a popup follows its window by itself; when a move leaves it somewhere its positioner rules would not put it (a menu slid back from an edge, or one that now leaves the output) it is dismissed, since the protocol lets a compositor place a popup only when it opens or its client asks

---

## 🔥 High Priority (Critical)

- None at the moment.

---

## 🟡 Medium Priority (Important)

- None at the moment.

---

## 🟢 Low Priority (Nice‑to‑Have)

- **Documentation site** – guide on writing a WM with the library.

---

## 🔭 Long‑Term Vision

- **Wayland on real hardware** – a session backend for `lutece-wayland` (DRM, libinput, seat management) so it can run as the compositor of a login, not only nested.
- **XWayland windows** – treat X11 clients under the Wayland backend as one more window source.
- **Plugin protocol** – serializable `Event` and `Effect` over a socket, so the plugin-capable WM can host plugins in any language (lives in that WM, not this library).

---

## 🎯 Next Actions (Immediate)

1. **Look at the nested Wayland WM** – run `cargo run -p lutece-wayland --example minimal` (and with `LUTECE_OUTPUTS=2`) and confirm by eye that windows are drawn, tiled and follow the keys, that the pointer focuses windows, that Alt+drag moves a floating window (Alt+f first), that a menu opens, closes and stays on its output, that Alacritty draws no title bar of its own, that a client with a shadow (a GTK window) does not paint over its neighbour, that Alt+, and Alt+. move between the two outputs, that copy and paste work between two terminals, and that on a host at a scale other than 1 (try 1.5) windows have the size you expect and text in Alacritty or a GTK 4 app is sharp, and that a client without fractional scale (a GTK 3 app) is sharp too; and that a GPU client (`weston-simple-dmabuf-egl`, or Firefox) draws and is not blank, which also shows whether this machine's EGL device gives a render node (`/dev/dri/renderD128` exists here; with software rendering the nested compositor offers no dmabuf); none of this has been seen on screen yet (only tested with a scripted client).
2. **Record a benchmark baseline** – `cargo bench -p lutece`, so the 5% regression gate has something to compare against.
3. **Run the X11 integration tests once on a machine with `Xvfb`** – none was available while writing them, so they have only been compiled. This includes the `monitors` tests, which also settle whether Xvfb reports a monitor change as an event. Add a test there that drags a floating window with XTEST button presses; the X11 drag code is covered only by unit tests so far.
4. **Documentation site** – guide on writing a WM with the library.
