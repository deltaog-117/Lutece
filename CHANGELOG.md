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
