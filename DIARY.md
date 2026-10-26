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
