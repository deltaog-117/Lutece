# 📘 DIARY.md

## Architectural Decision Log

*A chronological record of architectural decisions, trade-offs and reasoning for lutece. Git tracks what changed; this diary tracks why.*

---

## 📋 Decision Index

| Date | Decision Area | Choice | Status |
|------|---------------|--------|--------|
| 2026-10-26 | Language | Rust | ✅ Confirmed |
| 2026-10-26 | Core and backend boundary | Pure core: events in, scene out | ✅ Confirmed |
| 2026-10-26 | Shape of the contracts | Closed set shared by both backends | ✅ Confirmed |
| 2026-10-26 | Project split and names | lutece (library), lucarne (suckless WM), plugin WM (unnamed) | ✅ Confirmed |
| 2026-10-26 | Dependencies | thiserror, optional serde (Serialize only), proptest and criterion (dev) | ✅ Confirmed |
| 2026-10-26 | Workspace and focus state | Zipper per workspace, zipper over workspaces | ✅ Confirmed |
| 2026-10-26 | Layout representation | Closed enum with one `arrange` entry point | ✅ Confirmed |
| 2026-10-27 | Engine interface | Stateful engine, scene derived on request | ✅ Confirmed |
| 2026-10-27 | X11 backend loop | Caller drives the loop; backend translates and applies | ✅ Confirmed |
| 2026-10-27 | Minimal example exit | Plain loop, stopped from outside | ✅ Confirmed |
| 2026-10-27 | Nested dev environment | POSIX shell script around Xephyr, fixed spare display | ✅ Confirmed |
| 2026-10-27 | Key bindings | Engine-owned keymap with a closed `Action` enum | ✅ Confirmed |
| 2026-10-30 | Adopting existing windows | Scan the root's children at connect, report them as `WindowOpened` | ✅ Confirmed |
| 2026-10-30 | Manage rules | Ignore and workspace rules as data; float waits for floating windows | ✅ Confirmed |
| 2026-10-31 | Floating windows | Offsets from a centered default rectangle, kept per workspace | ✅ Confirmed |
| 2026-11-02 | Keyboard remapping | Remember the grabs; refetch the table and regrab on `MappingNotify` | ✅ Confirmed |
| 2026-11-02 | X11 integration tests | Example WM on a private Xvfb, keys pressed through XTEST | ✅ Confirmed |
| 2026-11-02 | Headless CI | `scripts/ci` requires Xvfb; a GitHub Actions workflow calls it | ✅ Confirmed |
| 2026-11-02 | More layouts | Grid and centered master as new enum variants; one shared master ratio | ✅ Confirmed |
| 2026-11-02 | Multi-output | One shared pool of workspaces; each output's screen shows one | ✅ Confirmed |
| 2026-11-02 | Wayland backend | Smithay compositor the caller drives; spike, then headless protocol tests, then a nested window | ✅ Confirmed |
| 2026-11-02 | Pointer drags | One core event, `FloatAdjusted`, for pointer drags; focus stays in the backend | ✅ Confirmed |
| 2026-11-02 | Wayland outputs | A set of outputs on the backend; the nested window is cut into virtual monitors | ✅ Confirmed |
| 2026-11-02 | Wayland host scale | Fractional scale in 120ths: logical core, `wp_fractional_scale` and `wp_viewporter` for clients | ✅ Confirmed |
| 2026-11-02 | Wayland output enter | Worked out from window and output rectangles, not from the render pass | ✅ Confirmed |
| 2026-11-02 | Wayland dmabuf | `linux-dmabuf` v4 with the render GPU named, fed by the host that owns the renderer | ✅ Confirmed |
| 2026-11-02 | XWayland | Smithay's window manager on a loop kept inside the backend; windows and menus, no clipboard bridge yet | ✅ Confirmed |
| 2026-11-02 | X11 and Wayland clipboard | Formats cross when set, data on paste; X11 reads only while an X11 window has focus | ✅ Confirmed |
| 2026-11-03 | Reserved space | A separate `ReservedChanged` event with `Insets`; the output keeps its true size; a claim on over half an axis is ignored | ✅ Confirmed |
| 2026-11-03 | Fullscreen | State in the scene (a set), reported by `diff`; only the focused window can be fullscreen | ✅ Confirmed |
| 2026-11-03 | X11 bars | Dock windows tracked, not managed; struts worked out per monitor; EWMH advertised | ✅ Confirmed |
| 2026-11-03 | Wayland layer-shell | Smithay's layer map; one front-to-back list of layers; keyboard for exclusive and clicked surfaces | ✅ Confirmed |
| 2026-11-04 | Xwayland display numbers | The library never reuses a display number within a process | ✅ Confirmed |
| 2026-11-04 | Fullscreen at start under Xwayland | A second connection to Xwayland reads `_NET_WM_STATE` at the map request | ✅ Confirmed |
| 2026-11-04 | Override-redirect bars on X11 | Found at `MapNotify`; a bar is a window with a strut | ✅ Confirmed |
| 2026-11-06 | X11 stacking and active window under Xwayland | Backend restacks from the scene and writes `_NET_ACTIVE_WINDOW` itself; on-demand start deferred | ✅ Confirmed |

---

## 📝 Decision Entries

### Language: Rust

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

The library has to support X11 and Wayland. The X11 side is a client that manages other clients' windows; the Wayland side is the compositor itself, with rendering, input and protocols.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| Rust + Smithay | Used by Niri, Cosmic and Pinnacle; building blocks with no forced abstraction; memory safety; same language as the planned WMs | Smithay is less mature than wlroots and its API moves | M |
| C + wlroots | Smallest and most prior art (sway, dwl) | No memory safety; no maintained Rust binding | M |
| Zig + wlroots | Good C interop (river) | Pre-1.0, small ecosystem | M |
| Go or Python | Fine for X11 and plugins | Poor fit for a compositor's render and input loop | H |

#### Decision & Rationale

**Chosen Option:** Rust.

The X11 side has solid pure-Rust crates, Smithay covers Wayland, and both WMs planned on top of this library are Rust. Trade-off accepted: Smithay's API churn.

#### Implementation Notes

- Evidence came from Penrose's README, its Wayland tracking issue and the Xfce team's published reasoning for choosing Smithay. The Smithay-versus-wlroots comparisons are one project's rationale, not a neutral benchmark.

#### References

- https://github.com/sminez/penrose and its issue #288 (Wayland as a backend)
- https://github.com/Smithay/smithay

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Core and backend boundary: pure core, events in, scene out

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

Penrose's `Conn` trait is platform-agnostic in its types but imperative: the backend exposes `position_client`, `show_client`, `focus_client`, owns a blocking `next_event`, and handles events against `&mut State`. Smithay's `calloop` wants to own the event loop, so a blocking `next_event` does not fit Wayland. The question was where to draw the line between the shared core and the backends.

#### Options Considered

**Option A: Imperative backend trait (Penrose style)**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Simplest, familiar |
| **Disadvantages** | Core owns the loop; needs a thread or channel workaround on Wayland |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Poor for Wayland |

**Option B: Pure core; events in, desired scene out**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Testable with no display; no loop ownership; same config on both backends; data is serializable |
| **Disadvantages** | Needs an effect enum and scene diffing |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Best |

**Option C: Core generic over backend types**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Maximum flexibility per backend |
| **Disadvantages** | Config is not portable, which defeats the goal; worse compile times and errors |
| **Implementation Difficulty** | Hard |
| **Fit with Constraints** | Fails the main goal |

#### Decision & Rationale

**Chosen Option:** B.

It ranks first on correctness (the core can be replayed from recorded events with no display) and leaves the Wayland loop with Smithay. A is cheaper but becomes a workaround on Wayland, which is the problem the library exists to solve.

#### Implementation Notes

- Backends depend only on `lutece::shared::contracts`; they never import a feature.
- Window placement and focus travel in the `Scene`; other actions are `Effect`s.

#### References

- Penrose `src/core/conn.rs` (develop branch), read through a web fetch; the full source was not checked line by line.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Shape of the contracts: closed set shared by both backends

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

`Event` and `Effect` cross the line between core and backend. The question was whether a backend may send messages only it understands.

#### Options Considered

**Option A: Closed set, meaningful on both backends**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Exhaustive tests; identical replay on both backends |
| **Disadvantages** | A backend-only feature needs a library change |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Matches "one config on both backends" |

**Option B: Closed set plus an extension slot**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Backend-only features without a library change |
| **Disadvantages** | Weakens type safety and replay; the core knows about backends or loses types |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Weaker |

#### Decision & Rationale

**Chosen Option:** A.

Enums are `#[non_exhaustive]`, so adding a variant later is not a breaking change. B can be added on top of A later, but not the reverse. It would be worth reopening if lucarne or the plugin WM turn out to need many backend-only features.

#### Implementation Notes

- A scene lists only visible windows, bottom to top; a managed window that is not listed is hidden.
- `diff` emits hides, shows and moves, at most one restack, at most one focus change, in that order. A restack is sent whenever a window is shown, because its place in the stack is known only from the full order.
- `Size` is non-zero by construction, `SpawnCommand` has a non-empty program, `Scene` has unique windows and a listed focus, `Modifiers` only holds defined bits.
- `Keysym` is a bare `u32` because X11 and xkbcommon share the numbering.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Project split and names

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

Three projects will share one foundation. Names were checked against crates.io only; GitHub and trademark searches are not done.

#### Decision & Rationale

- **lutece:** the library (crates `lutece`, `lutece-x11`, `lutece-wayland`). Lutèce was the Roman name for Paris; the accent is dropped so the name types on any keyboard.
- **lucarne:** a suckless, dwm-inspired window manager for X11 and Wayland with a compile-time configuration. Lucarne is French for a dormer window.
- **Plugin WM:** a separate, unnamed project that will host plugins in any language. It is started after lutece is finished.

Names were chosen for being short and easy to type, in line with the minimalist goal. `voussoir` was the runner-up for the library.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Dependencies

**Date:** 2026-10-26
**Status:** Confirmed

#### Decision & Rationale

| Crate | Where | Why |
|---|---|---|
| `thiserror` | runtime | Library error type, per the Rust etiquette file |
| `serde` (optional feature) | runtime, off by default | Lets `Event` and `Effect` be serialized for the plugin WM later; Serialize only, because deserializing must go through the validating constructors |
| `proptest` | dev | Property tests, required by the manifesto |
| `criterion` | dev | Benchmark of `diff`, the first likely hot path |
| `x11rb` | runtime, `lutece-x11` only | Pure-Rust X11 client (no C library, no `unsafe` needed), added 2026-10-27 with the X11 backend |
| `thiserror`, `proptest` | `lutece-x11` | Same roles as in the core |

`criterion` was not in the list shown for approval; it is dev-only and easy to drop.

#### Implementation Notes

- `scripts/check` runs `cargo fmt --check`, clippy with `-D warnings`, and the tests, with `--all-features` so the serde feature is checked too.
- The bench file allows `missing_docs` locally because `criterion_group!` generates an undocumented function. The workspace-wide lint stays on.
- No baseline benchmark has been recorded yet; run `cargo bench -p lutece` to create one.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Workspace and focus state: zipper per workspace

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

The core has to know which windows are on which workspace and which one has focus. A window must be on exactly one workspace, and focus must never point at a window that is not there. Penrose's `Stack` (up, focus, down over two deques) and `StackSet` were read for reference.

#### Options Considered

**Option A: Zipper per workspace**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Focus is valid by the shape of the data; proven design (xmonad, Penrose) |
| **Disadvantages** | Harder to read and write; the one-workspace rule still needs tests |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Best match for "invalid states unrepresentable" |

**Option B: Ordered list plus focus id**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Simplest to read and extend |
| **Disadvantages** | Focus can disagree with the list; only tests guard it |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Weaker on correctness |

#### Decision & Rationale

**Chosen Option:** A.

A focus bug in a window manager shows up as the wrong window being closed, so the guarantee is worth the extra complexity, which stays inside one module.

#### Implementation Notes

- `Stack` is a private zipper (`up`, `focus`, `down`); `Workspace` holds an `Option<Stack>`, so an empty workspace has no focus by construction.
- `StackSet` is itself a zipper over workspaces (`before`, `current`, `after`), so a current workspace always exists. Switching rebuilds the split instead of using an index that could go out of range.
- A new window takes focus and the old focus sits right after it. Removing the focused window passes focus to the next one, or to the previous one at the end. Focus and swap operations wrap around.
- Sending the focused window to another workspace keeps the current workspace on screen, and the window becomes the focus of its new workspace.
- Inserting a known window and removing or focusing an unknown one return errors instead of being ignored.
- The one-workspace rule is kept by the methods, not the types. A model-based property test runs random operation sequences against a plain set of windows and checks it after every step.
- No benchmark was added: `StackSet` operations run on user input with tens of windows, not on a measured hot path.
- Multi-output support is left out of this cut; the private fields allow adding it later without breaking users.

#### References

- Penrose `src/pure/stack.rs` and `src/pure/stack_set.rs` (develop branch), read through a web fetch.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Layout representation: closed enum

**Date:** 2026-10-26
**Status:** Confirmed

#### Context / Background

A layout turns the windows of a workspace and an output rectangle into one rectangle per window. How a layout is represented is part of the public interface, which lucarne and the plugin WM will build on.

#### Options Considered

**Option A: Free functions**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Simplest, nothing new to maintain |
| **Disadvantages** | A workspace cannot remember or switch its layout without a selector, which is option B built later |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Fine, but incomplete |

**Option B: Closed enum with one `arrange` entry point**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Comparable and copyable value, exhaustive matching, easy switching and testing |
| **Disadvantages** | Adding a layout means editing the enum |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Matches the pure core and the model-based tests |

**Option C: Trait with implementations**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Other crates can add layouts without forking |
| **Disadvantages** | `Box<dyn Layout>` has no free `Clone`/`Eq`/`Serialize`, loses exhaustive matching, adds allocation |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Open extension is not needed: lucarne is edited and recompiled, the plugin WM speaks a protocol |

#### Decision & Rationale

**Chosen Option:** B.

Keeping a layout a plain value fits the pure core. If open extension is ever needed, a `Custom(fn(..))` variant can be added without losing `Clone` and `Eq`.

#### Implementation Notes

- `MasterRatio` is whole thousandths in `1..=999` instead of a float, so layouts stay `Eq` and `Hash` and the pixel maths is exact. The master width is clamped so both columns keep at least one pixel.
- Tile divides the column height evenly and gives the leftover pixels one each to the first rows, so the rectangles cover the area exactly.
- `Size` cannot be zero, so an area with fewer pixels than the layout needs returns `AreaTooSmall` instead of producing zero-size windows. An area reaching past `i32::MAX` returns `AreaOutOfRange` instead of wrapping.
- Monocle puts the focused window last (on top) and cannot fail, because every window gets the area unchanged.
- `Tile` holds a `MasterRatio` directly; a one-field params struct would add nothing yet. No gaps, borders or master count in this cut.
- No benchmark was added: layouts run once per event on a handful of windows, not on a measured hot path.

#### References

- None beyond the existing contracts.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-26 | Initial entry | deltaog-117 |

---

### Engine interface: stateful engine, scene derived on request

**Date:** 2026-10-27
**Status:** Confirmed

#### Context / Background

The engine takes a backend's events, updates the window-manager state and produces the scene and effects. Its call shape is the interface every backend, lucarne and the plugin WM will depend on, so it is hard to change later.

#### Options Considered

**Option A: Stateful engine, scene on request**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | The scene is always derived from the state, so it cannot drift; matches the `&mut self` style of `StackSet`; smallest interface |
| **Disadvantages** | Each backend keeps its own last scene and calls `diff` |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Tests assert directly on a `Scene`, as the roadmap asks |

**Option B: Engine that returns ready-to-apply changes**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Backends are minimal |
| **Disadvantages** | The engine's memory of the last scene can disagree with the real screen after a failed apply or a reconnect; needs a full-scene call anyway; the diff becomes part of the contract |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Stores a second copy of the truth |

**Option C: Pure step function over a state value**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Rollback and replay come free |
| **Disadvantages** | Breaks the `&mut self` style of the crate; clones or moves the state per event; rollback has no consumer yet |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Possible later as a wrapper |

#### Decision & Rationale

**Chosen Option:** A.

Deriving the scene from the state removes a whole class of drift bugs, and B's convenience can be added later as a thin wrapper around A, while going from B back to A would break the interface.

#### Implementation Notes

- Layout lives in the engine, not in `StackSet`, so `stack` stays independent of `layout` (Delete Test). One layout serves every workspace until an event can change it.
- Events that no longer apply are ignored, not reported: backends see close and focus events for windows they already lost, and a duplicate open is harmless.
- `KeyPressed` does nothing until key bindings exist, so no event yields an `Effect` yet. `handle` still returns them so adding bindings does not change its signature.
- The first output is the screen; an empty output set leaves an empty scene. Multi-output is a later item.
- `scene` returns a `Result`, because a layout refuses a screen too small for its windows; the backend decides what to do.

#### References

- Core and backend boundary entry above.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-27 | Initial entry | deltaog-117 |

---

### X11 backend: the caller drives the loop

**Date:** 2026-10-27
**Status:** Confirmed

#### Context / Background

The backend reads X events, turns them into `Event`s, and applies the engine's `Scene` and `Effect`s as X requests. The open question was who owns the loop that does this.

#### Options Considered

**Option A: Backend owns the loop**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Smallest example WM: one call |
| **Disadvantages** | No place for other inputs (plugin socket, timers); adding one later breaks the interface |
| **Implementation Difficulty** | Easy |
| **Fit with Constraints** | Hides the loop the core was designed to leave to the caller |

**Option B: Caller drives the loop**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Testable without a running loop; other inputs can share a `poll` through `AsFd`; Wayland can follow the same shape |
| **Disadvantages** | About 15 more lines in every WM; the caller can get the loop order wrong |
| **Implementation Difficulty** | Medium |
| **Fit with Constraints** | Matches events-in, scene-out |

**Option C: `calloop` event source**

| Aspect | Assessment |
|--------|------------|
| **Advantages** | Same loop style as Smithay; built-in timers |
| **Disadvantages** | Heavy dependency and callback style before X11 has run once |
| **Implementation Difficulty** | Hard |
| **Fit with Constraints** | Premature; can be layered on B later through `AsFd` |

#### Decision & Rationale

**Chosen Option:** B.

It keeps the backend a thin translator and lets the plugin WM poll its socket next to X. The library choice (`x11rb`, pure-Rust connection) was settled without options: it needs no C library and works with `unsafe_code = "forbid"`.

#### Implementation Notes

- `X11Backend` has `next_events` (blocks), `pending_events` (no wait), `apply_scene`, `apply_effects` and `AsFd`. It remembers the last scene it showed and applies the `diff` itself.
- `plan` (changes to requests) and `translate` (X event to `Event`) take no connection, so they are checked by property tests: a fake server reaches every scene of a random sequence, and opens and closes alternate for every window.
- X reports a window we unmapped exactly like one the client withdrew. `Windows` counts the unmaps our own hides will cause so only client unmaps become `WindowClosed`; `forget` clears everything for a window so a reused X id starts clean.
- A `ConfigureRequest` from a managed window is ignored (the layout sizes it); from an unmanaged window it is passed through.
- Close sends `WM_DELETE_WINDOW` if the client lists it, else kills the client. Spawned children are reaped by a waiting thread.
- Requests for windows that vanished fail on the server and come back as error events, which are ignored.
- Smoke-tested by hand in Xephyr with xterm: the backend became the WM, adopted the window with its title and class, tiled it full screen and focused it.
- Left for later: adopting windows that exist before the WM starts, `grabs.rs` (with key bindings), several outputs through RandR, and automated tests against a real server (Xvfb item).

#### References

- Engine interface entry above.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-27 | Initial entry | deltaog-117 |

---

### Minimal example exit: plain loop, stopped from outside

**Date:** 2026-10-27
**Status:** Confirmed

#### Context / Background

The example window manager had to be a short `main.rs` wiring `Engine` to `X11Backend`. The engine ignores key presses and `handle` returns no effects, so nothing in the library can produce `Quit` or `Spawn` yet. The question was how the example ends.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A: Plain loop, stop from outside | No library change; matches the crate docs | No quit or spawn from inside the WM | S |
| B: Plain loop, exit when the last window closes | Terminates in scripted runs | Example-only rule that can surprise | S |
| C: Pull minimal key bindings forward | Quit, spawn and focus keys now | Hard-to-undo interface; takes over a Medium item | H |

#### Decision & Rationale

**Chosen Option:** A.

It stays inside the roadmap item and adds no interface that would be hard to undo. Key bindings get their own design pass, and the example gains quit and spawn keys afterwards. B can be added in a few lines if scripted tests need a run that ends by itself.

#### Implementation Notes

- The example lives in `crates/lutece-x11/examples/minimal.rs`, because a workspace root with no package cannot build examples. The empty `examples/minimal/main.rs` was removed.
- Smoke-tested in Xephyr: it starts and runs without errors. The tiled geometry was not confirmed by eye, because `xterm` could not load fonts on this machine; `xeyes` or `xclock` would work.

#### References

- X11 backend entry above.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-27 | Initial entry | deltaog-117 |

---

### Nested dev environment: `scripts/dev-x11`

**Date:** 2026-10-27
**Status:** Confirmed

#### Context / Background

The example window manager has to be tried on a real X server, but the host session is Hyprland and the WM must not run on it. Xephyr opens an X server as a window inside the current session, so no logout is needed.

#### Decision & Rationale

**Chosen Option:** a POSIX `sh` script, in the style of `scripts/check`, with a fixed spare display (`:2`, overridable by argument).

Only one approach was credible, so no options were compared. A fixed display keeps the commands in the docs and the example valid; auto-picking a free number would make `DISPLAY=:2 xterm` wrong. The script builds first, waits until `xdpyinfo` can connect instead of sleeping a fixed time, and stops Xephyr through a trap, so a Ctrl-C or a WM crash leaves no stray server.

#### Implementation Notes

- If the display is taken, Xephyr exits and the script reports it instead of waiting.
- Smoke-tested on `:7` with a timeout: Xephyr came up, the WM ran, and no Xephyr was left afterwards. The tiled geometry is still unchecked by eye; that belongs to the scripted X11 integration tests.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-27 | Initial entry | deltaog-117 |

---

### Key bindings: engine-owned keymap with a closed `Action` enum

**Date:** 2026-10-27
**Status:** Confirmed

#### Context / Background

`Event::KeyPressed` existed but did nothing. The core needed a way to say which key does what, the backend needed to know which keys to grab, and the example needed to quit, spawn and move focus.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Keymap owned by the engine, closed `Action` enum | Replayable and property-testable; one path for X11 and Wayland | A new action means a new variant | M |
| B. Keymap owned by the host, plus a public `Engine::apply` | Modal bindings are possible | Glue repeated in every host; weaker replay coverage | M |
| C. Matching keys in the example only | Smallest change | Logic duplicated for Wayland; untestable; against the architecture | S |

#### Decision & Rationale

**Chosen Option:** A.

It matches what the `KeyPressed` event already promised ("a key combination the core asked the backend to grab"), and key presses replay through `Engine::handle` like every other event. B only wins if modal bindings become a firm requirement, and `Engine::apply` can be added later without breaking A.

#### Implementation Notes

- A binding names the key by what it produces without Shift, so Super+Shift+q is the keysym of `q` plus the Shift modifier. The backend looks up the first column of the keyboard mapping, as dwm does.
- Each key is grabbed four times (plain, CapsLock, NumLock, both), because X matches grabs on the exact modifier mask. NumLock is assumed to be Mod2.
- The keymap is a `Vec` in binding order, not a hash map: grabs happen in a stable order and a WM has a few dozen bindings.
- `ViewTag` and `MoveToTag` with an unknown tag are ignored, like other requests that no longer apply.
- `SwapMaster` from the plan became `SwapNext` and `SwapPrev`, because those are the swap operations `StackSet` has. A swap-with-master operation can be added to `StackSet` and `Action` later.
- A unit test caught a bug: looking up keysym 0 ("no symbol") matched every unused key slot. It now names no key.
- Smoke-tested in Xephyr: the grabs succeed and the WM runs. Sending real key presses was not tested, because `xdotool` is not installed; that belongs to the scripted X11 integration tests.
- Not done: the keycode table is read once. Changing the keyboard layout while running needs `MappingNotify` handling (added to the roadmap).

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-27 | Initial entry | deltaog-117 |

---

### Adopting existing windows: scan the root's children at connect

**Date:** 2026-10-30
**Status:** Confirmed

#### Context / Background

The backend only learned of windows through map requests, so windows already open when it started were never managed and stayed wherever they were. Taking over a running session needs them reported too.

#### Options Considered

Only one approach was credible, so no alternatives were weighed: after redirecting the root, list its children and adopt the ones that are showing. Re-mapping every window to provoke a map request would flash the screen and break clients that expect to stay mapped.

#### Decision & Rationale

**Chosen Option:** scan in `connect`, after the redirect is in place.

Asking after the redirect closes the race: a window that maps later sends a map request, and one that mapped earlier is in the list. The events reuse `WindowOpened`, so the core needed no change and Wayland can do the same for its own sources.

#### Implementation Notes

- Only windows in `Viewable` state and without override-redirect are adopted. Unmapped ones send a map request when they want to show; override-redirect ones (menus, tooltips) opted out of management.
- An adopted window is recorded as on screen, so hiding it later sends the unmap it needs and the resulting unmap event is not mistaken for a client close.
- Stacking order from `query_tree` (bottom first) becomes the order of the `WindowOpened` events, so the first-opened window is the lowest.
- Attribute requests are sent together before any answer is read. A window destroyed in between is skipped.
- Not done: iconic windows (minimised by a previous WM) are skipped, and every adopted window lands on the first workspace. Both belong to manage rules.
- Smoke-tested in Xephyr: the example starts and runs with two clients already open. Their resulting geometry was not checked, because `xdotool` and `xwininfo` are not installed; that belongs to the scripted X11 integration tests.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-30 | Initial entry | deltaog-117 |

---

### Layout and master-ratio keys: the layout lives on each workspace

**Date:** 2026-10-30
**Status:** Confirmed

#### Context / Background

The layout was one value on `Engine`, shared by every workspace, so there was nowhere for a key to change it per workspace. The roadmap asked for actions to switch the layout and resize the master column.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. One engine-wide layout, cycling actions change it | Smallest change; nothing in `stack` moves | All workspaces share one layout; going per-workspace later changes engine behaviour | S |
| B. Layout stored on each `Workspace` | Each tag keeps its layout and ratio, as in dwm; state is covered by workspace tests | Touches `Workspace` and `StackSet` | M |
| C. As A, with `SetLayout(Layout)` and `AdjustMaster(i16)` | A key can pick a layout directly | A binding can carry a stale ratio; still engine-wide | S |

#### Decision & Rationale

**Chosen Option:** B.

The library is the base for two window managers, and sharing one layout across workspaces is the wrong model for both. Doing it before `Action` has users avoids a behaviour change later.

#### Implementation Notes

- `Engine::new` still takes a layout; it becomes the starting layout of every workspace (`StackSet::with_layout`, crate-private), so existing callers compile unchanged.
- Cycling goes Tile, Monocle, Tile. A workspace remembers the tile ratio while monocle shows, so a round trip does not reset the width.
- `MasterRatio::step` clamps to 1..=999, keeping the type's guarantee, and a property test checks that any step stays in range and is exact when it can be.
- Resizing is ignored in monocle, like other requests that no longer apply. The step (50 thousandths) is a constant in the engine, not part of the action.
- Not done: `SetLayout` for picking a layout directly. It is not needed until a third layout exists.
- Not tested against a real X server, for the same reason as the earlier key work: `xdotool` is not installed.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-30 | Initial entry | deltaog-117 |

---

### Manage rules: ignore and workspace rules, float later

**Date:** 2026-10-30
**Status:** Confirmed

#### Context / Background

Every new window opened on the current workspace and was tiled. The roadmap asked for rules that skip a window, send it to a workspace or float it. Float needs a floating layer in the scene and a source of geometry (`WindowInfo` carries none), which is its own roadmap item.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Ignore and workspace rules only | Small, fully testable, no placeholder model | No float rule yet | S |
| B. Rules plus a minimal floating layer | Float works immediately | Touches scene, stack and both layers of X11; pre-empts the floating item | H |

#### Decision & Rationale

**Chosen Option:** A.

Every rule it offers does something real, and matching is property-testable. `Outcome` is `non_exhaustive`, so `Float` is added with the floating item, where the geometry question is solved once.

#### Implementation Notes

- Rules are data: a `Match` with one criterion (exact class, exact title, or dialog) and an `Outcome`. Two conditions mean two ordered rules, so a rule can never test nothing. First match wins.
- `Engine::with_rules` returns an error for a `SendTo` tag the engine lacks, so `handle` never meets an unknown tag.
- A window sent to another workspace is focused there; the view and its focus do not change.
- Finding while implementing: the X11 backend holds a window back at its map request and maps it only through the scene, so an ignored window would have stayed invisible. `Effect::Release` fixes this: the backend forgets the window and maps it. Its later configure requests are granted because it is no longer managed.
- Not done: matching by substring or several fields at once; the X11 `Release` is not tested against a real server (no `xdotool`), only the translation side is.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-30 | Initial entry | deltaog-117 |

---

### Floating windows: offsets from a default rectangle

**Date:** 2026-10-31
**Status:** Confirmed

#### Context / Background

The manage rules left a `Float` outcome for later, because the core knows neither a window's size nor the pointer. A floating window needs a rectangle from somewhere, and a way to change it.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Default rectangle plus move and resize keys | Complete feature, core only, property-testable | Every floating window starts the same size | M |
| B. The window's requested size plus keys | Windows open at the right size | New `WindowInfo` field and X11 geometry query | H |
| C. Float outcome only, fixed rectangle | Smallest change | No move or resize; needs a follow-up | S |

#### Decision & Rationale

**Chosen Option:** A.

It delivers the whole roadmap item inside the core, where the model-based and replay tests already live. B only adds the initial size, which can later seed the same state from `WindowInfo`.

#### Implementation Notes

- A `Float` stores how far the window was moved and resized from a default rectangle (60% of the screen, centered), not a rectangle. It is therefore valid before a screen is reported and follows the screen when it changes; `Float::rect` never fails except through `Size::new`, which it cannot trip because the size has a 64-pixel floor.
- Resizing clamps the stored offset at the floor, not just the drawn size. Otherwise shrinking past the floor would build up a hidden debt and the next grow key would seem to do nothing.
- Float state lives in `Workspace`, next to the stack. `remove` drops it, `set_float` refuses windows the workspace lacks, and `move_focused_to` carries it across, so a floating window is always a window of its workspace.
- The scene lists tiled windows as the layout places them, then the floating ones in stack order, so floating windows are always on top. The layout only sees tiled windows; in monocle a focused floating window is not in that list and the layout ignores it.
- The move and resize actions take a `Direction`; for resize, right and down grow, left and up shrink. Moving or resizing a tiled window does nothing, and resizing with no screen does nothing.
- Not done: the window's own requested size (option B), mouse move and resize, raising a floating window on focus, and keeping a window on screen.
- Not tested against a real X server: `xdotool` is not installed. The X11 backend needed no change, since it already applies placements in order.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-10-31 | Initial entry | deltaog-117 |

---

### Keyboard remapping: regrab when the keycode table changes

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The backend fetched the keycode table once at connect. After a layout switch (`setxkbmap`, `xmodmap`) the same key produces another keysym, so grabs sat on the wrong keys and presses were translated with a stale table.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Remember the grabbed presses; refetch and regrab on `MappingNotify` | Bindings follow their keys; no new API | The backend keeps a list of presses | S |
| B. Ask the host to call `grab_keys` again on a new event | Backend stays stateless | Every host repeats the glue; needs a new `Event` | M |
| C. Grab every key and match in the core | No regrab ever | Steals all keyboard input from clients | H |

#### Decision & Rationale

**Chosen Option:** A.

It is the only option that needs no change in the core or in hosts. The backend already owns the keycode table, so keeping it current is its job.

#### Implementation Notes

- Only a keyboard remap (`Mapping::KEYBOARD`) triggers it. Modifier and pointer remaps are ignored: CapsLock and NumLock are always grabbed on Lock and Mod2, so those grabs stay valid.
- `regrab_keys` drops every key grab on the root and grabs again. Every key grab on the root is this client's, so nothing foreign is dropped.
- A press whose keysym no key produces any more is skipped, not an error: a layout switch must not end the window manager. It is grabbed again by the next remap that brings the key back.
- If another client grabbed a combination in the meantime, the error is `KeyAlreadyGrabbed`, as at the first grab.
- Not tested here: the new integration test swaps two keys on a real server, but no headless server is installed on this machine, so it only compiled locally.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### X11 integration tests: example WM on Xvfb, keys through XTEST

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

Nothing so far checked real window geometry or key presses against an X server. The roadmap named `xdotool` for this.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Test client in Rust over x11rb, keys through XTEST | No external tool beyond the server; typed geometry reads | A small client to maintain | M |
| B. Shell out to `xdotool` and `xwininfo` | Less code | Two more tools to install; text parsing | S |

#### Decision & Rationale

**Chosen Option:** A.

The tests already depend on x11rb, and XTEST fakes key presses the way a keyboard would. CI then needs only `Xvfb`.

#### Implementation Notes

- `tests/integration` starts its own `Xvfb` on a display number between 90 and 190 that has no socket or lock file, runs the `minimal` example on it, and acts as a client: it opens windows, presses keys, and polls their geometry with a timeout.
- Without `Xvfb` each test prints a notice and passes; with `LUTECE_REQUIRE_X11` set a missing server is a failure.
- Covers tiling, rules, floating, layout and master keys, workspaces, closing and quitting, and the keyboard remap.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Headless CI: `scripts/ci` and a GitHub Actions workflow

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The integration tests skip when `Xvfb` is missing, so a CI runner without it would report success for tests that never ran.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. `scripts/ci` sets `LUTECE_REQUIRE_X11` and runs `scripts/check`; the workflow only installs Xvfb and calls it | Same command locally and in CI; one place for the rule | One more script | S |
| B. Put the steps in the workflow only | No extra script | Cannot be reproduced locally | S |

#### Decision & Rationale

**Chosen Option:** A.

The check stays the single source of what is verified. The workflow is the only host-specific part.

#### Implementation Notes

- `scripts/ci` fails at once if `Xvfb` is not on the path, then runs `scripts/check` with `LUTECE_REQUIRE_X11=1`.
- `.github/workflows/ci.yml` runs on pushes to `main` and on pull requests, with read-only permissions.
- Not done: toolchain and dependency caching, and a run on a real runner. The workflow has not run yet.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### More layouts: grid and centered master

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The layout enum had tile and monocle. The roadmap asked for grid and centered master as small separate modules.

#### Options Considered

Only one approach was credible, so no COAs: the enum is closed on purpose, so new layouts are new variants with their own module.

#### Decision & Rationale

**Chosen Option:** Add `Grid` and `CenteredMaster(MasterRatio)`, each in its own file, with a small `span` helper that cuts a length into parts and gives leftover pixels to the first parts.

Centered master takes a ratio like tile, and the workspace keeps one ratio for both, so `GrowMaster` works on either and the ratio survives a trip round the cycle. With one or two windows there is nothing to flank the master, so centered master falls back to tile.

#### Implementation Notes

- Cycle order is tile, monocle, grid, centered master. The two existing tests that expected a two-step cycle were changed to the four-step one.
- `tile.rs` keeps its own copy of the splitting arithmetic; it was left alone to keep the change small.
- Property tests check, for both layouts, that cells cover the area exactly with no overlap, and that too-small areas fail with `AreaTooSmall`.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Multi-output: one shared pool of workspaces, each screen shows one

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The engine laid out one workspace on the first output the backend reported. Several monitors needed a decision on who owns the workspaces and what moving focus between outputs means.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. A `StackSet` per screen (dwm style) | `StackSet` untouched; per-monitor tags | A tag no longer names one workspace; unplugging must migrate windows by hand | M |
| B. One shared pool, each screen shows one workspace (xmonad style) | One window on one workspace stays enforced in one type; unplugging cannot lose a window; tags stay unique | Large change to `StackSet` and its tests | H |

#### Decision & Rationale

**Chosen Option:** B.

Correctness decided it. A workspace is always either on exactly one screen or hidden, so a window cannot be lost or duplicated when an output goes away, and the model-based property test could be extended instead of replaced. Tags keep one meaning, so manage rules and `ViewTag` / `MoveToTag` needed no change. The cost was a bigger `StackSet`; A would be the pick if lucarne needs the same tag name independently on each monitor.

#### Implementation Notes

- `StackSet` keeps a zipper of screens (output id plus workspace) and a list of hidden workspaces. A workspace carries its position in the tag order, because where it is stored no longer says where it belongs. `workspaces()` and `tags()` still return tag order.
- `set_outputs` rebuilds the screens from a copy and swaps the result in, so no early exit can leave a workspace out. A new output takes the workspaces of vanished outputs first, so replacing a monitor keeps its workspace; an empty report changes nothing, so a returning monitor finds the old layout of screens.
- Viewing a workspace another screen shows moves the focus there instead of swapping the two (the xmonad rule).
- The engine keeps the reported outputs and their areas; the screens in `StackSet` only know output ids. The scene stays a flat list of rectangles in global coordinates, so `diff` and the backends' scene code did not change.
- X11 outputs are RandR 1.5 monitors, identified by their name atom, not RandR outputs: a monitor is what a user sees as one screen, and a server can define its own. Without RandR 1.5 the whole root window is output 0.
- Not verified on a server: no `Xvfb` was available here, so the `monitors` integration tests are only compiled. A smoke test in Xephyr was inconclusive because its RandR cannot set monitors. It is also unknown whether `SetMonitor` produces a RandR notify event; the backend selects screen, output and CRTC changes to cover the likely cases.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Wayland backend: a Smithay compositor the caller drives, built in three slices

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

On X11 another server draws and lutece only manages windows. On Wayland the backend is the server: it accepts clients, tells each one its size, handles the keyboard and draws. A whole backend did not fit one iteration, so the question was what the first slice delivers and how it is tested.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Protocol first, no pixels | Testable without a screen, mirrors the X11 approach | Nothing visible | H |
| B. Nested winit window first | Visible result, closest to the final backend | Hard to test; protocol and GL bugs mixed | H |
| C. Toolchain spike only | Smallest; proves Smithay builds here | No WM behavior | S |

#### Decision & Rationale

**Chosen Option:** C, then A, then B, all in this cycle.

The spike first because a failed Smithay build would have invalidated the design. A second because it gives the test-first footing the X11 backend has, so the drawing in B sits on a protocol layer that is already checked. B last because it is the only slice that needs a screen. The slices were built in that order; the spike was dropped once B's example covered what it proved.

#### Implementation Notes

- **Spike:** my first dependency check looked for `libxkbcommon` and concluded the development files were missing; the pkg-config name is `xkbcommon`, and everything Smithay needs was installed. Smithay 0.7 built without trouble with `backend_winit`, `renderer_gl`, `wayland_frontend` and `desktop`.
- **No event loop framework:** the backend polls the listening socket and the display's file descriptor with `rustix`, so the caller-owns-the-loop shape of the X11 backend is kept and `calloop` is not needed. `NestedBackend::next_events` alternates between the window system and the clients in 8 ms slices, which bounds key latency; it is a polling loop and uses CPU while idle.
- **When a window opens:** a toplevel is reported on its first commit, because title and app id are set before it. The initial configure is sent right then, so a window the core never places still finishes its handshake.
- **Scene application:** `Show` and `Move` set the toplevel's size and send a configure; focus sets the active state and the keyboard focus. Position lives only in the renderer. `Hide` clears the rectangle, so the window is not drawn but stays alive.
- **`Effect::Release`:** Wayland has no client to hand a window back to, so the window is dropped from the managed set and never sized or drawn again.
- **Keys:** the grab test uses the keysym a key produces with no modifier applied (the same naming as X11) plus Shift, Ctrl, Alt and Super; locks are ignored. A grabbed key's release is hidden too, so a client never sees half a key press. The public `key_event` takes a plain keycode and a `KeyAction` so Smithay types stay out of the public API.
- **Tests:** a real `wayland-client` client and the backend run on one thread and are stepped in turn, so nothing blocks and no thread races. Seven integration tests plus a property test for modifier translation. I removed the key grab to confirm the grabbed-key test fails without it.
- **Duplication:** `Flow` exists in both backend crates. Moving it into the core would touch the X11 crate, so it was left.
- **Not done, on purpose:** pointer input, drawing and positioning popups (they are configured only), clipboard (no data device), `linux-dmabuf`, a real session backend (DRM, libinput), several outputs, XWayland.
- **Not verified:** the nested WM was only smoke-tested inside Hyprland (it ran, a client stayed connected, nothing was printed). Nobody has looked at the window, so that windows are drawn at the right place is unconfirmed. Alacritty prints EGL warnings because no dmabuf is offered; it falls back to shared memory.
- **Tooling:** the formatting hook orders imports differently from `cargo fmt` (edition 2024), so `cargo fmt --all` must be run before the check.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Pointer drags: one core event, focus stays in the backend

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The Wayland backend had no pointer. Hover focus fits the existing `FocusRequested` event, but moving and resizing a floating window with the mouse has to reach the core, because a float is stored there as offsets from a default rectangle. Popups and the clipboard were built in the same cycle and needed no decision.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Focus only, pointer stays in the backend | No core change, easiest to finish | No mouse move or resize | S |
| B. A plus one float-drag event | Finishes the roadmap item with a small contract change | One new shared event; X11 does not emit it yet | M |
| C. Raw pointer events in the core | Full pointer control for the plugin protocol | Large, hard to undo, duplicates hit-testing | H |

#### Decision & Rationale

**Chosen Option:** B.

The core already had the pure pieces (`Float::moved` and `Float::resized`), so the only new contract surface is `Event::FloatAdjusted { id, dx, dy, dw, dh }`, and `Event` is `#[non_exhaustive]`, so adding a variant breaks nobody. C would have been the hardest to undo for a need nobody has yet; A would have left the mouse half of the item for a later core change anyway.

#### Implementation Notes

- **Hit-testing and focus stay in the backend.** It knows which surface is under the pointer, including popups and subsurfaces, and only a window with a buffer can be hit, so a pointer over a window's unused margin changes nothing.
- **The drag modifier is a backend setting**, Alt by default, because the backend swallows the click and the core cannot know what to swallow. `Modifiers::NONE` turns drags off.
- **Drags are whole pixels.** The backend sends integer deltas and carries the sub-pixel remainder forward, so slow motion is not lost. A resize always grows from the bottom-right corner.
- **The engine decides what a drag means.** `FloatAdjusted` for a tiled or unknown window does nothing, so the backend does not need to know which windows float. A resize is measured against the focused output's area, the same limitation as the resize keys.
- **Popups** are tracked with Smithay's popup manager, configured after their first commit, placed at their parent's origin plus the position the client asked for, and listed in front of their window. A popup that grabbed the pointer is dismissed by the first click outside popups. Popups are not kept on the output.
- **Clipboard:** only the selection focus follows keyboard focus; the compositor never reads a selection. Drag and drop between clients is wired but has no icon and is untested.
- **Cursor:** a named cursor is passed to the host window, a client-drawn one is drawn with the windows, and a hidden one hides the host cursor.
- **Tests:** core tests include a property test that a drag and its reverse restore the scene. The Wayland tests use real shm buffers because a surface without one has no size and cannot be hit. I disabled the selection focus and the popup layers in turn and confirmed the matching tests fail.
- **Not verified:** the pointer and cursor code in the nested window, popups drawn on screen and a real paste between terminals have not been looked at; the nested example was smoke-tested only (it ran, a client stayed connected, nothing was printed).
- **Not done:** the X11 backend does not report drags; clients still draw their own decorations (no `xdg-decoration`); window geometry offsets are ignored.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Wayland outputs: a set of outputs on the backend, virtual monitors in the nested window

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The Wayland backend had one hard-wired output, so `FocusNextOutput` and the other multi-output actions could not be tried on it. The same cycle added X11 pointer drags, `xdg-decoration` with window geometry offsets, and popup placement on the output. Those had one credible approach each; only the outputs needed a choice, because Smithay's winit backend opens exactly one host window.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Virtual monitors inside the one window | Tests the multi-output core on Wayland, no new dependency | Simulated hot-plug, one shared scale | M |
| B. Single output that follows the host | Smallest change, handles scale | Multi-output stays untestable on Wayland | S |
| C. One host window per output | Real independent windows | Needs our own winit and EGL layer | H |

#### Decision & Rationale

**Chosen Option:** A.

A and B both need the backend to hold a list of outputs, and A adds only the cut of the window on top. It is also the only option that lets popup placement be tried against more than one output. C is not possible on Smithay's winit backend.

#### Implementation Notes

- **The backend takes the complete set**, like the core's `OutputsChanged`: `set_outputs` creates a `wl_output` global for a new id, removes the global of a missing id and moves a kept one. Several sets reported before the caller collects events are merged into the latest, in place, so the core hears one.
- **The nested window is cut by position, not by summing widths**, so the pieces cover it exactly and differ by at most a pixel; a window narrower than the count gets one output per column. A property test checks the tiling.
- **Decorations:** the answer is always server side, because the compositor draws no frames. The configure is sent at once only for a window that already had its first one.
- **Geometry offsets:** the scene rectangle is for the window geometry, so the surface is drawn at the rectangle minus the declared geometry origin. A popup's position is relative to the parent geometry, so it needed no change.
- **Popups:** Smithay's `get_unconstrained_geometry` applies the client's own slide, flip and resize rules against the output of the parent window, measured from the parent's origin; a popup of a popup is shifted by where the parent popup sits. It is decided when the popup opens or is repositioned, so a window dragged afterwards takes its popups along unconstrained.
- **X11 drags:** the buttons are grabbed on the root with the modifier (and CapsLock/NumLock variants), so the click never reaches the window. Unlike Wayland there is no default modifier: the grab can fail when another client holds it, so `set_drag_modifier` is explicit and returns an error.
- **Not verified:** none of this has been seen on screen. The nested example with several outputs, decorations in Alacritty, a popup near an edge and X11 drags in Xephyr are checked only by scripted clients and unit tests. The X11 drag path has no integration test, because `Xvfb` is not installed here.
- **Not done:** host scale changes (every output draws at scale 1), clipping a client's shadow to its geometry, and re-constraining popups when their window moves.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Wayland host scale: fractional scale in 120ths, logical pixels in the core

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

The nested window may be shown at a scale other than 1, and every output was drawn at scale 1. The same cycle clipped a client's shadow to its window and dealt with popups when their window moves; those had one credible approach each (Smithay's crop element for the shadow), only the scale needed a choice.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Whole-number scale | Sharp; small change | Wrong size on 1.25 or 1.5 hosts | M |
| B. Fractional scale and viewporter | Sharp and exact at any scale | Two new protocols; part visible only on screen | H |
| C. Stretch the finished frame | Smallest change | Blurry; to be redone later | S |

#### Decision & Rationale

**Chosen Option:** B.

The roadmap item says to follow the host's scale, and only B does so at every factor. A would be wrong on exactly the fractional hosts that need this most. The core and the toplevel configure sizes stay in logical pixels, which is what xdg-shell uses, so no layout code changed.

#### Implementation Notes

- **`Scale` is a count of 120ths in a non-zero integer**, the protocol's own resolution, so it is never zero, negative or not a number, compares exactly, and the value drawn is the value sent. All pixel conversions are integer arithmetic rounding half up. A rectangle is converted edge by edge so rectangles that touch still touch.
- **Where the scale is applied:** `wl_output` gets the physical mode (logical size times the scale) and the scale rounded up, with the location staying logical; the renderer gets the scale and the physical position of every surface; the nested window divides the host's pixels by the scale for the output size and the pointer.
- **Clients that do not bind `wp_fractional_scale`** stay at scale 1 and are enlarged when drawn. The compositor is at version 5 and sends no `wl_surface.enter`, so `wl_output.scale` alone does not reach them; listed in the roadmap.
- **Shadows:** a window's surface tree is cropped to the placed rectangle (not the declared geometry): it is the same for a client that honours its size, and it also holds back one that ignores it. Popups are not cropped. Cropping is Smithay's `CropRenderElement`; the draw list now holds an enum of plain and cropped elements.
- **Popups:** the first plan was to place a popup again when its window moved. The protocol does not allow it: a compositor may configure a popup only when it opens or after the client's reposition request, and Smithay refuses the configure (`AlreadyConfigured`). A popup now follows its window as before, and one whose placement would differ after the move is dismissed, like a menu that closes when its window is dragged. A popup of a popup is judged from where its parent popup was acknowledged to be.
- **Tests:** unit and property tests for the scale; protocol tests with a real client for the preferred scale (at binding and after a change), the output's scale and physical mode, the viewport, and popup dismissal. The crop and the drawing at a scale cannot be tested without a GPU.
- **Not verified:** none of the drawing has been seen on screen: the crop of a real shadow, text sharpness at 1.5 and the pointer position on a scaled host.
- **Not done:** `wl_surface.enter` for clients without fractional scale; moving a popup when its window moves (not possible); `linux-dmabuf`.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Wayland output enter and dmabuf: the GPU named, the overlap worked out from rectangles

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

Two roadmap items done together. Clients that do not bind `wp_fractional_scale` read `wl_output.scale`, but only for outputs their surface has entered, and the compositor sent no enter, so they drew at 1 and were enlarged. Separately, GPU clients fell back to shared memory because there was no `linux-dmabuf`. The enter had one credible approach; the dmabuf had a choice.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Formats only (dmabuf v3) | No new dependency, smallest change | No GPU hint, so two-GPU machines may misbehave | S |
| B. Formats and render GPU (dmabuf v4) | Correct on hybrid GPUs; the DRM backend will need the same | Adds the `drm` crates | M |

#### Decision & Rationale

**Chosen Option:** B for the dmabuf. Correctness ranked first and only B is right with two GPUs; the `drm` crates are small bindings with no new system library, and the planned DRM session backend must identify its GPU the same way. A's only win was effort.

For the enter, the render-pass helper in Smithay was rejected: it needs the renderer, so it could not be tested headlessly and would tie the protocol side to drawing. The core already gives each window's rectangle and the backend knows each output's, so overlap is a pure function.

#### Implementation Notes

- **Overlap rule:** two rectangles overlap when they share at least one pixel, so touching edges do not. It is computed in 64 bits so a rectangle at the edge of `i32` cannot overflow, and property-tested for symmetry, containment and gaps.
- **When it runs:** `refresh_presence` is idempotent (Smithay's enter and leave ignore repeats), so it is called on place, hide, new popup, new subsurface, output set changes and scale changes. Not on every commit, so the per-frame path is unchanged. An output that is unplugged is left first, before its global goes.
- **Which surfaces:** the window's surface tree and its popups' trees share the window's outputs; a popup reaching past its window is not worked out separately, which is fine because every output shares one scale.
- **Buffer scale:** `preferred_buffer_scale` is the host scale rounded up, equal to what `wl_output` says. It needs compositor version 6, which was 5; version 6 adds only these events.
- **dmabuf split:** the backend owns no renderer, so `enable_dmabuf` takes a `DmabufSupport` (device number and `(fourcc, modifier)` pairs, no Smithay types in the public interface). The nested backend builds it from its EGL device and render node. With software rendering there is no node, and no global is offered.
- **No GPU import test:** the renderer sits in the host window layer, out of reach while a request is served, and `create_immed` needs an answer at once, so a queue answered later is not possible. Smithay refuses a format that was not offered (a protocol error that ends the client); a buffer the driver still rejects is skipped when drawn.
- **Tests:** protocol tests with a real client for enters at the seam, on move, hide, unplug and output growth, for buffer scale and a later change, for popups, and for the dmabuf global, its feedback (device and table size) and buffer creation, using a synthetic format list since no GPU is involved.
- **Not verified:** a real GPU client in the nested window; that the EGL device gives a render node on this machine; text sharpness of a GTK 3 client at 1.5.
- **Not done:** per-surface dmabuf feedback for scan-out (belongs with the DRM backend); the legacy `wl_drm` path.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### XWayland windows: a loop inside the backend, menus handled, clipboard left out

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

X11 programs (Steam, older Java and Electron apps) cannot speak Wayland. Xwayland is an X server that runs as a client of the compositor and shows each X11 window as a surface. The roadmap item was to treat those windows as one more window source. The same cycle ran the X11 integration tests for the first time on a real `Xvfb` and added the missing XTEST drag test; those are recorded below because they changed what is known about the tests.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| A. Windows plus menus and tooltips | Most useful per unit of work, reuses the popup logic | No X11/Wayland clipboard, Xwayland starts eagerly | M |
| B. Normal windows only | Smallest | Menus invisible, half the job redone later | S |
| C. A plus clipboard bridge plus start on demand | Complete | Touches the clipboard code, hardest to test | H |

#### Decision & Rationale

**Chosen Option:** A. It ranked first on correctness and fit (X11 windows go through the same `WindowOpened` path and one scene; menus reuse the layer and hit-test code) without B's unusable menus or C's extra risk. The clipboard bridge and starting on demand are separate roadmap items.

#### Implementation Notes

- **The loop:** Smithay runs Xwayland and its window manager on a `calloop` loop, and this backend is driven by the caller's loop. The backend now holds a `calloop` `EventLoop` of its own, steps it without waiting in `next_events`, and polls its file descriptor with the other two, so the caller's loop is still the only one that blocks.
- **One window type:** a managed window is a `Shell`, either a Wayland toplevel or an X11 window. The few places that read the toplevel directly (placing, focusing, closing, presence, popups) go through it. A `Layer` may now belong to no managed window; the pointer then asks for no focus and starts no drag.
- **When an X11 window is announced:** after its map request has been granted and its surface has been tied to it. The tie happens when the X11 window's serial and the surface's serial meet, in either order, so the check runs at both events. Doing it only at a commit missed it: the window is tied just before the commit hook the backend sees.
- **Placing:** one configure request with position and size. A configure request from a managed window is answered with its current geometry; anything else gets what it asks for.
- **Menus (override-redirect):** tracked when mapped, never given to the core, drawn in front of all windows at the geometry their program set, uncut. Moving the pointer onto one resets which window the pointer is over, so it asks for no focus.
- **Keyboard focus:** a Wayland surface alone does not give an X11 window the X input focus (the test saw `PointerRoot`). Smithay does it when the X11 window is the focus target, so the seat's focus type is now a small enum of either kind; selections still follow the surface.
- **Xwayland as a client:** its client data differs from the other clients', so the compositor state lookup tries both. The `xwayland_shell` global is created at start but only Xwayland can see it.
- **Spawned programs** get `DISPLAY` once Xwayland is ready, so they use it and not the session's X server.
- **Failure:** a failed start (the program exits, or its window manager cannot connect) is stored and returned once by `next_events` as `Error::XWayland`; the events collected in that call are returned by the next one.
- **Tests:** a real Xwayland and an X11 client built with `x11rb`: reported with title and class, placed, two windows share the screen, close, menu unmanaged and in front, X input focus. They skip when `Xwayland` is missing unless `LUTECE_REQUIRE_X11` is set.
- **Test pitfalls found:** a window keeps its small original buffer until its program redraws (no frame callbacks are sent in tests), so a pointer point has to be inside that buffer, not inside the placed rectangle.
- **X11 tests run for the first time:** 18 of 18 failed at first, then 17 passed, then all. Cause one: both examples were named `minimal`, so building the Wayland example overwrote the X11 binary in `target/<profile>/examples`; the Wayland one is now `minimal_wayland`. Cause two: the key-swap helper read the second key after overwriting the first. `monitors` passed, so RandR changes on `Xvfb` reach the backend.
- **Drag tests:** left-drag moves by the pointer distance, right-drag resizes, no modifier changes nothing, with XTEST button and motion events.
- **Not verified:** nothing has been seen on screen. Real X11 programs, scaled output with X11 windows, and the clipboard are untested.
- **Not done:** the X11/Wayland clipboard bridge; starting Xwayland on demand; stacking and active-window hints for the X side; honouring a program's own move and resize requests (they are ignored on purpose, the core owns geometry).

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### X11 and Wayland clipboard: formats cross early, data crosses on paste

**Date:** 2026-11-02
**Status:** Confirmed

#### Context / Background

Copy and paste worked between Wayland clients but stopped at Xwayland, which keeps selections of its own. The roadmap item was to bridge the two through the hooks Smithay's X11 window manager offers.

#### Decision & Rationale

**Chosen Option:** offer only the list of formats when a selection is set, and move the data when someone pastes, straight from the program that copied it. Copying the data up front would cost memory and time for every copy that is never pasted, and could not work for large or lazily produced data.

#### Implementation Notes

- **Wayland to X11 and back:** a selection set by a Wayland client is offered to Xwayland, and one set by an X11 program is offered to the Wayland clients, for the clipboard and the primary selection alike.
- **Access:** an X11 program may read a Wayland selection only while an X11 window has the keyboard focus, as a Wayland client may only while one of its surfaces does. Without that, any background X11 program could read the clipboard.
- **Clearing:** when an X11 program gives up a selection, it is cleared only if the X11 side set it. One a Wayland client holds is left alone.
- **Event loop handle:** the transfers started by the X11 window manager need the backend's loop, so the compositor state now keeps its handle.
- **Input module:** keyboard and pointer handling left `NestedBackend` for an `input` module that takes events from any Smithay input backend, so a libinput source can use it later. A relative pointer is kept inside the output.
- **Smithay features:** the DRM, GBM, udev, libinput, libseat and multi-GPU features were enabled ahead of a session backend. Nothing uses them yet.
- **Not verified:** nothing has been seen on screen; the bridge is only tested with scripted clients.
- **Not done:** the session backend for real hardware.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-02 | Initial entry | deltaog-117 |

---

### Reserved space and fullscreen: two contracts, one rule for each

**Date:** 2026-11-03
**Status:** Confirmed

#### Context / Background

The next library work was fullscreen windows, X11 struts and Wayland layer-shell. The three share one need in the core: a bar must shrink the area tiles share without shrinking what a fullscreen window covers, and a backend must be able to tell a client it is fullscreen.

#### Options Considered

| Option | Advantages | Disadvantages | Difficulty |
|---|---|---|---|
| Reserve: a separate `ReservedChanged` event | The output keeps its true size; monitor matching is untouched | One more event and map | M |
| Reserve: `Output` carries a usable rectangle | Fewest event kinds | A bar change looks like a monitor change; breaks every `Output { .. }` | M |
| Fullscreen: a flag in the scene | One source of truth; `diff` and its property tests extend | `Scene` and `Change` grow | M |
| Fullscreen: a one-shot `Effect` | Scene unchanged | State can drift; contradicts "placement and focus live in the scene" | S |

#### Decision & Rationale

**Chosen Option:** a separate event for the reserve, and the state in the scene. Both were picked from the COA tables. One detail differs from the text of the second COA: the flag is not a field of `Placement` but a set on `Scene` (`with_fullscreen` checks that each window is listed). A field would have meant touching about twenty `Placement { .. }` literals in the layouts for no gain, and the set keeps the same property: the scene says it, `diff` reports it.

#### Implementation Notes

- **Insets:** `Insets` is four plain pixel counts; `Rect::inset` clamps so a result is never empty. `Insets::max` is for two bars on one edge, which both start at the screen's edge, so the larger wins, not the sum.
- **A claim on most of an axis is ignored on that axis.** A property test over random event streams found that a bar reserving nearly the whole width left a 1-pixel area, and `scene()` then failed with `AreaTooSmall`. Any X11 client can set a strut, so a client could break the scene. `Insets::limited_to` drops a claim that takes more than half of an axis, whole, not partly: easy to state and to test.
- **Where the reserve is kept:** in the engine, per output, and not dropped when an output goes away, so a bar that outlives a monitor change is still honoured when the output returns. Backends therefore reset an output they no longer have.
- **Fullscreen is the focused window's:** `Workspace` keeps `fullscreen` only while it equals the focus, and every focus change settles it. Otherwise the window with the keyboard could be hidden behind a fullscreen one. Cost: opening a dialog ends a video's fullscreen. The alternative, keeping it and raising the dialog, was left out as more state for a less common case.
- **Scene order:** the fullscreen window is removed from its layout position and put last on its output, with the whole output as its rectangle. The other windows keep the slots they would have had, so leaving fullscreen reflows nothing.
- **A request from a window that is not focused** focuses it first, on its own workspace, without changing what any screen shows.
- **`diff` order:** fullscreen changes come before the window's show or move, so a Wayland backend can send the state and the size in one configure.
- **Not verified:** nothing has been seen on screen.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-03 | Initial entry | deltaog-117 |

---

### X11 bars: dock windows tracked, struts per monitor, EWMH advertised

**Date:** 2026-11-03
**Status:** Confirmed

#### Context / Background

A bar on X11 is an ordinary window to a window manager that does not know better: it was tiled with the others. The roadmap asked for struts and the fullscreen hints.

#### Decision & Rationale

**Chosen Option:** a window of type `_NET_WM_WINDOW_TYPE_DOCK` is mapped where it asks and kept out of the core; only what it reserves reaches the core. The strut is worked out per output in a small pure module so it can be property-tested without a server.

#### Implementation Notes

- **Struts are screen-wide, the core's reserve is per output.** A strut is measured from the edges of the whole X screen and may cover only part of an edge (`_NET_WM_STRUT_PARTIAL`). For each output the module finds how far each strut reaches into it, so a bar over the left of two monitors reserves space there and not on the right.
- **Reporting:** the backend keeps what it told the core for each output and sends only the differences, resets outputs that vanished, and sends the reserve before the adopted windows at startup so the first scene already leaves room.
- **Fullscreen:** a client message `_NET_WM_STATE` (add, remove, toggle) naming the fullscreen atom becomes `FullscreenRequested`; a toggle is worked out from what was last shown. A window that sets the state before mapping is read at its map request. The backend writes the property from the scene by changing only its own atom, and clears it when a fullscreen window is hidden, because the core forgets the state then and a client that left fullscreen while hidden would come back claiming it.
- **EWMH:** `_NET_SUPPORTED` and a check window named `lutece`. Toolkits use them to decide whether to ask the window manager for fullscreen or to resize themselves.
- **Test pitfall, a flaky harness:** the 13 new X11 tests made the suite fail in about half the runs. The same suite at the previous commit failed in 3 of 6 runs, so the fault was already there. The private `Xvfb` restarted when the readiness probe closed its only connection, and the test client's connection landed in the restart (`Connection reset by peer`). Starting it with `-noreset` fixed it: 0 failures in 12 runs on the old suite, 0 in 12 on the new one.
- **Test pitfall, a stale binary:** the harness builds the example only when it is missing, and `cargo test --test integration` does not rebuild examples, so the first run of the new tests used the old window manager and tiled the bar. Rebuild the example (`cargo build -p lutece-x11 --example minimal`) after changing it.
- **A fullscreen request from a window that is not focused** loses to the pointer: the stale enter event for the window under the pointer re-focuses it, which ends the fullscreen. That is the intended rule, so the X11 test uses the focused window, and the core tests cover the other case.
- **Not done:** bars that are override-redirect are not tracked.
- **Not verified:** a real bar (`tint2`, `polybar`) has not been run.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-03 | Initial entry | deltaog-117 |

---

### Wayland layer-shell: the layer map places, one list orders, the keyboard follows the surface that asks

**Date:** 2026-11-03
**Status:** Confirmed

#### Context / Background

Bars, launchers and wallpapers on Wayland are not windows: their client says where it wants to sit and how much space to keep clear, and the compositor places it. The core must hear only of the space.

#### Decision & Rationale

**Chosen Option:** use Smithay's layer map for placing and for the free zone, and turn that zone into insets for the core. It was the shortest correct path; writing the arrangement rules again would have copied behaviour that is easy to get wrong (margins, anchors, exclusive zones).

#### Implementation Notes

- **One list of layers, front to back,** now used for drawing, pointer hit-testing and frame callbacks: menus, overlay, fullscreen windows, top, other windows, bottom, background. A fullscreen window is above the top layer, so it hides bars, as on other compositors, but not an overlay.
- **Arrange before the first configure.** The first commit carries the size the client asked for, so the layer map arranges first and the initial configure follows. Sending it first gave the client a size from before it had asked.
- **From the zone to insets:** the zone is in the output's own coordinates, so the insets are its distances from the output's edges. The output's size is worked out the way Smithay does it, so the two agree after scaling.
- **Keyboard:** the core's focus is remembered as the window focus. A surface with exclusive interactivity on the top or overlay layer holds the keyboard whatever the core says; one that asks on demand takes it when clicked, until the pointer enters a window. Without the last rule, a clicked bar would keep the keyboard for ever, since the core sends no focus change when the pointer re-enters the same window.
- **Outputs:** a surface on an output that goes away is asked to close, as the protocol wants; the client can open another on a remaining output. A request with no output at all is closed at once.
- **Popups of a bar** are tracked and given the geometry their positioner describes, without keeping them on the output.
- **Fullscreen:** `xdg_toplevel` requests become events, also before the first commit (remembered until the window is announced). The state is set in the pending state when the scene says so and sent with the configure that sizes the window; windows whose state changed with no resize get a configure after all the changes.
- **Xwayland:** requests by `_NET_WM_STATE` message reach the core. A program that sets the state before mapping is not seen as asking: Smithay keeps its own record of the state and never reads that property, so the check I first wrote there was always false and was removed.
- **Test pitfall:** a test that discarded the events returned by `connect` also discarded the output report, so the engine had no screen and the scene was empty. The fixture helpers return them for that reason.
- **Test flake, found and not fixed:** the Xwayland tests fail in about one run in four (3 of 12 on the previous commit) at `Fixture::start` with `Connection refused`. It is listed in the roadmap.
- **Not done:** keyboard focus rules beyond exclusive and clicked, a surface that names no output goes to the first one, not the focused one; session lock.
- **Not verified:** a real `waybar`, `fuzzel` or `swaybg` has not been run.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-03 | Initial entry | deltaog-117 |

---

### Xwayland display numbers: the library never reuses one within a process

**Date:** 2026-11-04
**Status:** Confirmed

#### Context / Background

The Xwayland integration tests failed in about one run in eight at `Fixture::start` with `Connection refused`, and a run could lose two tests. Smithay picks the lowest free display number when no number is given, and the tests start many Xwayland instances in one process at once.

#### Options Considered

| COA | Approach | Advantages | Disadvantages | Difficulty |
|---|---|---|---|---|
| A | Retry the start in the test fixture | Smallest change, no library edit | The clash stays and is hidden; slower on a flake | S |
| B | The library never reuses a display number in a process | Removes the clash at its source; measured 0 failures in 100 runs | The number is no longer always the lowest; narrow cross-process case | M |
| C | Serialise the tests and wait for Xwayland to exit | Library behaviour unchanged | Slower; relies on scanning `/proc`; the hazard stays | M |

#### Decision & Rationale

**Chosen Option:** B. It is the only option that removes the clash instead of tolerating it, and the fix sits in the one function that starts Xwayland.

#### Implementation Notes

- **What was seen when it failed:** the socket file and the lock file of the display were gone from `/tmp` while Xwayland was alive and listening on both of its sockets. The same display number was created and removed several times within a few milliseconds. Each instance cleans up its lock and socket file by path, so one instance can delete the files of another that took the number after it. Which deletion starts the chain was not traced: no tracing tool is installed here.
- **The fix:** `Displays`, a process-wide counter, hands each start a candidate number and a start whose number is taken moves to the next. The first candidate is 0, so a lone Xwayland still gets the lowest free number. The counter wraps inside 256 numbers, so reuse becomes possible again only after that many starts in one process.
- **How it was checked:** a throwaway version of the fix gave 0 failures in 100 runs of the Xwayland tests, against about 1 in 8 before; the real one gave 0 in 100 again.
- **Not covered:** a new process that starts while the previous process's Xwayland is still dying can still meet the same number. That is far narrower than the in-process case and was not seen.
- **Leftover files:** runs that end without dropping the lock leave `/tmp/.X<n>-lock` files. A later start treats them as stale and takes the number. They are harmless and were left alone, since the live session's own lock is among them.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-04 | Initial entry | deltaog-117 |

---

### Fullscreen at start under Xwayland: a second connection reads the state at the map request

**Date:** 2026-11-04
**Status:** Confirmed

#### Context / Background

A program that sets `_NET_WM_STATE` fullscreen before it maps was not seen asking, because Smithay's window manager treats that property as its own to write and never reads it (its source says so).

#### Decision & Rationale

**Chosen Option:** the backend opens its own connection to Xwayland once it is ready (`StateReader`) and reads the property when a window asks to be mapped. If the state includes fullscreen, the window is remembered, and `FullscreenRequested` is sent right after its `WindowOpened`, so the core knows the window when it is asked. Only this approach was credible: Smithay keeps its own connection private, and sharing its socket would interleave two protocols on one stream.

#### Implementation Notes

- **Blocking read:** the read waits for Xwayland's answer. That is not a new kind of wait; Smithay's own window manager already asks Xwayland about each window it maps, on the same thread.
- **Dependency:** `x11rb` 0.14 moves from the dev-dependencies to the dependencies of `lutece-wayland`. It was already in the lockfile. Smithay's own re-export is 0.13 and sits behind a feature this crate does not use.
- **Failure:** if the connection cannot be made, Xwayland is reported as failed through the same channel as a failed window manager start, instead of quietly losing the feature.
- **Tests:** a window that sets the state before it maps covers the band a bar leaves, and the request arrives after the window opens; a plain window makes no request. With the read switched off the first test times out, so it tests the feature.
- **Not verified:** a real video player started fullscreen has not been run.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-04 | Initial entry | deltaog-117 |

---

### Override-redirect bars on X11: found when they map, kept if they have a strut

**Date:** 2026-11-04
**Status:** Confirmed

#### Context / Background

A bar that opts out of management (override-redirect) is never redirected to the window manager, so it sends no map request and was not tracked. Bars such as `polybar` can be configured this way and still publish struts.

#### Decision & Rationale

**Chosen Option:** the backend already hears every window mapped under the root, so a `MapNotify` of an override-redirect window reads its struts and feeds the existing bar tracking. Only this approach was credible: guessing a reserve from an unmanaged window's position would take menus and tooltips at a screen edge for bars, and a caller that knows a fixed bar size can already send `Event::ReservedChanged` itself.

#### Implementation Notes

- **A bar is a window with a strut.** For an unmanaged window the type `_NET_WM_WINDOW_TYPE_DOCK` makes no difference, since it is never managed and a dock with no strut reserves nothing. `Strut::reserves` says whether any edge has depth.
- **Menus and tooltips pass through the same path** and are dropped at once because they have no strut. The cost is two property reads for each such window that maps.
- **Struts set after the window maps** are caught: the backend asks to be told of property changes on every unmanaged window it looks at, and a strut change on a window not yet kept re-checks it.
- **Leaving:** unmapping or destroying the window uses the bar code that already existed. A window that was already showing at start is checked at start, as managed bars are.
- **Limit that stays:** a bar that publishes no strut (plain `dzen`) still reserves nothing; there is nothing to read. The roadmap's earlier wording promised more than this can do.
- **Tests:** five integration tests on a private `Xvfb` (reserves, no strut reserves nothing, strut set after mapping, already showing at start, closing gives the space back) and one unit test.
- **Test pitfall, again:** targeted runs such as `cargo test -p lutece-x11 bars` used the example window manager from the day before, so the new tests failed on old code. Building the example first fixed it. `scripts/check` runs the whole workspace, which does rebuild it.
- **Formatting pitfall:** the post-edit formatting hook sorts imports differently from `cargo fmt --check`, so the check failed on formatting alone until `cargo fmt --all` was run.
- **Not verified:** a real `polybar` has not been run.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-04 | Initial entry | deltaog-117 |

---

### X11 stacking and the active window under Xwayland; Xwayland on demand deferred

**Date:** 2026-11-06
**Status:** Confirmed

#### Context / Background

Two roadmap items were taken together: EWMH stacking and active-window hints for X11 windows under Xwayland, and starting Xwayland only when the first X11 program connects. Smithay's window manager already writes `_NET_CLIENT_LIST`, and `_NET_WM_STATE_FOCUSED` through `set_activated`. Nothing kept `_NET_CLIENT_LIST_STACKING` in step with the scene, and `_NET_ACTIVE_WINDOW` was wrong.

#### Options Considered

On demand:

| COA | Approach | Advantages | Disadvantages | Difficulty |
|---|---|---|---|---|
| A | Bind the X sockets ourselves, start Xwayland on the first connection | Real on-demand | Copies Smithay's private lock and socket code and needs our own client data | H |
| B | Start at the first spawn effect | Small; `DISPLAY` is known at spawn | A window manager's first spawn is nearly always a terminal, so it saves almost nothing | S |
| C | Keep the eager start; leave the item open | No risk to the Xwayland tests | The item stays open | S |

Stacking and active window:

| COA | Approach | Advantages | Disadvantages | Difficulty |
|---|---|---|---|---|
| A | Restack through Smithay after each scene change; write `_NET_ACTIVE_WINDOW` on the existing second connection | Smithay keeps the list properties; one extra writer | Must not fight Smithay over one property | M |
| B | Fix only the active window | Smallest | Stacking list stays wrong | S |
| C | Write every hint ourselves | One owner | Races with Smithay's writes | H |

#### Decision & Rationale

**Chosen Options:** on demand C, stacking and active window A. Real on-demand would mean maintaining a copy of Smithay's socket and lock handling, a risk to the Xwayland tests that had just reached 100 clean runs, for a memory saving nobody has asked for. Option A for the hints fixes both gaps and leaves the properties Smithay does well to Smithay.

#### Implementation Notes

- **Active window:** Smithay selects focus events on the root only, so the `FocusIn` and `FocusOut` it turns into `_NET_ACTIVE_WINDOW` carry the root or the window that lost the focus. My first guess, a stale old window after focus moved to a Wayland surface, was too narrow: the property never named the window that had the focus. The backend now writes the X window of the keyboard focus (0 for a Wayland surface) after each scene and each pump. It listens for property changes on the root and rereads the property only when told, so it settles although it hears its own writes.
- **Stacking:** `update_stacking_order_downwards` takes the order from the top down; I first passed it bottom to top and the test caught that. The core keeps a fullscreen window's place in the scene order and only flags it, so there is no `Restack` for it: the order sent to X puts fullscreen windows last, and is redone when either a `Restack` or a fullscreen change arrives.
- **Scene order is layout order for tiles:** the first test assumed the older window starts below. The scene's order for tiled windows is not map order, and since tiles do not overlap that is harmless; the test now reads the order X has and checks it changes as the scene says.
- **Tests:** two integration tests with a real Xwayland (the hint follows the focus and clears for a Wayland window, and stays cleared over 20 pumps; stacking follows fullscreen on and off). Both failed before the change. The 14 Xwayland tests ran 15 times with no failure.
- **Not verified:** a real pager, taskbar or `xprop -root` watching the properties.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-06 | Initial entry | deltaog-117 |

### Window style and animation in the scene

**Date:** 2026-11-08
**Status:** Confirmed

#### Context / Background

The window manager built on this library needs windows that look and move well: rounded corners, borders, opacity and animated moves. `Scene` only said where windows are. Two decisions were taken together because both change what the core hands to a backend: where a window's look lives, and how its animation is described.

#### Options Considered

Where the style lives:

| COA | Approach | Advantages | Disadvantages | Difficulty |
|---|---|---|---|---|
| A | Inside each `Placement` | One record per window | Breaks every place that builds a placement | M |
| B | A checked list on `Scene`, like fullscreen | Non-breaking, replayable, validated | Two lookups per window | M |
| C | A table outside the core | No contract change | Not logged or replayed | S |

Animation:

| COA | Approach | Advantages | Disadvantages | Difficulty |
|---|---|---|---|---|
| A | The renderer tweens with host settings | Simple | One curve unless overridden | M |
| B | The spec travels in the scene | Per-window control, logged | Larger contract | H |

#### Decision & Rationale

**Chosen Options:** style B, animation B. The spec is part of `Style` as a `Transition`, so one list and one change kind carry both. I had recommended A for animation; B was chosen for per-window control by plugins and for keeping animation in the log.

#### Implementation Notes

- **Eq stays:** `Scene` derives `Eq`, so opacity is `u8`, durations are `u16` milliseconds and curves are a fixed enum. No floats.
- **Canonical form:** a style equal to the default is not stored, so two scenes that look the same compare equal however they were built. The default is opaque, because an opacity of zero would hide a window with no style.
- **Order in the diff:** `Change::Style` comes after fullscreen and before shows and moves, so a backend knows the curve before it starts the move. A style dropped back to the default is announced as the default so the backend forgets the old one. A window that leaves gets only `Hide`.
- **Engine:** `Event::StyleChanged` is ignored for an unknown window, so nothing lingers; a closed window loses its style; a window on a hidden workspace keeps its style and gets it back on return. The scene lists styles only for windows it shows.
- **Tests:** the diff property test now generates styles and its stand-in backend applies `Change::Style`, so applying the diff still reaches the new scene; the engine event-stream property test includes style events, and four tests cover the event, unknown and closed windows, and hidden windows. `scripts/check` passes.
- **Not done:** no backend draws style or runs transitions yet; both backends skip `Change::Style`. That is the next roadmap item.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### Action requests in the core

- **What:** `Event::ActionRequested(Action)`. The engine handles it by calling the same private `run` a bound key reaches, so a request and the key give the same effects and the same state.
- **Why:** actions could only be reached through the keymap, so a control tool or plugin of `window-manager` could not drive the engine, and anything that did would not be an event a replay could repeat. Chosen over hidden key bindings (the replay would depend on a table nobody sees).
- **Order of dependencies:** `shared::contracts::event` now imports `input::Action`, while `input` already imports contract types. Rust allows it and the action list is small, so it stays; if it grows it moves to `shared::contracts`.
- **Tests:** a request gives the same effects and the same engine as the bound key; it works with no binding; `Quit` and `Close` give their effects; a tag nobody has changes nothing.
- **Not done:** no property test generates `ActionRequested` in the engine event-stream test, only the four cases above.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### External layout, and the nested window seen for the first time

- **What:** `Layout::External`, `Effect::LayoutRequested { tag, area, windows, focus, serial }`, `Event::LayoutProvided { serial, placements }`, `Action::ToggleExternalLayout`, `Engine::pending_layout_requests`. Chosen over a plugin that pushes rectangles on its own: with request and reply a replay can reproduce what the screen showed, and the plugin cannot leave stale rectangles after the windows change.
- **No timeout in the core:** the engine never waits. A workspace with the external layout is shown with the default layout until an answer fits its windows and area; the scene is derived from state as before. A late answer is still used if it still fits, a stale or wrong one is ignored. So the host needs no timer event, and a replayed session ends in the same state because answers are events. The earlier idea of a tick event was dropped.
- **Answers are checked:** a serial that is not outstanding is ignored; the placements must name each requested window exactly once and keep every rectangle inside the area. A refused answer is remembered so the same question is not asked again in a loop; it is asked again when the windows or the area change.
- **Requests are made once:** when the windows or area change, not on every event. A plugin that connects later (or restarts) would never be asked, so `pending_layout_requests` lists what is unanswered for a host to hand over on connect. Found by the first end-to-end test with a real plugin, which waited ten seconds for an answer that was never requested again.
- **Not in the NextLayout cycle:** the external layout is entered and left with `ToggleExternalLayout`, because cycling onto it with no plugin would show the default layout twice in a row.
- **Tests:** 12 new, among them: effects and state equal to the default layout before any answer, an answer used, an old serial ignored, a closing window making the answer stale, wrong and repeated answers refused without a loop, no request for empty or floating-only workspaces, a replayed session with answers equal to the live engine, and a property test that answers every request over random events and checks the scene is always buildable.
- **Found by looking:** with `wm-host` running a layout plugin and a GTK bar, the nested window was shown upside down (bar at the bottom, mirrored text). The winit framebuffer has its origin at the bottom; the frame is now drawn with `Transform::Flipped180`. The cause is read from Smithay's winit examples, and the fix was confirmed by a second screenshot. No automated test covers it: it needs a host window.
- **Not done:** the cursor and pointer input were not checked after the flip, since only windows and the bar were looked at; no screenshot was taken at a scale other than 1.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### Window style drawn, animated moves, and the manage hook

- **Style (COA 1A):** corners are cut by a texture shader that replaces the renderer's default for one window's draw calls (`override_default_tex_program`, cleared right after so it cannot leak to the next element); the border is a ring drawn by a pixel shader around the window, grown by its width; opacity is the surface alpha. All three are tied to the GLES renderer, so the render path is no longer generic over the renderer. Chosen over square borders with no radius (a dead field) and an offscreen pass per window (an extra pass nothing needs until blur).
- **Animation (COA 2A):** the core still only says where a window starts and ends. The backend keeps an `Animation` (old rectangle, new rectangle, start, transition) per window and draws the window at the curve's blend; the client was configured at the final size at once, so its surface is scaled about the drawn origin to fit. A move that starts mid-animation begins from where the window is drawn, so a window never jumps. A window that appears is not animated in. The curves, the blend and the clamping are pure and property-tested (every curve runs 0 to 1 without going back; the drawn rectangle stays between the two and never collapses).
- **Found by looking, twice:** the first styled screenshot showed the two right-hand windows empty except a thin strip: the corner shader measured y from the wrong end, so a window was cut by its own mirror image (the full-height master was symmetric and looked fine). Fixed by turning gl_FragCoord's y over for the flipped frame; the next screenshot showed all three windows whole, with rounded corners and borders, and a mid-move screenshot showed the windows scaled and overlapping on their way.
- **Not verified:** the shader was only seen at scale 1 and at one radius; the opacity (0.92) is not distinguishable by eye on a dark terminal; the pointer was moved over the nested window once with the host's own tools and the focus did not change, but the nested window was by then covered by another window (see below), so pointer focus on a live window was not confirmed. The pointer is covered by the scripted-client tests that run in the check.
- **Known fault, found while looking:** when the host's own compositor puts another window over the nested one, the buffer swap waits for a frame callback that is never sent, so the loop stops: events, commands and plugin supervision wait until the window is visible again, and the host shows "Application Not Responding" dialogs (two are still open on the screen this was tried on, and need a click on Wait or Terminate). It is on the roadmap. It is also a reminder that the nested backend is for development.
- **Manage hook (COA 5A):** `Effect::ManageRequested { serial, id, info, tag }` and `Event::ManageDecided { serial, decision }` with a `ManageDecision` whose every part is optional. The window is placed by the rules first; a decision then moves it with `StackSet::decide` (remove and insert again for a workspace change, with the float state kept unless the decision sets it). Only the first answer to a serial counts, a window that closes takes its pending request with it, and a tag nobody has changes nothing. Pending requests are kept for a plugin that connects late, as for layouts.
- **Tests:** 12 in the engine (the hook only when enabled, a decision applied, parts left out left alone, a decision that does not apply, a closed window, a replay with decisions equal to the live engine) and 9 for the drawing helpers and animation.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### Nested window stall when covered

- **Cause:** Smithay's winit backend never sets the swap interval, so Mesa's Wayland default of one applies and `eglSwapBuffers` waits for a frame callback. A host that covers the window sends none. Smithay 0.7 also drops winit's `Occluded` event, so "draw only while visible" could not be done.
- **COA 1A:** set the interval to zero with one `eglSwapInterval` call on the first draw, with `unsafe_code` relaxed from `forbid` to `deny` for `lutece-wayland` alone and allowed on that one function with a SAFETY note. Rejected: `vblank_mode=0` in the launch scripts (stall returns without it), a swap thread (fragile), a swap-time guard (still freezes once).
- **Rule change you approved:** `lutece-wayland` has its own `[lints.rust]` table instead of the workspace one; the other crates still forbid `unsafe`.
- **Tested:** `scripts/check` passes, and the nested example opens, draws and runs for six seconds with no error. The window was not covered on purpose (the last time, that froze the real session), so the fix itself is **not** yet seen working. Drawing now returns at once, so the loop is paced only by the 8 ms wait slice.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### Reconfiguring a running engine

- **Why:** the window manager's reload must change keys and the manage hook without rebuilding the engine, which would lose its windows. The engine could only be configured when built.
- **Choice (reload COA B):** one method, `reconfigure(keymap, manage_hook)`, and nothing more. Workspaces, layouts and rules are state that windows live in, so changing them stays with whoever builds a new engine. It is not an event: the log records events, and the window manager records the reload itself.
- **Tests:** four integration tests (the new keys work and the old stop, the hook on and off, an open request stays pending) and two property tests (a reload never moves a window or the scene; reloading the same configuration leaves the engine equal).

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### Properties, custom commands and richer events

- **Why:** the window manager's plugins need to keep a little state of their own in the core, to be asked for by name, and to hear that a window's title changed or that the pointer entered it. The window manager is the only user so far.
- **Properties (no COA, the roadmap had chosen):** a string map per window or workspace, changed by one event, so a recorded session and a replay hold the same ones. Keys, values and the count per target are bounded where they are built, so a plugin cannot grow the core or a log without limit. An empty target has no entry in the map, so two engines that reached the same properties by different sets and removals compare equal (a property test checks this).
- **Custom commands:** an action that the core only reports, as an effect. I chose this over a core that stores a command's actions: the core stays free of plugin policy, and a plugin's answers are ordinary action requests, which are events, so a replay does not need the plugin to reach the same state. The cost is that a key bound to a command does nothing when the plugin is not there.
- **Pointer entry:** a separate event, which the core handles as a focus request. Wayland now sends it instead of `FocusRequested`; X11 was left as it was, so the two backends differ until X11 is done. I did not send both on Wayland: a host would then see every entry twice.
- **Title and class:** Smithay calls its title and app id hooks when each request arrives, not when the surface commits, so a client that sets both makes two reports. I first wrote a comment saying it makes one; the Smithay source showed otherwise. The backend remembers what the core was last told and sends the event only when the info differs, with the complete info each time. The X11 backend does not report these yet.
- **Tests:** the core's property tests (the limit, no empty entries, a replay ends equal), unit tests for each event and the action, and Wayland tests with a real client for the pointer event and for a changed title, a changed class and a repeated value.
- **Not done:** X11 pointer, title and class; `Engine` keeps no title or class, so the core cannot answer "what is this window called" (a plugin keeps its own from the events).

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### 2026-11-08 – X11 events and what a host needs from the X11 backend

- **Scope:** the X11 half of the richer events, and three additions the window manager host asked for. No COA: one credible approach each.
- **Events:** managed windows now subscribe to property changes. The backend keeps the last description it sent per window and reports only a real change; it reads the properties when it handles the event, so changes made before it looks merge into one report. Pointer entry is `PointerEntered`, as on Wayland.
- **Host needs:** `next_events_timeout` polls the connection with `rustix` (already in the tree through x11rb, now a direct dependency); `connect_to` takes a display name so tests run the backend in-process; `set_grabs` replaces the grab set, because `grab_keys` only adds and a reload or key mode would leave stale keys grabbed.
- **Tests:** unit tests for the description bookkeeping and the atoms; in-process tests on Xvfb for title changes, pointer entry, the timeout and `set_grabs`. `scripts/check` passes with `LUTECE_REQUIRE_X11=1`.
- **Not done:** nothing was looked at on a real X session; style is still ignored on X11.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### 2026-11-08 – Naming the rule that matched

- **Scope:** one accessor the window manager asked for. No COA: the alternative, a copy of the matching in the host, would drift from the engine the first time `Match` gains a kind.
- **What:** `Rules::matching(&WindowInfo)` returns the first matching rule with its place, and `ManageRule` and `Match` can be read and printed. `outcome_for` is unchanged and a property test checks the two never disagree.
- **Not done:** `Rules` still cannot be listed or edited; only the one question is answered.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-08 | Initial entry | deltaog-117 |

### 2026-11-10 – Title bar and shadow, a read-only screen, a snapshot of the engine, a DRM test pattern

- **Scope:** the parts of four window-manager roadmap items that are this library's, with COAs chosen there: 2-C (modeset and render behind a feature flag), 3-A (a read-only mode and a way to show a past scene), 4-A (title bar and shadow in `Style`) and 5-A (the engine serialisable).
- **Style:** `TitleBar` and `Shadow` joined `Style` as two more small copy types; `Style` is `non_exhaustive`, so nothing else changed. They are drawn outside the area the core gave the window, like the border: a window's own size is never changed by them. The band is a filled bar rounded at all four ends (a bar rounded only on top depends on which way the pixel shader's y axis runs, which could not be checked without a screen). The shadow is a separate pixel shader that finds its place from `gl_FragCoord` with the same `fb_height` and `flip` the window shader uses, fades with `smoothstep` over `blur` pixels and is cut away under the window. All three shaders pass `glslangValidator` as GLSL ES 1.00.
- **Read-only screen:** `Scene::retaining` (property-free, two tests and a doctest) cuts a past scene to the windows that still exist; `NestedBackend::set_read_only` drops input in `next_events`. A release of a key or button is still passed so nothing stays down in a client; a press, motion or scroll is dropped before the core or a client sees it.
- **Snapshot:** `Engine` keeps its state in private types, so `lutece::snapshot` is a tree of public plain types and the conversions live next to the private types they read (child modules of `workspace` and `stack`, and `external`). Restoring checks what the parts promise each other: a window on one workspace, a focus inside its stack, floating windows inside the stack, the fullscreen window being the focused one, the template's workspaces in the template's order, styles and properties only for managed windows, and nothing stored that the engine would not store (a default style, an empty reservation). Lists kept in hash tables are written sorted. The configuration is not in a snapshot, so a snapshot cannot change what keys do. A property test at 5 000 cases found one mistake of mine (outputs repeated as the backend reported them).
- **DRM:** the module is behind a feature, since nested use never needs it; one `unsafe` call each for the EGL display and the renderer, in the one function that already is allowed it for the swap interval. It compiled against Smithay 0.7 as written.
- **Not done:** nothing here was seen on a screen. `Engine::restore` takes `Engine`'s state only, so a window manager that wants a snapshot of its own state must save that too. The session backend with clients and input is not built.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-10 | Initial entry | deltaog-117 |

### 2026-11-11 – Title text, input held back on X11, the first hardware session

- **Scope:** the parts of three window-manager roadmap items that are this library's, with the COAs chosen there: 4-A (a bundled font drawn with `ab_glyph`), 5-A (grab the input and move the windows) and 3-A (one screen with clients, input and a safe quit).
- **Title text:** the text is rasterized on the CPU and given to Smithay as a `MemoryRenderBuffer`, drawn in front of the band. Its size, padding and colour come from the bar (60% of its height, half a height of padding, light or dark by brightness) instead of new `Style` fields: a field would have changed the wire format, the snapshot and the window manager for something the bar already decides. The picture is cached by title, bar size and colour and the cache is emptied at 256 entries, because a clock in a terminal's title makes a new picture every second. The font is JetBrains Mono Regular (273 KB, OFL) in `assets/` with its licence, since the roadmap left the choice open and the machine has it. A control character in a title is turned into a space, so a newline cannot break the bar.
- **X11 rewind:** nothing else was missing, since the host already hands the screen a retained scene, which X11 shows by moving the windows. What was missing was the input. `XGrabKeyboard` and `XGrabPointer` on the root with no event mask make the server discard what the devices send; a failure of the second releases the first, so it is all or nothing. The server frees the grabs with the connection, so a crashed host leaves a working screen; a hung one does not until it is killed, and that is the cost of this choice. A test with a second client that tries to grab says whether the hold is on, with no timing in it.
- **DRM session:** built on what `show_test_pattern` already did (its screen choice, renderer and error naming are shared, with three visibility changes) and on the `input` module, which was written for libinput from the start. The pieces that cannot be tried here are kept as small as possible: the two chords are a pure state machine with property tests, the pointer is an ASCII arrow with tests on its pixels. Frames are drawn with `Transform::Normal` and `flipped = false`, the opposite of the nested window: a dmabuf is scanned out from its first row and a window surface from its last, and the corner and shadow shaders measure from the top with `flip`. That is reasoning from the shaders, not something seen, and it is the first thing to look at on a screen.
- **Not done:** a second screen, a screen plugged in or out, each output's scale, libinput settings, drawing only on change. Nothing here was seen on a screen. The session was compiled against Smithay 0.7 and its pure parts are tested; `DrmBackend::open` was not run, because from a graphical session it could take the screen from the compositor that is showing it.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-11 | Initial entry | deltaog-117 |

---

### 2026-11-13 – Input settings, drawing on change, and a swap interval that is not fatal

- **Scope:** the lutece half of the window manager's roadmap items 1 to 3 for this cycle: a start-up failure found when the nested host was first run on Mesa 26, and the settings and the redraw rule of the hardware session.
- **Swap interval:** `eglSwapInterval` answered `EGL_BAD_SURFACE` (`0x300d`) on the first frame on an AMD machine with Mesa 26.2, and `render` returned the error, so the host stopped at start-up. The call only protects against the covered-window stall, so a refusal is not an error now: `set_swap_interval` returns a bool, and the nested backend asks again each frame until it works. A driver that never accepts it loses that protection; whether it ever stalls there was not watched. The cause of the bad surface on the first frame is a guess (the surface not yet valid for the call), not something I checked.
- **Decision (COA C):** `DrmBackend` draws a frame when its `frame_due` rule says so: always in `Redraw::Always`, and in `Redraw::OnChange` when something may have changed or a second has passed. The signals are a flag in the compositor state set by every client commit and cursor-image change, by `apply_scene` only when its diff is not empty (it runs on every turn of the host's loop), by `apply_effects` when it gets any, by input, by events, by an outline change and by the terminal coming back, and `Windows::animating` for a glide in progress. Rejected: only the flag (a forgotten source stays stale for ever), and smithay's damage tracker (every shader element would have to report its own damage, which cannot be checked without a screen).
- **Settings:** `InputSettings` is the only way to hold a repeat delay (100 to 5000 ms), a rate (0 to 200) or a pointer speed (-1 to 1), so applying one never checks. The repeat constants moved from the compositor into the settings module, so the default is the old behaviour by construction. Device settings go through libinput on each `DeviceAdded` and again on a change; a feature a device lacks is skipped and a refusal is dropped.
- **Tests:** property tests for the settings and for `frame_due`, and three integration tests with a real client for the repeat (now, after a change, on a later connection). Format, clippy with warnings denied and every test pass with the `drm` feature on and off.
- **Not done:** the damage flag is not unit-tested (its state needs a display), and nothing was seen on a screen: the DRM session has still not run, so neither the redraw nor the libinput settings were exercised on hardware.

#### Review / Update Log

| Date | Update | Author |
|------|--------|--------|
| 2026-11-13 | Initial entry | deltaog-117 |
