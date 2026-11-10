//! A plain-data copy of what an [`Engine`](crate::Engine) knows, to keep a moment of a
//! session and bring it back.
//!
//! Every type here is made of numbers, text and lists, with public fields, so any format
//! can write it. None of them is trusted when it comes back: [`Engine::restore`] builds the
//! engine again only through the core's own checked constructors and then checks what ties
//! the parts together (a window on one workspace only, a focus inside its stack), so a
//! damaged or invented snapshot is refused and can never become an engine that breaks the
//! rules the core keeps.
//!
//! What the configuration decides (the keys, the rules, whether the manage hook is on, the
//! workspace names and their order) is not in a snapshot: it is taken from the engine the
//! snapshot is restored onto, so a snapshot cannot change the configuration.
//!
//! Lists that the engine keeps in hash tables are written sorted, so the same state gives the
//! same snapshot.
//!
//! [`Engine::restore`]: crate::Engine::restore

/// The layout of the snapshot, raised when a field changes meaning or is added. A snapshot
/// with another version is refused, not guessed at.
pub const VERSION: u32 = 1;

/// What an engine knows about the session at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Snapshot {
    /// The layout of this snapshot, [`VERSION`].
    pub version: u32,
    /// The screens, in the order the backend reported their outputs, each with the
    /// workspace it shows.
    pub screens: Vec<ScreenState>,
    /// Which of the screens has the focus, as a position in `screens`.
    pub current: usize,
    /// The workspaces no screen shows.
    pub hidden: Vec<WorkspaceState>,
    /// The outputs the backend reported.
    pub outputs: Vec<OutputState>,
    /// What bars reserve on each output, for the outputs that have any.
    pub reserved: Vec<ReservedState>,
    /// The style of each window that differs from the default.
    pub styles: Vec<StyleEntry>,
    /// The properties set on windows and workspaces, for the targets that have any.
    pub properties: Vec<PropertiesState>,
    /// The external layout of each workspace that has used it.
    pub external: Vec<SlotState>,
    /// The manage requests nobody has answered.
    pub manage_pending: Vec<PendingState>,
    /// The serial of the next layout request.
    pub next_serial: u64,
}

/// A workspace on display at one output.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScreenState {
    /// The output.
    pub output: u32,
    /// The workspace it shows.
    pub workspace: WorkspaceState,
}

/// One workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WorkspaceState {
    /// Its tag.
    pub tag: String,
    /// Where the tag sits in the configured order.
    pub rank: usize,
    /// Its windows, or `None` when it has none.
    pub stack: Option<StackState>,
    /// How it arranges them.
    pub layout: LayoutState,
    /// The master ratio a layout with a master column gets back, in thousandths.
    pub tile_ratio: u16,
    /// The windows the layout leaves alone, by window.
    pub floating: Vec<FloatState>,
    /// The window that fills the screen, if any.
    pub fullscreen: Option<u64>,
}

/// The windows of a workspace, split around the focused one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StackState {
    /// The windows before the focus; the one nearest the focus is last.
    pub up: Vec<u64>,
    /// The focused window.
    pub focus: u64,
    /// The windows after the focus; the one nearest the focus is first.
    pub down: Vec<u64>,
}

/// How a workspace arranges its windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LayoutState {
    /// The tile layout, with its master ratio in thousandths.
    Tile(u16),
    /// The monocle layout.
    Monocle,
    /// The grid layout.
    Grid,
    /// The centered-master layout, with its master ratio in thousandths.
    CenteredMaster(u16),
    /// The layout a plugin supplies.
    External,
}

/// How a floating window differs from the default rectangle, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatState {
    /// The window.
    pub window: u64,
    /// How far it was moved to the right.
    pub dx: i32,
    /// How far it was moved down.
    pub dy: i32,
    /// How much wider it was made.
    pub dw: i32,
    /// How much taller it was made.
    pub dh: i32,
}

/// A rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RectState {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// One screen and the area it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputState {
    /// The output.
    pub id: u32,
    /// The area it covers.
    pub rect: RectState,
}

/// What a bar reserves on one output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReservedState {
    /// The output.
    pub output: u32,
    /// Taken from the left edge.
    pub left: u32,
    /// Taken from the top edge.
    pub top: u32,
    /// Taken from the right edge.
    pub right: u32,
    /// Taken from the bottom edge.
    pub bottom: u32,
}

/// The style of one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StyleEntry {
    /// The window.
    pub window: u64,
    /// Its style.
    pub style: StyleState,
}

/// A colour with transparency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ColorState {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Opacity.
    pub a: u8,
}

/// How an animation moves between two places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CurveState {
    /// Constant speed.
    Linear,
    /// Starts slowly.
    EaseIn,
    /// Ends slowly.
    EaseOut,
    /// Slow at both ends.
    EaseInOut,
}

/// A window's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StyleState {
    /// Corner radius in pixels.
    pub radius: u16,
    /// Border width in pixels.
    pub border_width: u16,
    /// Border colour.
    pub border_color: ColorState,
    /// Opacity, 255 being opaque.
    pub opacity: u8,
    /// The curve of the animation.
    pub curve: CurveState,
    /// The duration of the animation in milliseconds.
    pub duration_ms: u16,
    /// Height of the band above the window.
    pub title_bar_height: u16,
    /// Colour of the band above the window.
    pub title_bar_color: ColorState,
    /// How far the shadow fades out.
    pub shadow_blur: u16,
    /// How far the shadow is moved to the right.
    pub shadow_dx: i16,
    /// How far the shadow is moved down.
    pub shadow_dy: i16,
    /// Colour of the shadow.
    pub shadow_color: ColorState,
}

/// What properties belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TargetState {
    /// A window.
    Window(u64),
    /// A workspace, by tag.
    Workspace(String),
}

/// The properties of one target, by key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertiesState {
    /// What they belong to.
    pub target: TargetState,
    /// The keys and values, sorted by key.
    pub entries: Vec<PropertyEntry>,
}

/// One property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertyEntry {
    /// The key.
    pub key: String,
    /// The value.
    pub value: String,
}

/// What an external layout answer is about: the area and the tiled windows in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutKeyState {
    /// The area.
    pub area: RectState,
    /// The tiled windows, in reading order.
    pub windows: Vec<u64>,
}

/// One window's place, as a layout plugin gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlacementState {
    /// The window.
    pub window: u64,
    /// Where it goes.
    pub rect: RectState,
}

/// A layout request that was made and not answered.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutRequestState {
    /// Its serial.
    pub serial: u64,
    /// What it asked about.
    pub key: LayoutKeyState,
}

/// An answer that was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LayoutAnswerState {
    /// What it answers.
    pub key: LayoutKeyState,
    /// The places, bottom to top.
    pub placements: Vec<PlacementState>,
}

/// The state of the external layout of one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SlotState {
    /// The workspace.
    pub tag: String,
    /// The request that was made and not answered yet.
    pub pending: Option<LayoutRequestState>,
    /// The last answer that was accepted.
    pub answer: Option<LayoutAnswerState>,
    /// A key whose answer was refused.
    pub refused: Option<LayoutKeyState>,
}

/// A manage request nobody has answered.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingState {
    /// Its serial.
    pub serial: u64,
    /// The window.
    pub window: u64,
    /// The window's title.
    pub title: String,
    /// The window's class.
    pub class: String,
    /// The window it is a dialog for, if any.
    pub transient_for: Option<u64>,
}
