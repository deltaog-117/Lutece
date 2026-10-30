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
