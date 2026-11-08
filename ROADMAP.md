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
- ✅ Window style and animation in the scene – `Style` (corner radius, border, opacity, and a `Transition` of curve and duration), carried as a checked list on `Scene` like fullscreen, set by `Event::StyleChanged`, and announced by `Change::Style` ahead of the move it animates; backends do not draw it yet
- ✅ Action requests in the core – `Event::ActionRequested(Action)` runs an action with no key bound to it, as a bound key would; it lets a window manager's tools and plugins drive the engine through the same event log
- ✅ External layout in the core – `Layout::External` asks a plugin for a workspace's rectangles with `Effect::LayoutRequested` and uses `Event::LayoutProvided`; until an answer fits it shows the default layout, so the engine never waits and a replay ends in the same state; `Engine::pending_layout_requests` lets a host give a plugin that connects late the questions it missed
- ✅ Nested window drawn the right way up – seen for the first time with a bar and terminals in it: the frame was upside down and is now drawn with `Transform::Flipped180`
- ✅ Wayland host scale – `Scale` (a factor kept to a 120th) and `WaylandBackend::set_scale`; the core stays in logical pixels, clients that bind `wp_fractional_scale` are told the exact scale and size their buffers with `wp_viewporter`, `wl_output` says the scale rounded up with the physical mode, and the nested window follows the host's scale and its changes
- ✅ Wayland shadows – a window's surface (subsurfaces included) is cut to the rectangle the core gave it, so a client that draws a shadow or frame of its own cannot paint over its neighbours; popups are not cut
- ✅ Wayland output enter – a window's surface tree and its popups are told with `wl_surface.enter` and `leave` which outputs its rectangle overlaps, and with `preferred_buffer_scale` the host scale rounded up, so clients that read `wl_output.scale` instead of `wp_fractional_scale` draw sharp too; kept current when a window is placed, hidden or moved, an output is added, resized or unplugged, and the scale changes; the compositor global is now version 6
- ✅ Wayland buffer sharing – `linux-dmabuf` (version 4, with default feedback): `WaylandBackend::enable_dmabuf` takes a `DmabufSupport` (the render node's device number and the formats it can import); `NestedBackend` finds both through EGL, so GPU-rendered clients hand over their frames without a copy; a format that was not offered is refused by Smithay, and a buffer the driver still rejects is skipped when drawn
- ✅ Popups and window moves – a popup follows its window by itself; when a move leaves it somewhere its positioner rules would not put it (a menu slid back from an edge, or one that now leaves the output) it is dismissed, since the protocol lets a compositor place a popup only when it opens or its client asks
- ✅ X11 integration tests run – the suite ran for the first time on a real `Xvfb` and all of it passes, `monitors` included (so RandR changes on `Xvfb` do reach the backend). Two faults in the test harness itself came out: the Wayland example was also named `minimal`, so building it overwrote the X11 example the tests start (it is now `minimal_wayland`), and the key-swap helper copied one key's symbols over the other's, so the remap test could not find `l`
- ✅ X11 drag tests – three tests with XTEST pointer events: Super+left-drag moves a floating window, Super+right-drag resizes it, and a drag without the modifier changes nothing
- ✅ XWayland windows – `WaylandBackend::enable_xwayland` runs Xwayland and the window manager side Smithay needs; an X11 window is reported as `WindowOpened` (title and class from its properties, a dialog's parent as `transient_for`), tiled, floated, focused (the X input focus follows) and closed like any other, and a spawned program gets `DISPLAY`. X11 menus and tooltips (override-redirect) are not managed: they are drawn above everything where their program put them and do not take focus. Tested with a real Xwayland and an X11 client of its own; copy and paste between X11 and Wayland programs is done in the next item
- ✅ Clipboard between X11 and Wayland programs – Xwayland's selections are bridged to the Wayland ones for the clipboard and the primary selection: only the format list crosses when a selection is set, the data moves on paste straight from the program that copied it; an X11 program reads a Wayland selection only while an X11 window has the keyboard focus; a selection a Wayland client holds is not cleared by an X11 program giving up its own
- ✅ Fullscreen windows in the core – `Event::FullscreenRequested` and `Action::ToggleFullscreen`; a fullscreen window covers its whole output, bars included, and is drawn above everything on it, while the other windows keep their places below. Only the focused window can be fullscreen: moving the focus, closing the window or opening another ends it, so the window with the keyboard is never hidden. The scene carries the state (`Scene::is_fullscreen`) and `diff` reports `Change::Fullscreen` ahead of the window's move, so a backend sizes it once
- ✅ Reserved space in the core – `Event::ReservedChanged` gives the complete reserve of an output as `Insets`; tiled and floating windows are laid out in what is left (`Rect::inset`). A claim on more than half of an axis is ignored on that axis, so no client can leave the windows without room (a property test found the 1-pixel case)
- ✅ X11 hints and struts – windows of type dock (bars) are left where they are and not managed; their `_NET_WM_STRUT_PARTIAL` or `_NET_WM_STRUT` is worked out per monitor (a bar over one monitor reserves space on that one), follows changes and removal, and bars already showing at startup count. `_NET_WM_STATE` fullscreen requests (add, remove, toggle) reach the core, and the backend keeps the property in step with the scene, also when a window is hidden. `_NET_SUPPORTED` and the check window tell toolkits what is honoured. The example binds Super+Shift+f
- ✅ Layer-shell on Wayland – `wlr-layer-shell` (version 4) through Smithay's layer map: bars, launchers and wallpapers are placed by the compositor, never tiled, and the space they keep clear reaches the core as `ReservedChanged`. Stacking is overlay, fullscreen windows, top, windows, bottom, background, so a fullscreen window hides the bars. They take the pointer; one with exclusive interactivity holds the keyboard, one that asks for it on demand gets it when clicked until the pointer enters a window; a bar's popups are placed where the client asks; a surface on an output that goes away is asked to close. Fullscreen requests from toplevels (also before the first commit) and from X11 programs through Xwayland reach the core, and the state rides on the next configure
- ✅ Input module and hardware groundwork – keyboard and pointer handling moved out of `NestedBackend` into `input`, which takes events from any Smithay input backend (winit now, libinput later) and keeps a relative pointer inside the output; Smithay gains the DRM, GBM, udev, libinput, libseat and multi-GPU features, not yet used by a session backend
- ✅ Xwayland display numbers – `enable_xwayland` takes each instance's display number from a process-wide counter, so two instances of one process never share a number (the first still gets the lowest free one). The Xwayland tests, which failed in about 1 run in 8 with `Connection refused`, ran 100 times with no failure. A new process starting while the previous one's Xwayland is still dying is not covered
- ✅ X11 programs that start fullscreen under Xwayland – the backend opens a second connection to Xwayland (`x11rb`, now a normal dependency of `lutece-wayland`) and reads `_NET_WM_STATE` when a window asks to map; a window that has set fullscreen gets `FullscreenRequested` right after `WindowOpened`
- ✅ Override-redirect bars on X11 – a window that maps without asking the window manager is kept as a bar if it has a strut (`_NET_WM_STRUT_PARTIAL` or `_NET_WM_STRUT`), also when it sets the strut after it maps or was showing at startup; menus and tooltips have none and are dropped. A bar that publishes no strut (plain `dzen`) still reserves nothing
- ✅ X11 stacking and active-window hints under Xwayland – the stacking order of the X11 windows (`_NET_CLIENT_LIST_STACKING`) follows the scene, with a fullscreen window above the others, and `_NET_ACTIVE_WINDOW` names the X11 window that has the keyboard focus and none when a Wayland window has it. Smithay hears focus changes only on the root window, so what it wrote there was the root or the window that had just lost the focus; the backend now writes it itself and puts it back if Smithay overwrites it. Tested with a real Xwayland; not seen with a real pager or taskbar

---

## 🔥 High Priority (Critical)

- None at the moment.

---

## 🟡 Medium Priority (Important)

Needed before `lucarne` can be written on top of the library:

- **First tagged release** – a `0.x` version of the three crates with the changelog moved out of `[Unreleased]`, so `lucarne` depends on a version and not on a path.

---

## 🟢 Low Priority (Nice‑to‑Have)

- **Documentation site** – guide on writing a WM with the library.
- **Start Xwayland on demand** – today it starts with `enable_xwayland`; starting it when the first X11 program connects saves its memory when none is used. Smithay's `XWayland::spawn` binds the sockets and starts Xwayland in one call, and its socket and lock code and client data are private, so it needs our own socket handling and spawn (copying that code), or a change in Smithay. Starting at the first spawn effect was rejected: it saves almost nothing. Left for when the memory matters.

---

## 🔭 Long‑Term Vision

- **Blur and shadows** – GLES shader effects beyond radius, border and opacity (background blur, drop shadows), drawn in `lutece-wayland` and described by new `Style` fields; after the style drawing in Next Actions item 3.
- **Wayland on real hardware** – a session backend for `lutece-wayland` (DRM, libinput, seat management) so it can run as the compositor of a login, not only nested.
- **Session lock and idle protocols** – what a login compositor needs on top of the hardware backend; only after that backend exists.
- **`lucarne`, the suckless-style WM** – its own repository and its own dev loop, built on a tagged release of lutece; starts on X11, the more complete backend, unless the Wayland work above is done first. Building it shows which gaps in the library matter.
- **Plugin protocol** – serializable `Event` and `Effect` over a socket, so the plugin-capable WM can host plugins in any language (lives in that WM, not this library).

---

## 🎯 Next Actions (Immediate)

1. **Look at the nested Wayland WM** – run `cargo run -p lutece-wayland --example minimal_wayland` (and with `LUTECE_OUTPUTS=2`) and confirm by eye that windows are drawn, tiled and follow the keys, that the pointer focuses windows, that Alt+drag moves a floating window (Alt+f first), that a menu opens, closes and stays on its output, that Alacritty draws no title bar of its own, that a client with a shadow (a GTK window) does not paint over its neighbour, that Alt+, and Alt+. move between the two outputs, that copy and paste work between two terminals, and that on a host at a scale other than 1 (try 1.5) windows have the size you expect and text in Alacritty or a GTK 4 app is sharp, and that a client without fractional scale (a GTK 3 app) is sharp too; that a GPU client (`weston-simple-dmabuf-egl`, or Firefox) draws and is not blank, which also shows whether this machine's EGL device gives a render node (`/dev/dri/renderD128` exists here; with software rendering the nested compositor offers no dmabuf); that copy and paste cross between an X11 program (`xterm`) and a Wayland one, in both directions, for the clipboard and for selecting text with the mouse; and that an X11 program (`DISPLAY=:N` is printed at startup; try `xterm` or `xeyes`) is tiled with the others, takes focus and typing, that its menus open in place and over its window, and that it is not blurry at a scale of 1.5; none of this has been seen on screen yet (only tested with a scripted client and a scripted X11 client). The one run so far only showed the startup line.
2. **Look at bars and fullscreen** – on X11, `scripts/dev-x11` then a bar (`tint2` or `polybar`) and `xterm`: tiles start below the bar, Super+Shift+f covers the bar and again puts the window back, a video player's fullscreen button does the same, and the bar's space comes back when it quits. On the nested Wayland WM, `waybar` (windows leave room for it, a fullscreen window hides it; Alt+Shift+f fullscreens the focused window), a launcher such as `fuzzel` or `wofi` (it takes the keyboard and gives it back), and a wallpaper (`swaybg`). None of this has been seen on screen; it is tested with scripted clients only. Also try `polybar` with `override-redirect = true` (tiles start below it, and the space comes back when it quits) and an X11 video player started fullscreen under the nested Wayland WM (for example `mpv --fs`).
3. **Draw style and run transitions in the backends** – `lutece-wayland` stores `Change::Style` per window and draws radius, border and opacity with GLES shader elements in `render.rs`, then interpolates a move over the transition's duration; X11 keeps ignoring it.
4. **Record a benchmark baseline** – `cargo bench -p lutece`, so the 5% regression gate has something to compare against.
5. **Documentation site** – guide on writing a WM with the library.
