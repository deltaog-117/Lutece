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
