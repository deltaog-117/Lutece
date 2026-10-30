# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Cargo workspace with three crates: `lutece` (core), `lutece-x11` and `lutece-wayland` (backend placeholders)
- Core contracts in `lutece::shared::contracts`: `Event`, `Effect`, `Scene` and `Change`
- `diff`, which compares two scenes and returns the minimal changes a backend must apply
- Validated building blocks: non-zero `Size`, `Rect`, `WinId`, `OutputId`, `Modifiers`, `SpawnCommand`
- Optional `serde` feature that serializes the contract types
- Property tests for geometry, ids, modifiers and `diff`, and a Criterion benchmark for `diff`
- `scripts/check` (format, clippy, tests)
- ROADMAP.md, DIARY.md and this changelog
- `StackSet`: workspaces with a current one, each holding its windows as a focus-tracking zipper
- Window operations on `StackSet`: insert, remove, focus next/previous, swap next/previous, focus a window on any workspace
- Workspace operations on `StackSet`: switch by tag or by next/previous with wrap-around, send the focused window to another workspace
- `Tag`, a workspace name that cannot be empty, and the errors `EmptyTag`, `DuplicateTag`, `NoWorkspaces`, `UnknownTag`, `UnknownWindow`
- Model-based property test that runs random operation sequences and checks every window is on exactly one workspace and focus is always valid
- `Layout` enum with `Tile` and `Monocle`, and `Layout::arrange`, which turns a window list and an output rectangle into placements
- `MasterRatio`, the master column's share of the width in whole thousandths (1 to 999), so both columns always keep a pixel
- Tile splits the right column evenly and gives leftover pixels to the first rows, so the rectangles cover the area exactly; monocle puts the focused window on top
- Errors `InvalidRatio`, `AreaTooSmall` and `AreaOutOfRange`
- Property tests for exact coverage, no overlap, window order, and no panic near the coordinate limit
- `Engine`: `handle` applies a backend `Event` to the workspaces and screen and returns the `Effect`s to carry out; `scene` derives the `Scene` to show from the current state
- Events that no longer apply (a close or focus request for an unknown window, a duplicate open) are ignored; key presses do nothing until key bindings exist
- Event-replay integration tests and a property test over random event streams, checking every open window is on exactly one workspace and the scene matches the current workspace
- `lutece-x11`: `X11Backend` connects to the display, becomes its window manager and reports the screen as `OutputsChanged`
- X events translated to `Event`: map requests become `WindowOpened` (with title, class and transient parent), client unmaps and destroys become `WindowClosed`, and the pointer entering a window becomes `FocusRequested`
- `X11Backend::apply_scene` shows a `Scene` by applying only the `diff` from what is shown (place, map, unmap, restack, focus); `apply_effects` closes windows politely, spawns programs and reports `Flow::Quit`
- The caller owns the loop: `next_events` blocks, `pending_events` does not, and `AsFd` lets other inputs share a `poll`
- Error type for the backend, including `AnotherWindowManager` when the display already has one
- Property tests for event translation and for scene application against a fake server
- Dependencies for `lutece-x11`: `x11rb`, `thiserror` (and `proptest` for tests)
- `minimal` example (`cargo run -p lutece-x11 --example minimal`): a window manager in one file that tiles every window on a single workspace; stop it with Ctrl-C or by closing the nested X server
- `scripts/dev-x11 [display]`: builds the `minimal` example, starts Xephyr on a spare display (default `:2`, size from `LUTECE_SCREEN`), waits for it, runs the window manager in it and stops Xephyr on exit
- Key bindings: `Keymap` (key combination to `Action`, one action per combination) and the `Action` enum (quit, close, spawn, focus and swap next/previous, view a tag, move to a tag)
- `Engine::with_keymap` and `Engine::grabs`; a `KeyPressed` event now runs the bound action and returns `Quit`, `Close` and `Spawn` as effects
- `Keysym::RETURN` and `Keysym::from_char`, and the error `DuplicateBinding`
- `X11Backend::grab_keys` grabs key combinations on the root window, also with CapsLock and NumLock held, and key presses reach the core as `KeyPressed`; errors `UnmappedKeysym` and `KeyAlreadyGrabbed`
- Property tests for the keymap, the keycode lookup and the modifier conversion, and replay tests for every key action
- `X11Backend::connect` adopts windows already on screen: each showing, non-override-redirect window is reported as `WindowOpened` after the screen, bottom of the stack first
- Tests that only showing client windows are adopted, once, and that an adopted window is hidden with a real unmap

- Layout keys: the `Action` variants `NextLayout`, `GrowMaster` and `ShrinkMaster`; growing and shrinking move the master column by 5% and are ignored by layouts without one
- `Workspace::layout`, `StackSet::next_layout` and `StackSet::resize_master`, and `MasterRatio::step`, which moves a ratio and stops at the 1 to 999 limits
- Tests for `MasterRatio::step`, for tile getting its ratio back after a trip through monocle, and for replaying the layout keys through the engine
- The `minimal` example binds Super+space (next layout) and Super+h / Super+l (narrow / widen the master column)
- Manage rules in `lutece::rules`: `Rules` is an ordered list of `ManageRule`s, each a `Match` (class, title or dialog) and an `Outcome` (`Ignore` or `SendTo(Tag)`); the first match decides
- `Engine::with_rules`, which refuses rules that send windows to a workspace the engine lacks; `StackSet::insert_on` adds a window to a given workspace without switching to it
- `Effect::Release`: the core stops managing a window and leaves it to its client; the X11 backend maps it, forgets it and answers its configure requests as it asks
- Tests for rule matching and order, for send and ignore rules replayed through the engine, and for a released window never being reported again
- The `minimal` example ignores `Xmessage` windows and opens `Firefox` on workspace 9

### Changed
- Each workspace now holds its own layout instead of the engine holding one for all; `Engine::new` still takes a layout, which every workspace starts with
- The `minimal` example has nine workspaces and key bindings (Super+Return, j/k, 1-9, Shift variants, c, q); it no longer needs to be stopped from outside
