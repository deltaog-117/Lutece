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
