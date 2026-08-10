#![allow(dead_code)]

/*
CDXC:GPUITerminalElement 2026-07-03:
P1c GPUI-composited terminal renderer over the P1b terminal model. One
`TerminalView` entity owns the model plus the render caches; `TerminalElement`
is a thin gpui::Element that reads them each frame. Replaces native Ghostty
child views for portability (P1e swaps it into the panes).

Frame flow: model wakeups arrive on background threads through the event sink,
get marshalled to the gpui foreground via an unbounded channel + entity task,
and each wakeup takes EXACTLY ONE snapshot immediately (row dirty flags are
relative to the previous snapshot, so skipping or coalescing snapshots would
lose that frame's diff). The latest snapshot is kept as the current frame;
prepaint never touches the model except on first layout and grid resize.

Caching: shaped rows are cached per viewport row and invalidated by the
snapshot's dirty info (`Clean` global → whole previous layout reused; row
dirty → that row reshaped). Background spans merge horizontally only, per
row, so a row's cache entry is self-contained and dirty tracking stays sound;
whole-frame vertical rect merging would couple rows and defeat the cache.

Column alignment: text runs are shaped with `force_width = cell_width`, which
snaps every base glyph to its cell. A Wide cell would receive only one forced
cell of advance inside a batch, so wide cells always flush the current batch
and paint as their own run positioned by grid column; spacer cells keep the
cells vector column-aligned and are skipped for text.

CDXC:GPUITerminalInput 2026-07-04 (P1d):
Input rides libghostty-vt's own encoders so encoding always matches the modes
the running program set. Key path: the platform delivers plain-alt text keys
and IME composition to the InputHandler (marked text renders, commits write
UTF-8 to the PTY); every other key hits the on_key_event listener and goes
through the vt key encoder (kitty/legacy/DECCKM synced from live terminal
state) — an encoded key is consumed, an un-encoded one (bare mods, unbound
cmd chords) propagates so app bindings still work. Mouse path: with a
tracking mode active (and shift not held, the xterm local-override), events
encode through the vt mouse encoder in DEVICE pixels matching the PTY cell
sizes; otherwise the view owns click-to-focus plus cell/word/line drag
selection in viewport coordinates. Wheel routes by terminal state: reported,
arrow keys (alt screen + mode 1007), or local scrollback viewport scrolling.
Copy trims per-row trailing whitespace and joins rows with newlines; paste
uses the vt paste encoder (bracketed mode + unsafe byte stripping).

Known follow-ups: selection is viewport-relative (drifts under output,
no soft-wrap join on copy), modifier-only kitty events aren't wired.

CDXC:GPUITerminalElementIntegration 2026-07-04 (P1e):
The view is the app-facing integration boundary: it emits TerminalViewEvent
(title/pwd readback after wakeups, bell, exit, cmd+click URL opens) instead
of letting pane code reach into the model, and it owns the parity features
the panes rely on: alt+drag rectangular selection, OSC 8 hyperlink hover
underline plus a conservative scheme-based URL scan of the hovered row
(libghostty-vt has no regex link detection), and a viewport-scoped search
(`set_search_needle`/`navigate_search`) whose matches recompute per
snapshot. pwd has no dedicated vt callback, so title/pwd re-read on every
wakeup and only changes are emitted.
*/

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

use futures::StreamExt as _;
use image::Frame;

use gpui::{
    AnyElement, App, BorderStyle, Bounds, BoxShadow, ClipboardItem, ContentMask, Context, Corners,
    CursorStyle, DevicePixels, DispatchPhase, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, ExternalPaths, FocusHandle, Focusable, Font, FontStyle,
    FontWeight, Global, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InteractiveElement,
    IntoElement, KeyContext, KeyDownEvent, KeyUpEvent, Keystroke, LayoutId, Modifiers,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, Render, RenderImage, Rgba, ScrollDelta, ScrollWheelEvent,
    ShapedLine, SharedString, Size, StrikethroughStyle, Style, Styled, TextAlign, TextRun,
    UTF16Selection, UnderlineStyle as GpuiUnderlineStyle, Window, canvas, div, fill, outline,
    point, prelude::FluentBuilder as _, px, size, svg,
};
use gpui_component::{
    native_menu::NativeMenu,
    tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement, Tooltip},
};

use crate::ghostty_vt::{
    VtCellWide, VtDirty, VtKey, VtKeyAction, VtKeyInput, VtMods, VtMouseAction, VtMouseButton,
    VtMouseInput, VtScrollViewport, VtScrollbar, ffi,
};
use crate::terminal_model::{
    Rgb, SnapshotCell, SnapshotRow, TerminalConfirmCloseBehavior, TerminalEvent, TerminalEventSink,
    TerminalExit, TerminalModel, TerminalPasteDiagnostic, TerminalSnapshot, TerminalSpawnConfig,
    TerminalTextRow, UnderlineStyle as CellUnderline, WheelRoute,
};

gpui::actions!(
    ghostex_terminal,
    [TerminalContextMenuCopy, TerminalContextMenuPaste]
);

pub(crate) const TERMINAL_KEY_CONTEXT: &str = "GhostexGpuiTerminal";
const TERMINAL_SCROLLBAR_HIDE_DELAY: Duration = Duration::from_secs(2);
const TERMINAL_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const TERMINAL_SCROLLBAR_THICKNESS: f32 = 2.0;
const TERMINAL_SCROLLBAR_MIN_KNOB_HEIGHT: f32 = 18.0;
const TERMINAL_SCROLL_BUTTON_SIZE: f32 = 28.125;
/// Shared edge inset for terminal overlay chrome: Agent Actions (top-right) and
/// scroll-to-top/bottom (bottom-right) both sit 13px in from the pane edge.
const TERMINAL_ACTION_BUTTON_EDGE_INSET: f32 = 13.0;
const TERMINAL_AGENT_ACTIONS_EDGE_INSET: f32 = TERMINAL_ACTION_BUTTON_EDGE_INSET;
/// Vertical gap between the cluster row and the expanded Agent Actions menu.
const TERMINAL_AGENT_ACTIONS_MENU_GAP: f32 = 13.0;
const TERMINAL_BUTTON_GAP: f32 = 0.0;
const TERMINAL_SCROLL_BUTTON_VISIBILITY_THRESHOLD: f32 = 200.0;
const TERMINAL_SCROLL_BUTTON_MIN_WIDTH: f32 = 80.0;
const TERMINAL_SCROLL_BUTTON_MIN_HEIGHT: f32 = 96.0;
const TERMINAL_PROMPT_EDITOR_ICON: &str = "titlebar/message-code.svg";
const TERMINAL_ATTACH_PATH_ICON: &str = "titlebar/paperclip.svg";
const TERMINAL_RENAME_ICON: &str = "titlebar/pencil.svg";
const TERMINAL_SLEEP_ICON: &str = "titlebar/moon.svg";
const TERMINAL_DELAYED_ACTIONS_ICON: &str = "titlebar/clock-check.svg";
const TERMINAL_FORK_ICON: &str = "titlebar/git-branch.svg";

fn temporary_terminal_control_labels(bytes: &[u8]) -> Vec<&'static str> {
    let mut controls = Vec::new();
    if bytes.windows(5).any(|window| window == b"\x1b[27u") {
        controls.push("kitty-escape");
    } else if bytes == [0x1b] {
        controls.push("raw-escape");
    }
    for (byte, label) in [
        (0x03, "ctrl-c"),
        (0x04, "ctrl-d"),
        (0x15, "ctrl-u"),
        (0x19, "ctrl-y"),
        (0x1a, "ctrl-z"),
        (0x1c, "ctrl-backslash"),
    ] {
        if bytes.contains(&byte) {
            controls.push(label);
        }
    }
    controls
}

fn log_temporary_terminal_control_write(model: &TerminalModel, source: &str, bytes: &[u8]) {
    let controls = temporary_terminal_control_labels(bytes);
    if controls.is_empty() {
        return;
    }
    crate::support_logs::append_temporary(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.gpui.sessionInterrupt.controlWrite",
        serde_json::json!({
            "byteLength": bytes.len(),
            "childPid": model.diagnostic_child_pid(),
            "controls": controls,
            "source": source,
        }),
    );
}

fn install_terminal_paste_diagnostic_sink() {
    crate::terminal_model::install_paste_diagnostic_sink(Arc::new(|diagnostic| {
        let (event, details) = match diagnostic {
            TerminalPasteDiagnostic::Encoded {
                trace_id,
                child_pid,
                bracketed_paste,
                source_byte_length,
                source_contains_line_break,
                source_contains_non_ascii,
                encoded_byte_length,
            } => (
                "TEMP.gpui.remotePaste.encoded",
                serde_json::json!({
                    "bracketedPaste": bracketed_paste,
                    "childPid": child_pid,
                    "encodedByteLength": encoded_byte_length,
                    "sourceByteLength": source_byte_length,
                    "sourceContainsLineBreak": source_contains_line_break,
                    "sourceContainsNonAscii": source_contains_non_ascii,
                    "traceId": trace_id,
                }),
            ),
            TerminalPasteDiagnostic::ChannelQueued {
                trace_id,
                child_pid,
                success,
                error_kind,
            } => (
                "TEMP.gpui.remotePaste.channelQueued",
                serde_json::json!({
                    "childPid": child_pid,
                    "errorKind": error_kind.map(|kind| format!("{kind:?}")),
                    "success": success,
                    "traceId": trace_id,
                }),
            ),
            TerminalPasteDiagnostic::PtyWriteStarted {
                trace_id,
                child_pid,
                encoded_byte_length,
            } => (
                "TEMP.gpui.remotePaste.ptyWriteStarted",
                serde_json::json!({
                    "childPid": child_pid,
                    "encodedByteLength": encoded_byte_length,
                    "traceId": trace_id,
                }),
            ),
            TerminalPasteDiagnostic::PtyWriteCompleted {
                trace_id,
                child_pid,
                duration_micros,
                stage,
                success,
                error_kind,
            } => (
                "TEMP.gpui.remotePaste.ptyWriteCompleted",
                serde_json::json!({
                    "childPid": child_pid,
                    "durationMicros": duration_micros,
                    "errorKind": error_kind.map(|kind| format!("{kind:?}")),
                    "stage": stage,
                    "success": success,
                    "traceId": trace_id,
                }),
            ),
        };
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            event,
            details,
        );
    }));
}
const TERMINAL_FULL_RELOAD_ICON: &str = "titlebar/refresh.svg";
const TERMINAL_STASHED_PROMPTS_ICON: &str = "titlebar/stack.svg";
const TERMINAL_STASH_PROMPT_ICON: &str = "titlebar/stack-push.svg";
const TERMINAL_CHAT_VIEW_ICON: &str = "titlebar/message-circle.svg";
// #101010 blended 15% toward white.
const TERMINAL_SCROLL_BUTTON_HOVER_BACKGROUND_RGB: u32 = 0x343434;

/// Terminal font configuration used for cell metrics and run shaping.
/// TODO(P1e): sync from the app's terminal settings (shared_settings
/// font-family/size/weight) instead of this hardcoded app default.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalFontConfig {
    pub family: SharedString,
    pub size: Pixels,
    pub weight: FontWeight,
    pub cell_width_adjustment: TerminalMetricAdjustment,
    pub cell_height_adjustment: TerminalMetricAdjustment,
}

impl Default for TerminalFontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrains Mono".into(),
            size: px(13.),
            weight: FontWeight::LIGHT,
            cell_width_adjustment: TerminalMetricAdjustment::None,
            cell_height_adjustment: TerminalMetricAdjustment::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TerminalMetricAdjustment {
    #[default]
    None,
    Percent(f32),
    Absolute(f32),
}

impl TerminalMetricAdjustment {
    fn apply(self, value: Pixels) -> Pixels {
        match self {
            Self::None => value,
            Self::Percent(delta) => px((value.as_f32() * (1.0 + delta)).max(1.0)),
            Self::Absolute(delta) => px((value.as_f32() + delta).max(1.0)),
        }
    }
}

/// Cursor shape to draw. Defaults to Block; P1e syncs the real shape from
/// terminal modes/settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorShape {
    Block,
    Bar,
    Underline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalConfiguredColor {
    Rgb(Rgb),
    CellForeground,
    CellBackground,
}

impl TerminalConfiguredColor {
    fn resolve(self, frame: &TerminalSnapshot) -> Rgb {
        match self {
            Self::Rgb(color) => color,
            Self::CellForeground => frame.foreground,
            Self::CellBackground => frame.background,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalMouseShiftCapture {
    #[default]
    False,
    True,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalBackgroundImageFit {
    Contain,
    #[default]
    Cover,
    Stretch,
    Natural,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerminalBackgroundImage {
    pub path: PathBuf,
    pub opacity: f32,
    pub fit: TerminalBackgroundImageFit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerminalViewSettings {
    pub cursor_shape: TerminalCursorShape,
    pub background_image: Option<TerminalBackgroundImage>,
    pub cursor_blink: bool,
    pub cursor_opacity: f32,
    pub cursor_text: Option<TerminalConfiguredColor>,
    pub selection_background: Option<TerminalConfiguredColor>,
    pub selection_clear_on_copy: bool,
    pub selection_clear_on_typing: bool,
    pub selection_word_chars: String,
    pub copy_on_select: bool,
    pub selection_clipboard_enabled: bool,
    pub clipboard_trim_trailing_spaces: bool,
    pub mouse_hide_while_typing: bool,
    pub mouse_scroll_precision: f32,
    pub mouse_scroll_discrete: f32,
    pub mouse_shift_capture: TerminalMouseShiftCapture,
    pub scrollbar_visible: bool,
    pub scroll_to_bottom_when_typing: bool,
}

impl Default for TerminalViewSettings {
    fn default() -> Self {
        Self {
            cursor_shape: TerminalCursorShape::Block,
            background_image: None,
            cursor_blink: false,
            cursor_opacity: 1.0,
            cursor_text: None,
            selection_background: None,
            selection_clear_on_copy: false,
            selection_clear_on_typing: true,
            selection_word_chars: " \t'\"│`|:;,()[]{}<>$".to_string(),
            copy_on_select: false,
            selection_clipboard_enabled: false,
            clipboard_trim_trailing_spaces: true,
            mouse_hide_while_typing: false,
            mouse_scroll_precision: 1.0,
            mouse_scroll_discrete: 3.0,
            mouse_shift_capture: TerminalMouseShiftCapture::False,
            scrollbar_visible: true,
            scroll_to_bottom_when_typing: true,
        }
    }
}

/// Selection over viewport cells. Linear selections cover every cell from
/// `start` (inclusive) walking left-to-right/top-to-bottom to `end`
/// (exclusive column on the end row); rectangular selections (alt+drag)
/// cover the column range on every row between start and end. Driven by
/// mouse selection below; ranges are normalized (start/end swapped as
/// needed) wherever they are consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelection {
    pub start_row: u64,
    pub start_col: u16,
    pub end_row: u64,
    pub end_col: u16,
    pub rectangular: bool,
}

/// Drag granularity from the click count (single/double/triple click) plus
/// the alt+drag rectangular mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectMode {
    Cell,
    Block,
    Word,
    Line,
}

/// App-facing notifications from a live terminal view. The pane layer
/// subscribes to route titles/pwd into tab chrome, bell into the app's
/// bell path, exits into tab lifecycle, and cmd+click URLs into the app's
/// vetted opener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalViewEvent {
    TitleChanged(String),
    PwdChanged(String),
    Bell,
    Exited(TerminalExit),
    OpenUrlRequested(String),
    PasteRequested,
    ControlVRequested,
    PathsDropped(Vec<PathBuf>),
    AttachPathsRequested,
    AgentActionRequested(TerminalAgentActionRequest),
    EscapePressed,
    FirstPromptTitleGenerationCancelRequested,
    PromptEditorShortcutRequested,
    FocusChanged { focused: bool },
    KeyRouteDiagnostic(TerminalKeyRouteDiagnostic),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalAgentActionRequest {
    Rename,
    Sleep,
    DelayedActions,
    Fork,
    FullReload,
    StashPrompt,
    StashedPrompts,
    ToggleChatView,
}

/// Text-free metadata for diagnosing the GPUI-to-libghostty key boundary.
/// Scalar values identify layout translation without retaining typed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalKeyRouteDiagnostic {
    pub action: &'static str,
    pub accepted: bool,
    pub consumed_mods: VtMods,
    pub key_name: &'static str,
    pub key_codepoint: Option<u32>,
    pub key_char_codepoint: Option<u32>,
    pub kitty_keyboard_flags: Option<u8>,
    pub mods: VtMods,
    pub option_as_alt_translation: bool,
    pub utf8_codepoint: Option<u32>,
}

/// Hovered link under the pointer: OSC 8 hyperlink or a scheme-scanned URL
/// in the hovered row. `row`/`cols` bound the hover underline.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HoveredLink {
    url: String,
    row: u16,
    cols: Range<u16>,
}

/// Viewport-scoped find state: matches recompute against the current
/// snapshot on every refresh, so highlights track live output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TerminalSearch {
    needle: String,
    /// `(absolute scrollback row, column range)` per match, in buffer order.
    matches: Vec<(u64, Range<u16>)>,
    /// Index into `matches` of the currently selected match.
    selected: usize,
}

/// An in-progress local selection drag, anchored at the mouse-down cell.
#[derive(Clone, Copy, Debug)]
struct SelectionDrag {
    mode: SelectMode,
    anchor: (u64, u16),
}

/// Per-cell metrics derived from the configured font.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CellMetrics {
    cell_width: Pixels,
    line_height: Pixels,
}

/// A horizontal run of same-colored cells within one row.
#[derive(Clone, Debug)]
struct CellSpan {
    col: u16,
    len: u16,
    color: Hsla,
}

/// A shaped text run anchored at a grid column.
struct PositionedRun {
    col: u16,
    shaped: ShapedLine,
}

/// Cached per-row layout: everything needed to paint one row except
/// selection/cursor/marked text, which change independently of row content.
struct RowLayout {
    bg_spans: Vec<CellSpan>,
    overline_spans: Vec<CellSpan>,
    runs: Vec<PositionedRun>,
}

struct CursorLayout {
    col: u16,
    row: u16,
    width_cells: u16,
    shape: TerminalCursorShape,
    color: Hsla,
    /// Unfocused terminals draw a hollow (outline) block instead of a
    /// filled cursor.
    hollow: bool,
    /// Block cursors repaint the covered glyph in the inverted color.
    overlay: Option<ShapedLine>,
}

struct MarkedTextLayout {
    col: u16,
    row: u16,
    shaped: ShapedLine,
    background: Hsla,
}

struct ScrollbarLayout {
    slot: Bounds<Pixels>,
    knob: Bounds<Pixels>,
    slot_color: Hsla,
    knob_color: Hsla,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalScrollButtonVisibility {
    top: bool,
    bottom: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalScrollEdge {
    Top,
    Bottom,
}

/// Prepaint output consumed by paint; positions are grid coordinates
/// converted to pixels against the element origin at paint time.
pub struct TerminalLayout {
    metrics: CellMetrics,
    background: Hsla,
    rows: Vec<Arc<RowLayout>>,
    selection_spans: Vec<(u16, CellSpan)>,
    search_spans: Vec<(u16, CellSpan)>,
    link_underline: Option<(u16, CellSpan)>,
    cursor: Option<CursorLayout>,
    marked_text: Option<MarkedTextLayout>,
    scrollbar: Option<ScrollbarLayout>,
}

/// Entity that owns a live terminal: the P1b model, the latest snapshot, and
/// the shaped-row cache. Rendered by [`TerminalElement`]; its own `Render`
/// impl just emits that element so `cx.notify()` re-renders naturally.
pub struct TerminalView {
    model: TerminalModel,
    font: TerminalFontConfig,
    /// Settings/config-derived size restored by the focused-surface reset shortcut.
    configured_font_size: Pixels,
    frame: Option<TerminalSnapshot>,
    row_cache: Vec<Option<Arc<RowLayout>>>,
    cached_metrics: Option<CellMetrics>,
    exit: Option<TerminalExit>,
    pub cursor_shape: TerminalCursorShape,
    settings: TerminalViewSettings,
    /// Selection over viewport cells, driven by the mouse handlers below.
    pub selection: Option<TerminalSelection>,
    /// IME marked (pre-edit) text drawn at the cursor, driven by the
    /// EntityInputHandler impl below.
    pub marked_text: Option<String>,
    focus_handle: FocusHandle,
    /// Focus state as of the last prepaint; edges send focus reports
    /// (mode 1004) and switch the cursor to hollow.
    focused: bool,
    last_modifiers: Modifiers,
    input_suppressed: bool,
    cursor_blink_visible: bool,
    /// Local selection drag in progress (left button held, no reporting).
    drag: Option<SelectionDrag>,
    /// The left press was routed to PTY mouse reporting; drag sends motion
    /// and release goes to the PTY even if modes change mid-drag.
    reporting_drag: bool,
    left_button_down: bool,
    /// Fractional wheel rows not yet dispatched (trackpad smoothness).
    wheel_accum: f32,
    /// Runtime scrollbar reveal state for local scrollback gestures.
    scrollbar_visible: bool,
    scrollbar_hide_generation: u64,
    scrollbar_drag_offset: Option<f32>,
    terminal_bounds: Option<Bounds<Pixels>>,
    scroll_button_visibility: TerminalScrollButtonVisibility,
    agent_actions_visible: bool,
    chat_view_action_visible: bool,
    agent_actions_expanded: bool,
    /// Last OSC title/pwd read back from the terminal, for change detection.
    title: Option<String>,
    pwd: Option<String>,
    /// Grid cell the pointer last hovered, gating link re-resolution.
    hover_cell: Option<(u16, u16)>,
    /// Link under the pointer, if any (underlined; cmd+click opens).
    hovered_link: Option<HoveredLink>,
    /// Active in-terminal find, if open.
    search: Option<TerminalSearch>,
}

impl TerminalView {
    /// Spawn the configured process and start pumping model events onto the
    /// gpui foreground. Every Wakeup takes one snapshot and notifies.
    pub fn spawn(
        config: TerminalSpawnConfig,
        font: TerminalFontConfig,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Self> {
        let (sink, event_rx) = Self::event_channel();
        let model = TerminalModel::spawn(config, sink)?;
        Ok(Self::from_model(model, event_rx, font, cx))
    }

    /// Sink/receiver pair for [`from_model`](Self::from_model), letting
    /// hosts spawn the model fallibly before creating the entity.
    pub fn event_channel() -> (
        TerminalEventSink,
        futures::channel::mpsc::UnboundedReceiver<TerminalEvent>,
    ) {
        install_terminal_paste_diagnostic_sink();
        let (event_tx, event_rx) = futures::channel::mpsc::unbounded::<TerminalEvent>();
        let sink: TerminalEventSink = Arc::new(move |event| {
            let _ = event_tx.unbounded_send(event);
        });
        (sink, event_rx)
    }

    /// Wrap an already-spawned model and start pumping its events onto the
    /// gpui foreground. The receiver must come from the same
    /// [`event_channel`](Self::event_channel) whose sink the model uses.
    pub fn from_model(
        model: TerminalModel,
        mut event_rx: futures::channel::mpsc::UnboundedReceiver<TerminalEvent>,
        font: TerminalFontConfig,
        cx: &mut Context<Self>,
    ) -> Self {
        let configured_font_size = font.size;
        cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.next().await {
                let Ok(()) = this.update(cx, |view, cx| view.handle_event(event, cx)) else {
                    break;
                };
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(TERMINAL_CURSOR_BLINK_INTERVAL)
                    .await;
                let Ok(()) = this.update(cx, |view, cx| {
                    if view.settings.cursor_blink && view.focused {
                        view.cursor_blink_visible = !view.cursor_blink_visible;
                        cx.notify();
                    } else if !view.cursor_blink_visible {
                        view.cursor_blink_visible = true;
                        cx.notify();
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();

        Self {
            model,
            font,
            configured_font_size,
            frame: None,
            row_cache: Vec::new(),
            cached_metrics: None,
            exit: None,
            cursor_shape: TerminalCursorShape::Block,
            settings: TerminalViewSettings::default(),
            selection: None,
            marked_text: None,
            focus_handle: cx.focus_handle(),
            focused: false,
            last_modifiers: Modifiers::default(),
            input_suppressed: false,
            cursor_blink_visible: true,
            drag: None,
            reporting_drag: false,
            left_button_down: false,
            wheel_accum: 0.,
            scrollbar_visible: false,
            scrollbar_hide_generation: 0,
            scrollbar_drag_offset: None,
            terminal_bounds: None,
            scroll_button_visibility: TerminalScrollButtonVisibility::default(),
            agent_actions_visible: false,
            chat_view_action_visible: false,
            agent_actions_expanded: false,
            title: None,
            pwd: None,
            hover_cell: None,
            hovered_link: None,
            search: None,
        }
    }

    pub fn model(&self) -> &TerminalModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut TerminalModel {
        &mut self.model
    }

    pub fn paste_requires_confirmation(&mut self, text: &str) -> bool {
        if text.contains("\u{1b}[201~") {
            return true;
        }
        !self.model.mode_active(ffi::GHOSTTY_MODE_BRACKETED_PASTE) && text.contains('\n')
    }

    pub fn apply_settings(&mut self, settings: TerminalViewSettings) {
        self.cursor_shape = settings.cursor_shape;
        self.cursor_blink_visible = true;
        self.settings = settings;
        self.cached_metrics = None;
        self.row_cache.fill(None);
    }

    pub fn apply_font(&mut self, font: TerminalFontConfig) {
        self.configured_font_size = font.size;
        if self.font == font {
            return;
        }
        self.font = font;
        self.cached_metrics = None;
        self.row_cache.fill(None);
    }

    pub fn adjust_font_size(&mut self, delta: f32, min: f32, max: f32) -> bool {
        self.set_font_size(px((self.font.size.as_f32() + delta).clamp(min, max)))
    }

    pub fn reset_font_size(&mut self) -> bool {
        self.set_font_size(self.configured_font_size)
    }

    fn set_font_size(&mut self, size: Pixels) -> bool {
        if self.font.size == size {
            return false;
        }
        self.font.size = size;
        self.cached_metrics = None;
        self.row_cache.fill(None);
        true
    }

    pub fn set_input_suppressed(&mut self, suppressed: bool, cx: &mut Context<Self>) {
        if self.input_suppressed == suppressed {
            return;
        }
        self.input_suppressed = suppressed;
        if suppressed {
            self.marked_text = None;
        }
        cx.notify();
    }

    pub fn set_agent_actions_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.agent_actions_visible == visible {
            return;
        }
        self.agent_actions_visible = visible;
        if !visible {
            self.agent_actions_expanded = false;
        }
        cx.notify();
    }

    pub fn set_chat_view_action_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.chat_view_action_visible == visible {
            return;
        }
        self.chat_view_action_visible = visible;
        cx.notify();
    }

    pub fn toggle_agent_actions_expanded(&mut self, cx: &mut Context<Self>) {
        if !self.agent_actions_visible {
            return;
        }
        self.agent_actions_expanded = !self.agent_actions_expanded;
        cx.notify();
    }

    pub fn scroll_to_top(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_to_edge(TerminalScrollEdge::Top, window, cx);
    }

    pub fn scroll_to_bottom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_to_edge(TerminalScrollEdge::Bottom, window, cx);
    }

    /// Read either the visible viewport or the complete scrollback using the
    /// same soft-wrap joining semantics as native Ghostty readback.
    pub fn read_text(&mut self, visible_only: bool) -> Option<String> {
        let rows = if visible_only {
            let frame = self.frame.as_ref()?;
            frame
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| TerminalTextRow {
                    absolute_row: frame.scrollbar.offset + index as u64,
                    wraps: row.wraps,
                    wrap_continuation: row.wrap_continuation,
                    cells: row
                        .cells
                        .iter()
                        .enumerate()
                        .filter(|(_, cell)| {
                            !matches!(cell.width, VtCellWide::SpacerTail | VtCellWide::SpacerHead)
                        })
                        .map(|(column, cell)| {
                            let mut text = cell.base.to_string();
                            if let Some(combining) = &cell.combining {
                                text.push_str(combining);
                            }
                            crate::terminal_model::TerminalTextCell {
                                column: column as u16,
                                text,
                            }
                        })
                        .collect(),
                })
                .collect()
        } else {
            self.model.read_scrollback_rows().ok()?
        };
        let mut text = String::new();
        for (index, row) in rows.iter().enumerate() {
            text.push_str(row.text().trim_end());
            if index + 1 < rows.len() && !row.wraps {
                text.push('\n');
            }
        }
        Some(text)
    }

    pub fn exit_status(&self) -> Option<TerminalExit> {
        self.exit
    }

    /// Whether closing this terminal should ask for confirmation, per the
    /// app's confirm-close policy and the live process/prompt state.
    pub fn needs_confirm_close(&mut self, behavior: TerminalConfirmCloseBehavior) -> bool {
        self.model.needs_confirm_close(behavior)
    }

    fn handle_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Wakeup => {
                self.refresh_snapshot();
                self.sync_title_and_pwd(cx);
                cx.notify();
            }
            TerminalEvent::Exited(exit) => {
                // Contents stay readable after exit; the final Wakeup may
                // land before or after this by design.
                self.exit = Some(exit);
                cx.emit(TerminalViewEvent::Exited(exit));
                cx.notify();
            }
            TerminalEvent::Bell => cx.emit(TerminalViewEvent::Bell),
            // The changed title (and any pwd that came with it) is read back
            // from the terminal; the event itself carries no string.
            TerminalEvent::TitleChanged => self.sync_title_and_pwd(cx),
        }
    }

    /// Re-read title/pwd from the terminal and emit only actual changes.
    /// OSC 7 has no dedicated vt callback, so this also runs per wakeup.
    fn sync_title_and_pwd(&mut self, cx: &mut Context<Self>) {
        let title = self.model.title();
        if title != self.title {
            self.title = title.clone();
            cx.emit(TerminalViewEvent::TitleChanged(title.unwrap_or_default()));
        }
        let pwd = self.model.pwd();
        if pwd != self.pwd {
            self.pwd = pwd.clone();
            cx.emit(TerminalViewEvent::PwdChanged(pwd.unwrap_or_default()));
        }
    }

    /// Latest OSC title read back from the terminal, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Latest OSC 7 pwd read back from the terminal, if any.
    pub fn pwd(&self) -> Option<&str> {
        self.pwd.as_deref()
    }

    /// Take one snapshot and fold its dirty info into the row cache. Keeps
    /// the previous frame on snapshot errors so the next wakeup retries.
    fn refresh_snapshot(&mut self) {
        let Ok(frame) = self.model.snapshot() else {
            return;
        };
        if self.row_cache.len() != frame.rows.len() {
            self.row_cache = vec![None; frame.rows.len()];
        } else {
            match frame.dirty {
                VtDirty::Clean => {}
                VtDirty::Full => self.row_cache.fill(None),
                VtDirty::Partial => {
                    for (slot, row) in self.row_cache.iter_mut().zip(&frame.rows) {
                        if row.dirty {
                            *slot = None;
                        }
                    }
                }
            }
        }
        self.frame = Some(frame);
        self.recompute_search_matches();
        self.update_scroll_button_visibility();
    }

    fn update_scroll_button_visibility(&mut self) -> bool {
        let visibility = self.compute_scroll_button_visibility();
        if visibility == self.scroll_button_visibility {
            return false;
        }
        self.scroll_button_visibility = visibility;
        true
    }

    fn compute_scroll_button_visibility(&self) -> TerminalScrollButtonVisibility {
        let Some(bounds) = self.terminal_bounds else {
            return TerminalScrollButtonVisibility::default();
        };
        if bounds.size.width < px(TERMINAL_SCROLL_BUTTON_MIN_WIDTH)
            || bounds.size.height < px(TERMINAL_SCROLL_BUTTON_MIN_HEIGHT)
        {
            return TerminalScrollButtonVisibility::default();
        }

        let Some(metrics) = self.cached_metrics else {
            return TerminalScrollButtonVisibility::default();
        };
        let Some(frame) = self.frame.as_ref() else {
            return TerminalScrollButtonVisibility::default();
        };
        let scrollbar = frame.scrollbar;
        if scrollbar.total <= scrollbar.len {
            return TerminalScrollButtonVisibility::default();
        }

        let line_height = f64::from(metrics.line_height.as_f32());
        let distance_from_top = scrollbar.offset.min(scrollbar.total) as f64 * line_height;
        let distance_from_bottom = scrollbar
            .total
            .saturating_sub(scrollbar.offset.saturating_add(scrollbar.len))
            as f64
            * line_height;
        let threshold = f64::from(TERMINAL_SCROLL_BUTTON_VISIBILITY_THRESHOLD);
        let bottom = distance_from_bottom > threshold;
        let top = distance_from_top > threshold && bottom;

        TerminalScrollButtonVisibility { top, bottom }
    }

    fn scroll_to_edge(
        &mut self,
        edge: TerminalScrollEdge,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.model.scroll_viewport(match edge {
            TerminalScrollEdge::Top => VtScrollViewport::Top,
            TerminalScrollEdge::Bottom => VtScrollViewport::Bottom,
        });
        self.refresh_snapshot();
        cx.notify();
    }

    /// Recompute full-scrollback search matches,
    /// preserving the selected index when possible.
    fn recompute_search_matches(&mut self) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.matches.clear();
        if search.needle.is_empty() {
            search.selected = 0;
            return;
        }
        if let Ok(rows) = self.model.read_scrollback_rows() {
            let needle = search.needle.to_lowercase();
            for row in rows {
                let (text, columns) = terminal_text_row_with_columns(&row);
                let haystack = text.to_lowercase();
                let mut from = 0;
                while let Some(found) = haystack[from..].find(&needle) {
                    let start = from + found;
                    let end = start + needle.len();
                    let start_col = columns.get(start).copied().unwrap_or(0);
                    let end_col = columns
                        .get(end.saturating_sub(1))
                        .map(|col| col + 1)
                        .unwrap_or_else(|| self.frame.as_ref().map_or(0, |frame| frame.cols));
                    search.matches.push((row.absolute_row, start_col..end_col));
                    from = end.max(from + 1);
                }
            }
        }
        if search.matches.is_empty() {
            search.selected = 0;
        } else {
            search.selected = search.selected.min(search.matches.len() - 1);
        }
    }

    /// Open (or retarget) the in-terminal find with the given needle.
    /// Passing the same needle keeps the current selected match.
    pub fn set_search_needle(&mut self, needle: &str, cx: &mut Context<Self>) {
        match self.search.as_mut() {
            Some(search) if search.needle == needle => return,
            Some(search) => {
                search.needle = needle.to_string();
                search.selected = 0;
            }
            None => {
                self.search = Some(TerminalSearch {
                    needle: needle.to_string(),
                    ..TerminalSearch::default()
                });
            }
        }
        self.recompute_search_matches();
        cx.notify();
    }

    /// Close the in-terminal find and drop its highlights.
    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            cx.notify();
        }
    }

    /// Step the selected match forward/backward, wrapping at the ends.
    pub fn navigate_search(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        let count = search.matches.len();
        if count == 0 {
            return;
        }
        search.selected = if forward {
            (search.selected + 1) % count
        } else {
            (search.selected + count - 1) % count
        };
        let row = search.matches[search.selected].0;
        let viewport_len = self
            .frame
            .as_ref()
            .map(|frame| frame.scrollbar.len)
            .unwrap_or(1);
        self.model
            .scroll_viewport_to_row(row.saturating_sub(viewport_len / 2));
        self.refresh_snapshot();
        cx.notify();
    }

    /// `(total matches, selected 1-based index)` of the open find, if any.
    pub fn search_totals(&self) -> Option<(usize, usize)> {
        let search = self.search.as_ref()?;
        let total = search.matches.len();
        let selected = if total == 0 { 0 } else { search.selected + 1 };
        Some((total, selected))
    }

    /// Whether the in-terminal find is open.
    pub fn search_active(&self) -> bool {
        self.search.is_some()
    }

    /// After writing user input to the PTY: drop the local selection, snap
    /// the viewport back to the live tail (ghostty behavior), and redraw.
    fn after_send_input(&mut self, cx: &mut Context<Self>) {
        if self.settings.mouse_hide_while_typing {
            hide_mouse_cursor_until_mouse_moves();
        }
        self.cursor_blink_visible = true;
        if self.settings.selection_clear_on_typing {
            self.selection = None;
        }
        self.drag = None;
        if self.settings.scroll_to_bottom_when_typing {
            self.model.scroll_viewport(VtScrollViewport::Bottom);
        }
        self.refresh_snapshot();
        cx.notify();
    }

    fn reveal_scrollbar_for_user_scroll(&mut self, cx: &mut Context<Self>) {
        self.scrollbar_hide_generation = self.scrollbar_hide_generation.wrapping_add(1);
        let generation = self.scrollbar_hide_generation;
        self.scrollbar_visible = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TERMINAL_SCROLLBAR_HIDE_DELAY)
                .await;

            let _ = this.update(cx, |view, cx| {
                if view.scrollbar_hide_generation == generation {
                    view.scrollbar_visible = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        self.last_modifiers = *modifiers;
        /*
        CDXC:GPUISessionTitleOverlay 2026-07-26:
        While a first-prompt title is generating the pane is input-suppressed
        and shows the blocking "Generating title" overlay. Escape is the
        documented cancel affordance there, so it reports the cancel side band
        instead of the plain terminal-escape side band and never reaches the
        child process.
        */
        if self.input_suppressed {
            if keystroke.key == "escape" && !modifiers.modified() {
                crate::support_logs::append_temporary(
                    crate::support_logs::GpuiSupportLog::TerminalFocus,
                    "TEMP.gpui.sessionInterrupt.titleCancelKey",
                    serde_json::json!({
                        "childPid": self.model.diagnostic_child_pid(),
                        "held": event.is_held,
                    }),
                );
                cx.emit(TerminalViewEvent::FirstPromptTitleGenerationCancelRequested);
            }
            cx.stop_propagation();
            return;
        }

        /*
        CDXC:GPUITerminalNaturalEditing 2026-07-12:
        The composited libghostty-vt path owns terminal encoding but not the
        embedded surface's AppKit key-equivalent layer. Preserve the managed
        macOS pane contract here: natural line editing uses Ctrl-A/E, Cmd-G is
        the rich-editor Ctrl-G chord, and the main editing commands are sent as
        explicit Super CSI-u sequences before GPUI application bindings can
        claim them.
        */
        if cfg!(target_os = "macos")
            && modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.function
        {
            let managed_editing_input: Option<Vec<u8>> =
                match (modifiers.shift, keystroke.key.as_str()) {
                    (false, "left") => Some(b"\x01".to_vec()),
                    (false, "right") => Some(b"\x05".to_vec()),
                    (false, key @ ("a" | "c" | "s" | "y" | "z")) => key
                        .chars()
                        .next()
                        .map(|key| format!("\x1b[{};9u", key as u32).into_bytes()),
                    (true, "z") => Some(b"\x1b[122;10u".to_vec()),
                    _ => None,
                };
            if let Some(input) = managed_editing_input {
                // Preserve native Copy semantics when the terminal owns a
                // local selection; otherwise Cmd-C remains Super-C for TUIs.
                if !modifiers.shift && keystroke.key == "c" && self.selection.is_some() {
                    self.copy_selection(cx);
                    cx.stop_propagation();
                    return;
                }
                let _ = self.model.write_input(&input);
                self.after_send_input(cx);
                cx.stop_propagation();
                return;
            }
        }

        // Clipboard shortcuts are app-owned; they never reach the PTY.
        if modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function {
            match keystroke.key.as_str() {
                "c" if self.selection.is_some() => {
                    self.copy_selection(cx);
                    cx.stop_propagation();
                    return;
                }
                "v" => {
                    cx.emit(TerminalViewEvent::PasteRequested);
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        // Ctrl-V remains literal terminal input for text clipboards, but the
        // app must first get a chance to convert an image-only clipboard into
        // the same Markdown reference used by Cmd-V and contextual Paste.
        if modifiers.control
            && !modifiers.platform
            && !modifiers.alt
            && !modifiers.function
            && keystroke.key == "v"
        {
            cx.emit(TerminalViewEvent::ControlVRequested);
            cx.stop_propagation();
            return;
        }

        // Plain option-modified text keys belong to the IME/insertText path
        // while option is not acting as alt: dead keys compose there
        // (option-e + e -> é) and option-symbols (ß, ∫) arrive as committed
        // text. Consuming them here would break composition, so let them
        // propagate to the platform text input path.
        if modifiers.alt
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.function
            && keystroke.key_char.is_some()
            && !self.model.option_sends_alt()
        {
            return;
        }

        let option_as_alt_translation = modifiers.alt
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.function
            && self.model.option_sends_alt();
        let (key, unshifted_codepoint) = keystroke_vt_key(keystroke);
        let utf8 = if option_as_alt_translation {
            keystroke_option_as_alt_text(keystroke)
        } else {
            keystroke_text(keystroke)
        };
        let mods = vt_mods(modifiers);
        let input = VtKeyInput {
            action: if event.is_held {
                VtKeyAction::Repeat
            } else {
                VtKeyAction::Press
            },
            key,
            mods,
            // GPUI's key_char has already folded shift/option into produced
            // text. When Option acts as Alt, use GPUI's documented physical
            // layout key instead and leave every modifier unconsumed so
            // libghostty performs the configured Alt/Shift encoding itself.
            consumed_mods: if utf8.is_some() && !option_as_alt_translation {
                mods & (ffi::GHOSTTY_MODS_SHIFT | ffi::GHOSTTY_MODS_ALT)
            } else {
                0
            },
            utf8,
            unshifted_codepoint,
        };
        let kitty_keyboard_flags = self.model.kitty_keyboard_flags();
        let accepted = self.model.send_key(&input);
        if keystroke.key == "escape"
            || (modifiers.control && matches!(keystroke.key.as_str(), "c" | "d" | "z" | "\\"))
        {
            crate::support_logs::append_temporary(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.sessionInterrupt.physicalKey",
                serde_json::json!({
                    "accepted": accepted,
                    "childPid": self.model.diagnostic_child_pid(),
                    "control": modifiers.control,
                    "held": event.is_held,
                    "key": keystroke.key,
                    "kittyKeyboardFlags": kitty_keyboard_flags,
                    "markedTextActive": self.marked_text.is_some(),
                }),
            );
        }
        /*
        CDXC:GPUISessionTitleOverlay 2026-07-26:
        The agent-cancel side band reports Escape only when Escape is actually
        forwarded to the terminal process, mirroring the managed pane. During
        IME composition Escape belongs to marked text and must not race agent
        status, and while input is suppressed it is the title-generation cancel
        instead (handled above).
        */
        if keystroke.key == "escape" && !modifiers.modified() && self.marked_text.is_none() {
            cx.emit(TerminalViewEvent::EscapePressed);
        }
        if keystroke.key == "tab"
            || (keystroke.key == "enter" && modifiers.shift)
            || (modifiers.alt && matches!(keystroke.key.as_str(), "," | "."))
        {
            cx.emit(TerminalViewEvent::KeyRouteDiagnostic(
                TerminalKeyRouteDiagnostic {
                    action: match input.action {
                        VtKeyAction::Press => "press",
                        VtKeyAction::Repeat => "repeat",
                        VtKeyAction::Release => "release",
                    },
                    accepted,
                    consumed_mods: input.consumed_mods,
                    key_name: match keystroke.key.as_str() {
                        "enter" => "enter",
                        "tab" => "tab",
                        _ => "option-symbol",
                    },
                    key_codepoint: single_scalar_codepoint(Some(keystroke.key.as_str())),
                    key_char_codepoint: single_scalar_codepoint(keystroke.key_char.as_deref()),
                    kitty_keyboard_flags,
                    mods: input.mods,
                    option_as_alt_translation,
                    utf8_codepoint: single_scalar_codepoint(input.utf8),
                },
            ));
        }
        if accepted {
            self.after_send_input(cx);
            cx.stop_propagation();
        }
    }

    /// Key releases only produce output under kitty report-events mode;
    /// they never consume the event.
    fn handle_key_up(&mut self, event: &KeyUpEvent) {
        let keystroke = &event.keystroke;
        self.last_modifiers = keystroke.modifiers;
        let (key, unshifted_codepoint) = keystroke_vt_key(keystroke);
        let _ = self.model.send_key(&VtKeyInput {
            action: VtKeyAction::Release,
            key,
            mods: vt_mods(&keystroke.modifiers),
            consumed_mods: 0,
            utf8: None,
            unshifted_codepoint,
        });
    }

    /// Forward modifier-only transitions for kitty report-event mode. GPUI
    /// exposes semantic modifiers rather than left/right hardware keycodes, so
    /// use the canonical left-side identity while preserving the exact state
    /// after each transition, matching the managed surface's flagsChanged path.
    fn handle_modifiers_changed(&mut self, event: &ModifiersChangedEvent) {
        let previous = self.last_modifiers;
        let current = event.modifiers;
        self.last_modifiers = current;
        for (was_down, is_down, key) in [
            (previous.shift, current.shift, VtKey::ShiftLeft),
            (previous.control, current.control, VtKey::ControlLeft),
            (previous.alt, current.alt, VtKey::AltLeft),
            (previous.platform, current.platform, VtKey::MetaLeft),
        ] {
            if was_down == is_down {
                continue;
            }
            let _ = self.model.send_key(&VtKeyInput {
                action: if is_down {
                    VtKeyAction::Press
                } else {
                    VtKeyAction::Release
                },
                key,
                mods: vt_mods(&current),
                consumed_mods: 0,
                utf8: None,
                unshifted_codepoint: 0,
            });
        }
    }

    /// Commit IME text (or press-and-hold repeats) to the PTY as UTF-8. The
    /// borrowed platform string is written immediately and never retained
    /// (AppKit reuses IME insert buffers; gpui also copies before this).
    fn commit_ime_text(&mut self, text: &str, cx: &mut Context<Self>) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedImeCommit",
            serde_json::json!({
                "inputSuppressed": self.input_suppressed,
                "text": crate::support_logs::temporary_fluid_voice_text_shape(text),
            }),
        );
        if self.input_suppressed {
            self.marked_text = None;
            cx.notify();
            return;
        }
        self.marked_text = None;
        if text.is_empty() {
            cx.notify();
            return;
        }
        let _ = self.model.write_input(text.as_bytes());
        self.after_send_input(cx);
    }

    /// Host-initiated literal text input (prompt sends, rename flows):
    /// bytes go straight to the PTY like typed text.
    pub fn send_text_input(&mut self, text: &str, cx: &mut Context<Self>) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedLiteralText",
            serde_json::json!({
                "inputSuppressed": self.input_suppressed,
                "text": crate::support_logs::temporary_fluid_voice_text_shape(text),
            }),
        );
        if text.is_empty() {
            return;
        }
        log_temporary_terminal_control_write(&self.model, "host-literal-text", text.as_bytes());
        let _ = self.model.write_input(text.as_bytes());
        self.after_send_input(cx);
    }

    /// Host-initiated paste (app-level Cmd+V routing): honors bracketed
    /// paste mode like the element's own clipboard shortcut.
    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedPaste",
            serde_json::json!({
                "childPid": self.model.diagnostic_child_pid(),
                "inputSuppressed": self.input_suppressed,
                "text": crate::support_logs::temporary_fluid_voice_text_shape(text),
            }),
        );
        if text.is_empty() {
            return;
        }
        let _ = self.model.send_paste(text);
        self.after_send_input(cx);
    }

    /// Host-initiated Return (delayed send, rename submission), encoded
    /// against the live keyboard modes like a pressed Enter key.
    pub fn send_return_key(&mut self, cx: &mut Context<Self>) {
        let input = VtKeyInput {
            action: VtKeyAction::Press,
            key: VtKey::Enter,
            mods: 0,
            consumed_mods: 0,
            utf8: None,
            unshifted_codepoint: 0,
        };
        if self.model.send_key(&input) {
            self.after_send_input(cx);
        }
    }

    /// Host-initiated Ctrl+<letter> (rename flows kill/yank the composer line
    /// around a staged command), encoded against the live keyboard modes like
    /// a pressed key so kitty-protocol CLIs receive a real key event.
    pub fn send_ctrl_letter_key(&mut self, key: VtKey, letter: char, cx: &mut Context<Self>) {
        let input = VtKeyInput {
            action: VtKeyAction::Press,
            key,
            mods: ffi::GHOSTTY_MODS_CTRL,
            consumed_mods: 0,
            utf8: None,
            unshifted_codepoint: letter as u32,
        };
        if self.model.send_key(&input) {
            self.after_send_input(cx);
        }
    }

    /// Host-initiated Tab for macOS, whose text-input system consumes the
    /// native event before GPUI's key callback. Encode it against the same
    /// live terminal modes as the normal element key path.
    pub fn send_tab_key_action(
        &mut self,
        action: VtKeyAction,
        shift: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let mods = if shift { ffi::GHOSTTY_MODS_SHIFT } else { 0 };
        let input = VtKeyInput {
            action,
            key: VtKey::Tab,
            mods,
            consumed_mods: 0,
            utf8: None,
            unshifted_codepoint: 0,
        };
        let kitty_keyboard_flags = self.model.kitty_keyboard_flags();
        let accepted = self.model.send_key(&input);
        cx.emit(TerminalViewEvent::KeyRouteDiagnostic(
            TerminalKeyRouteDiagnostic {
                action: match input.action {
                    VtKeyAction::Press => "press",
                    VtKeyAction::Repeat => "repeat",
                    VtKeyAction::Release => "release",
                },
                accepted,
                consumed_mods: input.consumed_mods,
                key_name: "tab",
                key_codepoint: None,
                key_char_codepoint: None,
                kitty_keyboard_flags,
                mods: input.mods,
                option_as_alt_translation: false,
                utf8_codepoint: None,
            },
        ));
        if accepted && action != VtKeyAction::Release {
            self.after_send_input(cx);
        }
        accepted
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(mut text) = self.selection_text()
            && !text.is_empty()
        {
            if self.settings.clipboard_trim_trailing_spaces {
                text.truncate(text.trim_end().len());
            }
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            if self.settings.selection_clear_on_copy {
                self.selection = None;
                cx.notify();
            }
        }
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let _ = self.model.send_paste(&text);
        self.after_send_input(cx);
    }

    /// Copy from absolute scrollback rows so selections remain stable while
    /// output or autoscroll moves the viewport. Soft-wrapped rows join without
    /// injecting a newline; rectangular selections retain row boundaries.
    fn selection_text(&mut self) -> Option<String> {
        let selection = self.selection?;
        let cols = self.frame.as_ref()?.cols;
        let (start, end) = {
            let start = (selection.start_row, selection.start_col);
            let end = (selection.end_row, selection.end_col);
            if start <= end {
                (start, end)
            } else {
                (end, start)
            }
        };
        let rows = self.model.read_scrollback_rows().ok()?;
        let selected_rows = rows
            .iter()
            .filter(|row| row.absolute_row >= start.0 && row.absolute_row <= end.0)
            .collect::<Vec<_>>();
        let mut text = String::new();
        for (index, row) in selected_rows.iter().enumerate() {
            let range = selection_row_cols(selection, start, end, row.absolute_row, cols);
            let mut line = row.text_in_columns(range.start, range.end);
            if self.settings.clipboard_trim_trailing_spaces {
                line.truncate(line.trim_end().len());
            }
            text.push_str(&line);
            if index + 1 < selected_rows.len() && (selection.rectangular || !row.wraps) {
                text.push('\n');
            }
        }
        Some(text)
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        origin: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        if event.button == MouseButton::Left {
            self.left_button_down = true;
            if let Some(scrollbar) = self.current_scrollbar_layout()
                && scrollbar.slot.contains(&event.position)
            {
                let grab_offset = if scrollbar.knob.contains(&event.position) {
                    (event.position.y - scrollbar.knob.origin.y).as_f32()
                } else {
                    scrollbar.knob.size.height.as_f32() / 2.0
                };
                self.scrollbar_drag_offset = Some(grab_offset);
                self.scrollbar_to_pointer(event.position.y, grab_offset);
                self.refresh_snapshot();
                cx.notify();
                cx.stop_propagation();
                return;
            }
        }

        let shift_captured = event.modifiers.shift
            && matches!(
                self.settings.mouse_shift_capture,
                TerminalMouseShiftCapture::True | TerminalMouseShiftCapture::Always
            );
        if (!event.modifiers.shift || shift_captured)
            && self.model.mouse_tracking()
            && let Some(button) = vt_mouse_button(event.button)
        {
            let (x, y) = self.device_position(event.position, origin, scale);
            let sent = self.model.send_mouse(
                &VtMouseInput {
                    action: VtMouseAction::Press,
                    button: Some(button),
                    mods: vt_mods(&event.modifiers),
                    x,
                    y,
                },
                true,
            );
            if sent {
                if event.button == MouseButton::Left {
                    self.reporting_drag = true;
                }
                if self.selection.take().is_some() {
                    cx.notify();
                }
                self.drag = None;
                cx.stop_propagation();
                return;
            }
        }

        /*
        CDXC:GPUITerminalContextMenu 2026-07-11:
        Match the managed macOS libghostty surface menu exactly: terminal body
        right-clicks open an OS-owned Copy/Paste menu, Copy reflects the live
        selection, and a terminal application that consumed mouse reporting
        above suppresses the menu. The menu is attached to this terminal's real
        hitbox and focus node; no overlay or hit-test rerouting is involved.
        */
        if event.button == MouseButton::Right {
            NativeMenu::new()
                .menu_with_disabled(
                    "Copy",
                    self.selection.is_none(),
                    Box::new(TerminalContextMenuCopy),
                )
                .menu("Paste", Box::new(TerminalContextMenuPaste))
                .show(event.position, window, cx);
            cx.stop_propagation();
            return;
        }

        if event.button == MouseButton::Middle && self.settings.selection_clipboard_enabled {
            if let Some(text) = self.selection_text() {
                let _ = self.model.send_paste(&text);
                self.after_send_input(cx);
                cx.stop_propagation();
            }
            return;
        }

        if event.button != MouseButton::Left {
            return;
        }
        let viewport_cell = self.grid_cell(event.position, origin);

        // Cmd+click opens the link under the pointer (ghostty behavior)
        // instead of starting a selection; the app vets and opens the URL.
        if event.modifiers.platform
            && let Some(link) = self
                .hovered_link
                .as_ref()
                .filter(|link| link.row == viewport_cell.0 && link.cols.contains(&viewport_cell.1))
        {
            let target = resolve_relative_link_target(&link.url, self.pwd.as_deref());
            cx.emit(TerminalViewEvent::OpenUrlRequested(target));
            return;
        }
        let cell = self.absolute_cell(viewport_cell);

        match event.click_count {
            1 => {
                self.selection = None;
                self.drag = Some(SelectionDrag {
                    // Alt+drag selects a rectangular block (ghostty/xterm
                    // convention); plain drag selects linearly.
                    mode: if event.modifiers.alt {
                        SelectMode::Block
                    } else {
                        SelectMode::Cell
                    },
                    anchor: cell,
                });
            }
            2 => {
                self.drag = Some(SelectionDrag {
                    mode: SelectMode::Word,
                    anchor: cell,
                });
                self.selection = self.frame.as_ref().map(|frame| {
                    word_selection(
                        frame,
                        frame.scrollbar.offset,
                        cell,
                        cell,
                        &self.settings.selection_word_chars,
                    )
                });
            }
            _ => {
                self.drag = Some(SelectionDrag {
                    mode: SelectMode::Line,
                    anchor: cell,
                });
                self.selection = self
                    .frame
                    .as_ref()
                    .map(|frame| line_selection(frame.cols, cell.0, cell.0));
            }
        }
        cx.notify();
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        origin: Point<Pixels>,
        scale: f32,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(grab_offset) = self.scrollbar_drag_offset
            && event.pressed_button == Some(MouseButton::Left)
        {
            self.scrollbar_to_pointer(event.position.y, grab_offset);
            self.refresh_snapshot();
            cx.notify();
            return;
        }
        if self.reporting_drag && event.pressed_button == Some(MouseButton::Left) {
            let (x, y) = self.device_position(event.position, origin, scale);
            self.model.send_mouse(
                &VtMouseInput {
                    action: VtMouseAction::Motion,
                    button: Some(VtMouseButton::Left),
                    mods: vt_mods(&event.modifiers),
                    x,
                    y,
                },
                true,
            );
            return;
        }

        if self.drag.is_some() && event.pressed_button == Some(MouseButton::Left) {
            if let Some(metrics) = self.cached_metrics {
                let viewport_height = metrics.line_height * f32::from(self.model.size().1);
                let delta = if event.position.y < origin.y {
                    -1
                } else if event.position.y >= origin.y + viewport_height {
                    1
                } else {
                    0
                };
                if delta != 0 {
                    self.model.scroll_viewport(VtScrollViewport::Delta(delta));
                    self.reveal_scrollbar_for_user_scroll(cx);
                    self.refresh_snapshot();
                }
            }
            let cell = self.grid_cell(event.position, origin);
            self.extend_selection(cell);
            cx.notify();
            return;
        }

        // Buttonless hover motion only reports in any-event tracking; the
        // encoder filters it out for other modes.
        if hovered
            && event.pressed_button.is_none()
            && (!event.modifiers.shift
                || matches!(
                    self.settings.mouse_shift_capture,
                    TerminalMouseShiftCapture::True | TerminalMouseShiftCapture::Always
                ))
            && self.model.mouse_tracking()
        {
            let (x, y) = self.device_position(event.position, origin, scale);
            self.model.send_mouse(
                &VtMouseInput {
                    action: VtMouseAction::Motion,
                    button: None,
                    mods: vt_mods(&event.modifiers),
                    x,
                    y,
                },
                false,
            );
        }

        if hovered && event.pressed_button.is_none() {
            self.update_hovered_link(event.position, origin, cx);
        } else if self.hover_cell.take().is_some() && self.hovered_link.take().is_some() {
            cx.notify();
        }
    }

    /// Resolve the link under the pointer when the hovered cell changes:
    /// OSC 8 hyperlinks first, then a conservative scheme scan of the
    /// hovered row's text (libghostty-vt exposes no regex link detection).
    fn update_hovered_link(
        &mut self,
        position: Point<Pixels>,
        origin: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let cell = self.grid_cell(position, origin);
        if self.hover_cell == Some(cell) {
            return;
        }
        self.hover_cell = Some(cell);
        let link = self.resolve_link_at(cell);
        if link != self.hovered_link {
            self.hovered_link = link;
            cx.notify();
        }
    }

    fn resolve_link_at(&mut self, cell: (u16, u16)) -> Option<HoveredLink> {
        let frame = self.frame.as_ref()?;
        let (row, col) = cell;
        let cells = &frame.rows.get(usize::from(row))?.cells;
        let hovered = cells.get(usize::from(col))?;

        if hovered.has_hyperlink {
            let url = self.model.hyperlink_uri_at_cell(col, row)?;
            // Underline the contiguous run of cells sharing this URI.
            let mut start = col;
            while start > 0
                && cells[usize::from(start - 1)].has_hyperlink
                && self.model.hyperlink_uri_at_cell(start - 1, row).as_deref() == Some(&url)
            {
                start -= 1;
            }
            let mut end = col + 1;
            while usize::from(end) < cells.len()
                && cells[usize::from(end)].has_hyperlink
                && self.model.hyperlink_uri_at_cell(end, row).as_deref() == Some(&url)
            {
                end += 1;
            }
            return Some(HoveredLink {
                url,
                row,
                cols: start..end,
            });
        }

        let (text, columns) = row_text_with_columns(&frame.rows[usize::from(row)]);
        scan_row_url(&text, &columns, col)
            .or_else(|| scan_row_file_path(&text, &columns, col))
            .map(|(url, cols)| HoveredLink { url, row, cols })
    }

    fn handle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        origin: Point<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left {
            self.left_button_down = false;
            if self.scrollbar_drag_offset.take().is_some() {
                cx.notify();
                return;
            }
        }
        // A press that went to the PTY gets its matching release even if
        // modes changed mid-drag; other buttons release per current mode.
        let route_release = if event.button == MouseButton::Left {
            self.reporting_drag
        } else {
            !event.modifiers.shift && self.model.mouse_tracking()
        };
        if route_release && let Some(button) = vt_mouse_button(event.button) {
            let (x, y) = self.device_position(event.position, origin, scale);
            self.model.send_mouse(
                &VtMouseInput {
                    action: VtMouseAction::Release,
                    button: Some(button),
                    mods: vt_mods(&event.modifiers),
                    x,
                    y,
                },
                false,
            );
        }
        if event.button == MouseButton::Left {
            self.reporting_drag = false;
            self.drag = None;
            if self.settings.copy_on_select && self.selection.is_some() {
                self.copy_selection(cx);
            }
        }
    }

    fn current_scrollbar_layout(&self) -> Option<ScrollbarLayout> {
        layout_scrollbar(
            self.settings.scrollbar_visible && self.scrollbar_visible,
            self.frame.as_ref()?.scrollbar,
            self.terminal_bounds?,
        )
    }

    fn scrollbar_to_pointer(&mut self, pointer_y: Pixels, grab_offset: f32) {
        let Some(scrollbar) = self.current_scrollbar_layout() else {
            return;
        };
        let travel = (scrollbar.slot.size.height - scrollbar.knob.size.height)
            .as_f32()
            .max(0.0);
        let fraction = if travel == 0.0 {
            0.0
        } else {
            ((pointer_y - scrollbar.slot.origin.y).as_f32() - grab_offset).clamp(0.0, travel)
                / travel
        };
        let frame_scrollbar = self.frame.as_ref().map(|frame| frame.scrollbar);
        let Some(frame_scrollbar) = frame_scrollbar else {
            return;
        };
        let max_offset = frame_scrollbar.total.saturating_sub(frame_scrollbar.len);
        self.model
            .scroll_viewport_to_row((fraction * max_offset as f32).round() as u64);
    }

    fn handle_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        origin: Point<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) {
        let line_height = self
            .cached_metrics
            .map(|metrics| metrics.line_height)
            .unwrap_or(px(16.));
        let lines = match event.delta {
            ScrollDelta::Pixels(delta) => {
                // Ghostty's AppKit surface doubles precise trackpad deltas.
                delta.y.as_f32() / line_height.as_f32() * self.settings.mouse_scroll_precision * 2.0
            }
            ScrollDelta::Lines(delta) => delta.y * self.settings.mouse_scroll_discrete,
        };
        self.wheel_accum += lines;
        let steps = self.wheel_accum.trunc() as i32;
        if steps == 0 {
            return;
        }
        self.wheel_accum -= steps as f32;

        // Shift-wheel always scrolls locally (mirrors the selection
        // override for reporting modes).
        let route = if event.modifiers.shift {
            WheelRoute::Viewport
        } else {
            self.model.wheel_route()
        };
        match route {
            WheelRoute::Report => {
                let (x, y) = self.device_position(event.position, origin, scale);
                let button = if steps > 0 {
                    VtMouseButton::WheelUp
                } else {
                    VtMouseButton::WheelDown
                };
                // Wheel reports are press-only events (xterm buttons 64/65).
                for _ in 0..steps.abs() {
                    self.model.send_mouse(
                        &VtMouseInput {
                            action: VtMouseAction::Press,
                            button: Some(button),
                            mods: vt_mods(&event.modifiers),
                            x,
                            y,
                        },
                        self.left_button_down,
                    );
                }
            }
            WheelRoute::ArrowKeys => {
                let key = if steps > 0 {
                    VtKey::ArrowUp
                } else {
                    VtKey::ArrowDown
                };
                let input = VtKeyInput {
                    action: VtKeyAction::Press,
                    key,
                    mods: 0,
                    consumed_mods: 0,
                    utf8: None,
                    unshifted_codepoint: 0,
                };
                for _ in 0..steps.abs() {
                    self.model.send_key(&input);
                }
            }
            WheelRoute::Viewport => {
                // Viewport delta: up (positive wheel) is negative rows.
                self.model
                    .scroll_viewport(VtScrollViewport::Delta(-(steps as isize)));
                self.reveal_scrollbar_for_user_scroll(cx);
                self.refresh_snapshot();
                cx.notify();
            }
            WheelRoute::None => {}
        }
    }

    fn extend_selection(&mut self, viewport_cell: (u16, u16)) {
        let Some(drag) = self.drag else {
            return;
        };
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let cell = (
            frame
                .scrollbar
                .offset
                .saturating_add(u64::from(viewport_cell.0)),
            viewport_cell.1,
        );
        self.selection = Some(match drag.mode {
            SelectMode::Cell => cell_selection(drag.anchor, cell, false),
            SelectMode::Block => cell_selection(drag.anchor, cell, true),
            SelectMode::Word => word_selection(
                frame,
                frame.scrollbar.offset,
                drag.anchor,
                cell,
                &self.settings.selection_word_chars,
            ),
            SelectMode::Line => line_selection(frame.cols, drag.anchor.0, cell.0),
        });
    }

    fn absolute_cell(&self, viewport_cell: (u16, u16)) -> (u64, u16) {
        (
            self.frame
                .as_ref()
                .map(|frame| frame.scrollbar.offset)
                .unwrap_or_default()
                .saturating_add(u64::from(viewport_cell.0)),
            viewport_cell.1,
        )
    }

    /// Viewport cell under a window position, clamped to the grid.
    fn grid_cell(&self, position: Point<Pixels>, origin: Point<Pixels>) -> (u16, u16) {
        let Some(metrics) = self.cached_metrics else {
            return (0, 0);
        };
        let (cols, rows) = self.model.size();
        let col = ((position.x - origin.x).as_f32() / metrics.cell_width.as_f32())
            .floor()
            .clamp(0., f32::from(cols.saturating_sub(1)));
        let row = ((position.y - origin.y).as_f32() / metrics.line_height.as_f32())
            .floor()
            .clamp(0., f32::from(rows.saturating_sub(1)));
        (row as u16, col as u16)
    }

    /// Window position -> DEVICE pixels relative to the grid origin,
    /// clamped inside the grid; matches the cell pixel sizes the model
    /// reported to the PTY so SGR-pixels reports and cell math line up.
    fn device_position(
        &self,
        position: Point<Pixels>,
        origin: Point<Pixels>,
        scale: f32,
    ) -> (f32, f32) {
        let Some(metrics) = self.cached_metrics else {
            return (0., 0.);
        };
        let (cols, rows) = self.model.size();
        let cell_width_px = ((metrics.cell_width.as_f32() * scale).round()).max(1.);
        let cell_height_px = ((metrics.line_height.as_f32() * scale).round()).max(1.);
        /*
        The renderer advances cells using the precise logical metrics, while
        the VT model must report whole device-pixel cell dimensions. Map the
        pointer proportionally from the rendered logical grid into that
        rounded device-pixel grid. Scaling the raw logical coordinate by the
        window scale instead makes every rounding difference accumulate across
        the row or column, so clicks drift farther from the painted cell toward
        the terminal's right or bottom edge.
        */
        let x = ((position.x - origin.x).as_f32() / metrics.cell_width.as_f32() * cell_width_px)
            .clamp(0., f32::from(cols) * cell_width_px - 1.);
        let y = ((position.y - origin.y).as_f32() / metrics.line_height.as_f32() * cell_height_px)
            .clamp(0., f32::from(rows) * cell_height_px - 1.);
        (x, y)
    }

    /// Build the frame layout for the element bounds: sync focus state and
    /// metrics/grid, reshape dirty rows, and lay out cursor/selection/marked
    /// text.
    fn prepaint_layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TerminalLayout {
        self.terminal_bounds = Some(bounds);
        // Focus edges send focus reports (mode 1004) and toggle the hollow
        // cursor; gpui refreshes the window on focus changes, so prepaint
        // always observes them.
        let focused = self.focus_handle.is_focused(window);
        if focused != self.focused {
            self.focused = focused;
            self.cursor_blink_visible = true;
            self.model.send_focus(focused);
            cx.emit(TerminalViewEvent::FocusChanged { focused });
        }

        let metrics = compute_cell_metrics(&self.font, window);
        if self.cached_metrics != Some(metrics) {
            self.cached_metrics = Some(metrics);
            self.row_cache.fill(None);
        }

        let scrollbar_width = if self.settings.scrollbar_visible {
            px(TERMINAL_SCROLLBAR_THICKNESS)
        } else {
            px(0.0)
        };
        let content_width = (bounds.size.width - scrollbar_width).max(metrics.cell_width);
        let cols = ((content_width / metrics.cell_width) as u16).max(1);
        let rows = ((bounds.size.height / metrics.line_height) as u16).max(1);
        let scale = window.scale_factor();
        let cell_width_px = ((metrics.cell_width.as_f32() * scale).round() as u32).max(1);
        let cell_height_px = ((metrics.line_height.as_f32() * scale).round() as u32).max(1);

        if self.frame.is_none() || (cols, rows) != self.model.size() {
            // Resize reflows the vt grid synchronously, so take the fresh
            // frame now instead of waiting for the SIGWINCH redraw wakeup.
            // Best-effort: the PTY side can only fail once the child is
            // gone, and the vt grid (which rendering reads) resizes first.
            let _ = self.model.resize(cols, rows, cell_width_px, cell_height_px);
            self.row_cache.clear();
            self.refresh_snapshot();
        }

        if self.update_scroll_button_visibility() {
            cx.notify();
        }

        let Some(frame) = &self.frame else {
            return TerminalLayout {
                metrics,
                background: gpui::black().into(),
                rows: Vec::new(),
                selection_spans: Vec::new(),
                search_spans: Vec::new(),
                link_underline: None,
                cursor: None,
                marked_text: None,
                scrollbar: None,
            };
        };

        // A failed snapshot after a grid change can leave the cache length
        // stale; re-align before filling so indexing below stays in bounds.
        if self.row_cache.len() != frame.rows.len() {
            self.row_cache = vec![None; frame.rows.len()];
        }
        for (index, row) in frame.rows.iter().enumerate() {
            if self.row_cache[index].is_none() {
                self.row_cache[index] = Some(Arc::new(build_row_layout(
                    row, frame, &self.font, metrics, window,
                )));
            }
        }

        TerminalLayout {
            metrics,
            background: rgb_to_hsla(frame.background),
            rows: self
                .row_cache
                .iter()
                .map(|row| row.clone().expect("row layout built above"))
                .collect(),
            selection_spans: layout_selection(
                self.selection,
                frame,
                self.settings.selection_background,
            ),
            search_spans: layout_search(self.search.as_ref(), frame),
            link_underline: self.hovered_link.as_ref().map(|link| {
                (
                    link.row,
                    CellSpan {
                        col: link.cols.start,
                        len: link.cols.end.saturating_sub(link.cols.start),
                        color: rgb_to_hsla(frame.foreground),
                    },
                )
            }),
            cursor: if self.settings.cursor_blink && self.focused && !self.cursor_blink_visible {
                None
            } else {
                layout_cursor(
                    self.cursor_shape,
                    self.settings.cursor_opacity,
                    self.settings.cursor_text,
                    self.focused,
                    frame,
                    &self.font,
                    metrics,
                    window,
                )
            },
            marked_text: layout_marked_text(self.marked_text.as_deref(), frame, &self.font, window),
            scrollbar: layout_scrollbar(
                self.settings.scrollbar_visible && self.scrollbar_visible,
                frame.scrollbar,
                bounds,
            ),
        }
    }
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

#[cfg(target_os = "macos")]
fn hide_mouse_cursor_until_mouse_moves() {
    unsafe extern "C" {
        fn GhostexGpuiTerminalHideMouseCursorUntilMouseMoves();
    }

    unsafe {
        GhostexGpuiTerminalHideMouseCursorUntilMouseMoves();
    }
}

#[cfg(not(target_os = "macos"))]
fn hide_mouse_cursor_until_mouse_moves() {}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .relative()
            .size_full()
            .can_drop(|value, _window, _cx| value.is::<ExternalPaths>())
            .on_drop(cx.listener(|_view, paths: &ExternalPaths, _window, cx| {
                cx.emit(TerminalViewEvent::PathsDropped(paths.paths().to_vec()));
            }))
            .child(TerminalElement::new(cx.entity()));

        // Deliberate, user-approved terminal controls overlay: only exact,
        // visible button rectangles overlap the terminal. They block terminal
        // clicks while allowing wheel/trackpad scroll through; there is no
        // transparent shield or broad hit-test routing.
        if self.scroll_button_visibility.bottom {
            root = root.child(terminal_scroll_button(
                TerminalScrollEdge::Bottom,
                self.scroll_button_visibility.bottom,
                cx,
            ));
        }
        if self.scroll_button_visibility.top {
            root = root.child(terminal_scroll_button(
                TerminalScrollEdge::Top,
                self.scroll_button_visibility.bottom,
                cx,
            ));
        }
        if self.agent_actions_visible {
            // Agent Actions is always present for the focused agent terminal.
            // Prompts stays directly to its left. Chat View joins the cluster
            // once the provider session identity can resolve a transcript.
            if self.chat_view_action_visible {
                root = root.child(terminal_agent_action_button(
                    TerminalAgentAction::ToggleChatView,
                    2,
                    TerminalAgentActionRow::Cluster,
                    cx,
                ));
            }
            root = root.child(terminal_agent_action_button(
                TerminalAgentAction::StashedPrompts,
                1,
                TerminalAgentActionRow::Cluster,
                cx,
            ));
            root = root.child(terminal_agent_action_button(
                TerminalAgentAction::ToggleMenu,
                0,
                TerminalAgentActionRow::Cluster,
                cx,
            ));
            if self.agent_actions_expanded {
                for (column_from_right, action) in [
                    TerminalAgentAction::AttachPath,
                    TerminalAgentAction::StashPrompt,
                    TerminalAgentAction::PromptEditor,
                    TerminalAgentAction::FullReload,
                    TerminalAgentAction::Fork,
                    TerminalAgentAction::DelayedActions,
                    TerminalAgentAction::Sleep,
                    TerminalAgentAction::Rename,
                ]
                .into_iter()
                .enumerate()
                {
                    root = root.child(terminal_agent_action_button(
                        action,
                        column_from_right,
                        TerminalAgentActionRow::Menu,
                        cx,
                    ));
                }
            }
        }

        root
    }
}

/// Which overlay row a button belongs to: the always-visible cluster
/// ([Chat View][Prompts][Agent Actions]) or the expanded menu bar below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalAgentActionRow {
    Cluster,
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalAgentAction {
    ToggleChatView,
    Rename,
    Sleep,
    DelayedActions,
    Fork,
    FullReload,
    PromptEditor,
    StashPrompt,
    StashedPrompts,
    AttachPath,
    ToggleMenu,
}

fn terminal_agent_action_button(
    action: TerminalAgentAction,
    column_from_right: usize,
    row: TerminalAgentActionRow,
    cx: &mut Context<TerminalView>,
) -> impl IntoElement {
    let (id, tooltip, hotkey_action_id) = match action {
        TerminalAgentAction::ToggleChatView => (
            "ghostex-terminal-toggle-chat-view",
            "Chat View",
            "toggleChatView",
        ),
        TerminalAgentAction::Rename => ("ghostex-terminal-rename", "Rename", "renameActiveSession"),
        TerminalAgentAction::Sleep => ("ghostex-terminal-sleep", "Sleep", "sleepFocusedSession"),
        TerminalAgentAction::DelayedActions => (
            "ghostex-terminal-delayed-actions",
            "Delayed Actions",
            "delayedSend",
        ),
        TerminalAgentAction::Fork => ("ghostex-terminal-fork", "Fork", "forkSession"),
        TerminalAgentAction::FullReload => (
            "ghostex-terminal-full-reload",
            "Full Reload",
            "reloadSession",
        ),
        TerminalAgentAction::PromptEditor => (
            "ghostex-terminal-prompt-editor",
            "Prompt Editor",
            "promptEditor",
        ),
        TerminalAgentAction::StashPrompt => (
            "ghostex-terminal-stash-prompt",
            "Stash Prompt",
            "stashPrompt",
        ),
        TerminalAgentAction::StashedPrompts => (
            "ghostex-terminal-stashed-prompts",
            "Prompts",
            "stashedPrompts",
        ),
        TerminalAgentAction::AttachPath => (
            "ghostex-terminal-attach-path",
            "Attach File or Folder",
            "attachFileOrFolder",
        ),
        TerminalAgentAction::ToggleMenu => (
            "ghostex-terminal-agent-actions",
            "Agent Actions",
            "toggleAgentActions",
        ),
    };
    let tooltip = terminal_overlay_tooltip(tooltip, hotkey_action_id);
    let right = TERMINAL_AGENT_ACTIONS_EDGE_INSET
        + column_from_right as f32 * (TERMINAL_SCROLL_BUTTON_SIZE + TERMINAL_BUTTON_GAP);
    let top = match row {
        TerminalAgentActionRow::Cluster => TERMINAL_AGENT_ACTIONS_EDGE_INSET,
        // The Agent Actions menu bar sits 13px below the cluster row.
        TerminalAgentActionRow::Menu => {
            TERMINAL_AGENT_ACTIONS_EDGE_INSET
                + TERMINAL_SCROLL_BUTTON_SIZE
                + TERMINAL_AGENT_ACTIONS_MENU_GAP
        }
    };
    terminal_overlay_button(id)
        .right(px(right))
        .top(px(top))
        .border_t_1()
        .border_b_1()
        .border_l_1()
        .when(column_from_right == 0, |this| this.border_r_1())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                match action {
                    TerminalAgentAction::ToggleChatView
                    | TerminalAgentAction::Rename
                    | TerminalAgentAction::Sleep
                    | TerminalAgentAction::DelayedActions
                    | TerminalAgentAction::Fork
                    | TerminalAgentAction::FullReload
                    | TerminalAgentAction::StashPrompt
                    | TerminalAgentAction::StashedPrompts => {
                        view.agent_actions_expanded = false;
                        let request = match action {
                            TerminalAgentAction::ToggleChatView => {
                                TerminalAgentActionRequest::ToggleChatView
                            }
                            TerminalAgentAction::Rename => TerminalAgentActionRequest::Rename,
                            TerminalAgentAction::Sleep => TerminalAgentActionRequest::Sleep,
                            TerminalAgentAction::DelayedActions => {
                                TerminalAgentActionRequest::DelayedActions
                            }
                            TerminalAgentAction::Fork => TerminalAgentActionRequest::Fork,
                            TerminalAgentAction::FullReload => {
                                TerminalAgentActionRequest::FullReload
                            }
                            TerminalAgentAction::StashPrompt => {
                                TerminalAgentActionRequest::StashPrompt
                            }
                            TerminalAgentAction::StashedPrompts => {
                                TerminalAgentActionRequest::StashedPrompts
                            }
                            TerminalAgentAction::PromptEditor
                            | TerminalAgentAction::AttachPath
                            | TerminalAgentAction::ToggleMenu => unreachable!(),
                        };
                        cx.emit(TerminalViewEvent::AgentActionRequested(request));
                        cx.notify();
                    }
                    TerminalAgentAction::PromptEditor => {
                        view.agent_actions_expanded = false;
                        view.focus_handle.focus(window, cx);
                        cx.emit(TerminalViewEvent::PromptEditorShortcutRequested);
                        cx.notify();
                    }
                    TerminalAgentAction::AttachPath => {
                        view.agent_actions_expanded = false;
                        cx.emit(TerminalViewEvent::AttachPathsRequested);
                        cx.notify();
                    }
                    TerminalAgentAction::ToggleMenu => {
                        view.agent_actions_expanded = !view.agent_actions_expanded;
                        cx.notify();
                    }
                }
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Below, move |window, cx| {
            Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(terminal_agent_action_icon(action))
}

fn terminal_agent_action_icon(action: TerminalAgentAction) -> AnyElement {
    match action {
        TerminalAgentAction::ToggleChatView => terminal_agent_action_svg(TERMINAL_CHAT_VIEW_ICON),
        TerminalAgentAction::Rename => terminal_agent_action_svg(TERMINAL_RENAME_ICON),
        TerminalAgentAction::Sleep => terminal_agent_action_svg(TERMINAL_SLEEP_ICON),
        TerminalAgentAction::DelayedActions => {
            terminal_agent_action_svg(TERMINAL_DELAYED_ACTIONS_ICON)
        }
        TerminalAgentAction::Fork => terminal_agent_action_svg(TERMINAL_FORK_ICON),
        TerminalAgentAction::FullReload => terminal_agent_action_svg(TERMINAL_FULL_RELOAD_ICON),
        TerminalAgentAction::StashPrompt => terminal_agent_action_svg(TERMINAL_STASH_PROMPT_ICON),
        TerminalAgentAction::StashedPrompts => {
            terminal_agent_action_svg(TERMINAL_STASHED_PROMPTS_ICON)
        }
        TerminalAgentAction::PromptEditor => svg()
            .size(px(14.0))
            .path(TERMINAL_PROMPT_EDITOR_ICON)
            .text_color(gpui::rgb(0xa6a6a6))
            .into_any_element(),
        TerminalAgentAction::AttachPath => svg()
            .size(px(14.0))
            .path(TERMINAL_ATTACH_PATH_ICON)
            .text_color(gpui::rgb(0xa6a6a6))
            .into_any_element(),
        TerminalAgentAction::ToggleMenu => terminal_agent_actions_glyph().into_any_element(),
    }
}

fn terminal_agent_action_svg(path: &'static str) -> AnyElement {
    svg()
        .size(px(14.0))
        .path(path)
        .text_color(gpui::rgb(0xa6a6a6))
        .into_any_element()
}

fn terminal_agent_actions_glyph() -> impl IntoElement {
    canvas(
        |_bounds, _window, _cx| {},
        |bounds, _state: (), window, _cx| {
            let center_y = bounds.top().as_f32() + bounds.size.height.as_f32() / 2.0;
            let center_x = bounds.left().as_f32() + bounds.size.width.as_f32() / 2.0;
            let color: Hsla = gpui::rgb(0xa6a6a6).into();
            for x in [center_x - 4.5, center_x, center_x + 4.5] {
                window.paint_quad(gpui::quad(
                    Bounds::centered_at(point(px(x), px(center_y)), size(px(2.25), px(2.25))),
                    px(1.125),
                    color,
                    px(0.0),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));
            }
        },
    )
}

fn terminal_overlay_button(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .absolute()
        .size(px(TERMINAL_SCROLL_BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .border_color(gpui::rgb(0x2a2a2a))
        .bg(gpui::rgb(0x101010))
        .text_color(gpui::rgb(0xa6a6a6))
        .cursor_default()
        .hover(|this| this.bg(gpui::rgb(TERMINAL_SCROLL_BUTTON_HOVER_BACKGROUND_RGB)))
        .block_mouse_except_scroll()
        .shadow(vec![
            BoxShadow::new(
                px(0.0),
                px(10.0),
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.32,
                }
                .into(),
            )
            .blur_radius(px(22.0)),
        ])
}

fn terminal_scroll_button(
    edge: TerminalScrollEdge,
    bottom_button_visible: bool,
    cx: &mut Context<TerminalView>,
) -> impl IntoElement {
    let (id, tooltip, hotkey_action_id, bottom) = match edge {
        TerminalScrollEdge::Top => (
            "ghostex-terminal-scroll-to-top",
            "Scroll terminal to top",
            "scrollTerminalToTop",
            TERMINAL_ACTION_BUTTON_EDGE_INSET + TERMINAL_SCROLL_BUTTON_SIZE + TERMINAL_BUTTON_GAP,
        ),
        TerminalScrollEdge::Bottom => (
            "ghostex-terminal-scroll-to-bottom",
            "Scroll terminal to bottom",
            "scrollTerminalToBottom",
            TERMINAL_ACTION_BUTTON_EDGE_INSET,
        ),
    };
    let tooltip = terminal_overlay_tooltip(tooltip, hotkey_action_id);

    terminal_overlay_button(id)
        .right(px(TERMINAL_ACTION_BUTTON_EDGE_INSET))
        .bottom(px(bottom))
        .border_l_1()
        .border_r_1()
        .border_t_1()
        .when(
            edge == TerminalScrollEdge::Bottom || !bottom_button_visible,
            |this| this.border_b_1(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                view.scroll_to_edge(edge, window, cx);
            }),
        )
        .managed_tooltip_with_placement(ManagedTooltipPlacement::Below, move |window, cx| {
            Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(terminal_scroll_button_glyph(edge))
}

fn terminal_overlay_tooltip(label: &str, hotkey_action_id: &str) -> String {
    match terminal_overlay_hotkey_label(hotkey_action_id) {
        Some(hotkey) => format!("{label} ({hotkey})"),
        None => label.to_string(),
    }
}

fn terminal_overlay_hotkey_label(action_id: &str) -> Option<String> {
    let default_key = match action_id {
        "renameActiveSession" => "cmd+r",
        "sleepFocusedSession" => "alt+shift+s",
        "delayedSend" => "ctrl+shift+s",
        "forkSession" => "ctrl+shift+f",
        "reloadSession" => "ctrl+shift+r",
        "promptEditor" => "ctrl+g",
        "attachFileOrFolder"
        | "stashPrompt"
        | "stashedPrompts"
        | "toggleAgentActions"
        | "scrollTerminalToTop"
        | "scrollTerminalToBottom" => "",
        _ => return None,
    };
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let persisted_key = snapshot
        .object()
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
        .and_then(|hotkeys| hotkeys.get(action_id))
        .and_then(serde_json::Value::as_str);
    let key = terminal_overlay_platform_hotkey(action_id, persisted_key.unwrap_or(default_key));
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(
        key.split(' ')
            .map(terminal_overlay_hotkey_chord_label)
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn terminal_overlay_platform_hotkey<'a>(action_id: &str, key: &'a str) -> &'a str {
    if cfg!(target_os = "macos") {
        return key;
    }
    let defaults = match action_id {
        "delayedSend" => Some(("ctrl+shift+s", "cmd+alt+s")),
        "forkSession" => Some(("ctrl+shift+f", "cmd+alt+f")),
        "reloadSession" => Some(("ctrl+shift+r", "cmd+alt+r")),
        "promptEditor" => Some(("ctrl+g", "cmd+shift+g")),
        _ => None,
    };
    match defaults {
        Some((mac_default, windows_linux_default))
            if key.trim().eq_ignore_ascii_case(mac_default) =>
        {
            windows_linux_default
        }
        _ => key,
    }
}

fn terminal_overlay_hotkey_chord_label(chord: &str) -> String {
    let parts = chord
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_primary_modifier = parts
        .iter()
        .any(|part| matches!(part.as_str(), "cmd" | "command" | "meta"));
    let has_option_modifier = parts
        .iter()
        .any(|part| matches!(part.as_str(), "alt" | "opt" | "option"));
    let mut labels = Vec::with_capacity(parts.len());
    for part in parts {
        let label = if cfg!(target_os = "macos") {
            match part.as_str() {
                "cmd" | "command" | "meta" => "⌘".to_string(),
                "ctrl" | "control" => "⌃".to_string(),
                "alt" | "opt" | "option" => "⌥".to_string(),
                "shift" => "⇧".to_string(),
                "up" | "arrowup" => "↑".to_string(),
                "right" | "arrowright" => "→".to_string(),
                "down" | "arrowdown" => "↓".to_string(),
                "left" | "arrowleft" => "←".to_string(),
                "tab" => "Tab".to_string(),
                "ß" if has_option_modifier => "S".to_string(),
                value if value.len() == 1 => value.to_uppercase(),
                value if value.starts_with('f') => value.to_uppercase(),
                value => value.to_string(),
            }
        } else {
            match part.as_str() {
                "cmd" | "command" | "meta" => "Ctrl".to_string(),
                "ctrl" | "control" if has_primary_modifier => "Alt".to_string(),
                "ctrl" | "control" => "Ctrl".to_string(),
                "alt" | "opt" | "option" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                "up" | "arrowup" => "↑".to_string(),
                "right" | "arrowright" => "→".to_string(),
                "down" | "arrowdown" => "↓".to_string(),
                "left" | "arrowleft" => "←".to_string(),
                "tab" => "Tab".to_string(),
                "ß" if has_option_modifier => "S".to_string(),
                value if value.len() == 1 => value.to_uppercase(),
                value if value.starts_with('f') => value.to_uppercase(),
                value => value.to_string(),
            }
        };
        if labels.last() != Some(&label) {
            labels.push(label);
        }
    }
    labels.join(if cfg!(target_os = "macos") { "" } else { " + " })
}

fn terminal_scroll_button_glyph(edge: TerminalScrollEdge) -> impl IntoElement {
    canvas(
        move |_bounds, _window, _cx| {},
        move |bounds, _state: (), window, _cx| {
            let center_x = bounds.left().as_f32() + bounds.size.width.as_f32() / 2.0;
            let center_y = bounds.top().as_f32() + bounds.size.height.as_f32() / 2.0;
            let (left_y, center_y, right_y) = match edge {
                TerminalScrollEdge::Top => (center_y + 2.25, center_y - 3.0, center_y + 2.25),
                TerminalScrollEdge::Bottom => (center_y - 2.25, center_y + 3.0, center_y - 2.25),
            };
            let points = [
                point(px(center_x - 4.5), px(left_y)),
                point(px(center_x), px(center_y)),
                point(px(center_x + 4.5), px(right_y)),
            ];
            let color: Hsla = gpui::rgb(0xa6a6a6).into();

            // Separate segments plus filled endpoint circles reproduce the
            // deprecated NSBezierPath's round line caps and round line join.
            let mut stroke = gpui::PathBuilder::stroke(px(1.65));
            stroke.move_to(points[0]);
            stroke.line_to(points[1]);
            stroke.move_to(points[1]);
            stroke.line_to(points[2]);
            if let Ok(path) = stroke.build() {
                window.paint_path(path, color);
            }

            let radius = 1.65 / 2.0;
            let mut caps = gpui::PathBuilder::fill();
            for center in points {
                caps.move_to(point(px(center.x.as_f32() + radius), center.y));
                caps.arc_to(
                    point(px(radius), px(radius)),
                    px(0.0),
                    false,
                    true,
                    point(px(center.x.as_f32() - radius), center.y),
                );
                caps.arc_to(
                    point(px(radius), px(radius)),
                    px(0.0),
                    false,
                    true,
                    point(px(center.x.as_f32() + radius), center.y),
                );
                caps.close();
            }
            if let Ok(path) = caps.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size_full()
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// IME integration: composition text renders as the `marked_text` overlay at
/// the cursor cell, commits write UTF-8 straight to the PTY, and the
/// candidate window anchors to the cursor cell via `bounds_for_range`. The
/// terminal has no addressable document, so ranges are token values.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedTextServiceQuery",
            serde_json::json!({
                "focused": self.focus_handle.is_focused(window),
                "query": "textForRange",
                "rangeEnd": range.end,
                "rangeStart": range.start,
                "result": "none",
            }),
        );
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedTextServiceQuery",
            serde_json::json!({
                "focused": self.focus_handle.is_focused(window),
                "query": "selectedTextRange",
                "rangeEnd": 0,
                "rangeStart": 0,
            }),
        );
        // The terminal has no editable backing document. This token range is
        // sufficient for IME composition and keeps the platform from treating
        // the terminal's scrollback as an editable text buffer.
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedUnmarkText",
            serde_json::json!({
                "focused": self.focus_handle.is_focused(window),
                "hadMarkedText": self.marked_text.is_some(),
            }),
        );
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedReplaceText",
            serde_json::json!({
                "focused": self.focus_handle.is_focused(window),
                "rangeEnd": range.as_ref().map(|range| range.end),
                "rangeStart": range.as_ref().map(|range| range.start),
                "text": crate::support_logs::temporary_fluid_voice_text_shape(text),
            }),
        );
        self.commit_ime_text(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::support_logs::append_temporary(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.compositedReplaceMarkedText",
            serde_json::json!({
                "focused": self.focus_handle.is_focused(window),
                "rangeEnd": range.as_ref().map(|range| range.end),
                "rangeStart": range.as_ref().map(|range| range.start),
                "selectedRangeEnd": new_selected_range.as_ref().map(|range| range.end),
                "selectedRangeStart": new_selected_range.as_ref().map(|range| range.start),
                "text": crate::support_logs::temporary_fluid_voice_text_shape(new_text),
            }),
        );
        if self.input_suppressed {
            if self.marked_text.take().is_some() {
                cx.notify();
            }
            return;
        }
        self.marked_text = if new_text.is_empty() {
            None
        } else {
            Some(new_text.to_string())
        };
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let metrics = self.cached_metrics?;
        let (col, row) = self.frame.as_ref()?.cursor?;
        let origin = element_bounds.origin
            + point(
                metrics.cell_width * (f32::from(col) + range_utf16.start as f32),
                metrics.line_height * f32::from(row),
            );
        Some(Bounds::new(
            origin,
            size(metrics.cell_width, metrics.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// gpui element that fills its bounds with the terminal grid.
pub struct TerminalElement {
    terminal: Entity<TerminalView>,
}

impl TerminalElement {
    pub fn new(terminal: Entity<TerminalView>) -> Self {
        Self { terminal }
    }

    /// Draws the configured background image over the pane's background fill and
    /// under the cell backgrounds. Cells only paint a quad when they carry an
    /// explicit background, so the image stays visible behind default-background
    /// text while selections and TUI colors still cover it.
    fn paint_background_image(
        &self,
        bounds: Bounds<Pixels>,
        background: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(settings) = self.terminal.read(cx).settings.background_image.clone() else {
            return;
        };
        let Some(image) = background_image_for_path(settings.path.as_path(), cx) else {
            return;
        };

        let image_bounds =
            background_image_bounds(bounds, image.size(0), settings.fit, window.scale_factor());
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window
                .paint_image(image_bounds, Corners::default(), image, 0, false)
                .ok();
        });

        // gpui's element opacity is crate-private, so dim by laying the pane
        // background back over the image at the inverse alpha.
        let opacity = settings.opacity.clamp(0.0, 1.0);
        if opacity < 1.0 {
            window.paint_quad(fill(
                bounds,
                Hsla {
                    a: 1.0 - opacity,
                    ..background
                },
            ));
        }
    }

    /// Register this frame's input surfaces: the IME input handler on the
    /// focus handle, key listeners on this dispatch node (focused path
    /// only), and window mouse listeners scoped by the hitbox.
    fn register_input(
        &self,
        bounds: Bounds<Pixels>,
        hitbox: &Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let entity = self.terminal.clone();
        let focus_handle = entity.read(cx).focus_handle.clone();
        let cursor_style = if entity.read(cx).hovered_link.is_some() {
            CursorStyle::PointingHand
        } else {
            // Managed libghostty panes deliberately keep the normal pointer
            // over terminal cells; text selection remains drag-based.
            CursorStyle::Arrow
        };
        window.set_cursor_style(cursor_style, hitbox);
        /*
        GPUI Component's Root binds Tab and Shift+Tab to focus traversal before
        raw key listeners run. Mark the exact focused terminal dispatch node so
        the app keymap can disable those Root bindings here; the ordinary key
        listener below then sends both keys through libghostty's encoder.
        */
        window.set_key_context(
            KeyContext::parse(TERMINAL_KEY_CONTEXT)
                .expect("static terminal key context must be valid"),
        );
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, entity.clone()),
            cx,
        );

        window.on_action(TypeId::of::<TerminalContextMenuCopy>(), {
            let entity = entity.clone();
            move |_action, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                entity.update(cx, |view, cx| view.copy_selection(cx));
            }
        });
        window.on_action(TypeId::of::<TerminalContextMenuPaste>(), {
            let entity = entity.clone();
            move |_action, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                entity.update(cx, |_view, cx| cx.emit(TerminalViewEvent::PasteRequested));
            }
        });

        window.on_key_event({
            let entity = entity.clone();
            move |event: &KeyDownEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                entity.update(cx, |view, cx| view.handle_key_down(event, cx));
            }
        });
        window.on_modifiers_changed({
            let entity = entity.clone();
            move |event, _window, cx| {
                entity.update(cx, |view, _cx| view.handle_modifiers_changed(event));
            }
        });
        window.on_key_event({
            let entity = entity.clone();
            move |event: &KeyUpEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                entity.update(cx, |view, _cx| view.handle_key_up(event));
            }
        });

        let origin = bounds.origin;
        let scale = window.scale_factor();
        window.on_mouse_event({
            let entity = entity.clone();
            let hitbox = hitbox.clone();
            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                    return;
                }
                entity.update(cx, |view, cx| {
                    view.handle_mouse_down(event, origin, scale, window, cx)
                });
            }
        });
        // Drags leave the hitbox, so move/up listen window-wide and the view
        // gates on its own drag state.
        window.on_mouse_event({
            let entity = entity.clone();
            let hitbox = hitbox.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let hovered = hitbox.is_hovered(window);
                entity.update(cx, |view, cx| {
                    view.handle_mouse_move(event, origin, scale, hovered, cx)
                });
            }
        });
        window.on_mouse_event({
            let entity = entity.clone();
            move |event: &MouseUpEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                entity.update(cx, |view, cx| {
                    view.handle_mouse_up(event, origin, scale, cx)
                });
            }
        });
        window.on_mouse_event({
            let entity = entity.clone();
            let hitbox = hitbox.clone();
            move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !hitbox.should_handle_scroll(window) {
                    return;
                }
                entity.update(cx, |view, cx| view.handle_wheel(event, origin, scale, cx));
            }
        });
    }
}

impl IntoElement for TerminalElement {
    type Element = TerminalElement;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Prepaint output: the frame layout plus the hitbox that scopes mouse
/// listeners and the cursor style to this element's bounds.
pub struct TerminalPrepaint {
    layout: TerminalLayout,
    hitbox: Hitbox,
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size::full(),
            flex_shrink: 1.,
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let layout = self
            .terminal
            .update(cx, |view, cx| view.prepaint_layout(bounds, window, cx));
        // The focus handle attaches to this element's dispatch node so key
        // events only dispatch here while this terminal is focused; an
        // unfocused terminal never sees (or swallows) keys.
        let focus_handle = self.terminal.read(cx).focus_handle.clone();
        window.set_focus_handle(&focus_handle, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        TerminalPrepaint { layout, hitbox }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.register_input(bounds, &prepaint.hitbox, window, cx);
        let layout = &prepaint.layout;
        let origin = bounds.origin;
        let cell_width = layout.metrics.cell_width;
        let line_height = layout.metrics.line_height;
        let span_bounds = |row: u16, span_col: u16, span_len: u16| {
            let x = origin.x + cell_width * f32::from(span_col);
            let y = origin.y + line_height * f32::from(row);
            Bounds::new(
                point(x.floor(), y),
                size((cell_width * f32::from(span_len)).ceil(), line_height),
            )
        };

        window.paint_quad(fill(bounds, layout.background));
        self.paint_background_image(bounds, layout.background, window, cx);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (row, layout_row) in layout.rows.iter().enumerate() {
                for span in &layout_row.bg_spans {
                    window.paint_quad(fill(
                        span_bounds(row as u16, span.col, span.len),
                        span.color,
                    ));
                }
            }

            for (row, span) in &layout.selection_spans {
                window.paint_quad(fill(span_bounds(*row, span.col, span.len), span.color));
            }

            for (row, span) in &layout.search_spans {
                window.paint_quad(fill(span_bounds(*row, span.col, span.len), span.color));
            }

            for (row, layout_row) in layout.rows.iter().enumerate() {
                let y = origin.y + line_height * (row as f32);
                for run in &layout_row.runs {
                    let run_origin = point(origin.x + cell_width * f32::from(run.col), y);
                    let _ = run.shaped.paint(
                        run_origin,
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                for span in &layout_row.overline_spans {
                    let mut overline = span_bounds(row as u16, span.col, span.len);
                    overline.size.height = px(1.);
                    window.paint_quad(fill(overline, span.color));
                }
            }

            if let Some((row, span)) = &layout.link_underline {
                // Hover underline for the link under the pointer, painted
                // over the glyphs like overlines so it reads on any cell bg.
                let mut underline = span_bounds(*row, span.col, span.len);
                underline.origin.y = underline.origin.y + line_height - px(1.);
                underline.size.height = px(1.);
                window.paint_quad(fill(underline, span.color));
            }

            if let Some(marked) = &layout.marked_text {
                let marked_origin = point(
                    origin.x + cell_width * f32::from(marked.col),
                    origin.y + line_height * f32::from(marked.row),
                );
                window.paint_quad(fill(
                    Bounds::new(marked_origin, size(marked.shaped.width(), line_height)),
                    marked.background,
                ));
                let _ = marked.shaped.paint(
                    marked_origin,
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }

            if let Some(cursor) = &layout.cursor {
                let cell_origin = point(
                    origin.x + cell_width * f32::from(cursor.col),
                    origin.y + line_height * f32::from(cursor.row),
                );
                let cursor_bounds = match cursor.shape {
                    TerminalCursorShape::Block => Bounds::new(
                        cell_origin,
                        size(cell_width * f32::from(cursor.width_cells), line_height),
                    ),
                    TerminalCursorShape::Bar => Bounds::new(cell_origin, size(px(2.), line_height)),
                    TerminalCursorShape::Underline => Bounds::new(
                        point(cell_origin.x, cell_origin.y + line_height - px(2.)),
                        size(cell_width * f32::from(cursor.width_cells), px(2.)),
                    ),
                };
                if cursor.hollow {
                    window.paint_quad(outline(cursor_bounds, cursor.color, BorderStyle::Solid));
                } else {
                    window.paint_quad(fill(cursor_bounds, cursor.color));
                }
                if let Some(overlay) = &cursor.overlay {
                    let _ =
                        overlay.paint(cell_origin, line_height, TextAlign::Left, None, window, cx);
                }
            }

            if let Some(scrollbar) = &layout.scrollbar {
                window.paint_quad(fill(scrollbar.slot, scrollbar.slot_color));
                window.paint_quad(fill(scrollbar.knob, scrollbar.knob_color));
            }
        });
    }
}

fn base_font(config: &TerminalFontConfig, bold: bool, italic: bool) -> Font {
    let mut font = gpui::font(config.family.clone());
    font.weight = if bold {
        FontWeight::BOLD
    } else {
        config.weight
    };
    font.style = if italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    font
}

fn compute_cell_metrics(config: &TerminalFontConfig, window: &Window) -> CellMetrics {
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&base_font(config, false, false));
    let cell_width = config.cell_width_adjustment.apply(
        text_system
            .advance(font_id, config.size, 'm')
            .expect("terminal font advance for 'm'")
            .width,
    );
    // Terminal cell height: natural font extent, matching how monospace
    // grids are conventionally sized. gpui font metrics follow the font-kit
    // sign convention where descent is negative (below baseline), so the
    // extent is ascent plus |descent|. JetBrains Mono has no line gap;
    // revisit with real font config sync in P1e.
    let ascent = text_system.ascent(font_id, config.size).as_f32();
    let descent = text_system.descent(font_id, config.size).as_f32();
    let line_height = config
        .cell_height_adjustment
        .apply(px(ascent + descent.abs()).ceil());
    CellMetrics {
        cell_width,
        line_height,
    }
}

fn rgb_to_hsla(rgb: Rgb) -> Hsla {
    Rgba {
        r: f32::from(rgb.r) / 255.,
        g: f32::from(rgb.g) / 255.,
        b: f32::from(rgb.b) / 255.,
        a: 1.,
    }
    .into()
}

fn blend_rgb(a: Rgb, b: Rgb, factor: f32) -> Rgb {
    let mix = |x: u8, y: u8| -> u8 {
        (f32::from(x) + (f32::from(y) - f32::from(x)) * factor).round() as u8
    };
    Rgb {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
    }
}

/// Regular files only, and bounded on both ends so a mistyped path pointing at
/// a device node, a huge file, or a decompression-bomb image cannot stall or
/// exhaust the app: the source read and the decoded pixel count are capped.
const TERMINAL_BACKGROUND_IMAGE_MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
/// 36 MP comfortably covers 8K wallpapers (7680x4320 ≈ 33 MP).
const TERMINAL_BACKGROUND_IMAGE_MAX_PIXELS: u64 = 36_000_000;

/// Background images, keyed by path. Paint never touches the filesystem: a
/// cache miss records `Loading`, kicks decode onto the background executor, and
/// paints nothing this frame; the completion writes `Ready`/`Failed` back and
/// refreshes windows so panes pick the texture up on the next frame. A path
/// that fails caches `Failed` so a bad path is not retried per frame.
enum TerminalBackgroundImageState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

#[derive(Default)]
struct TerminalBackgroundImageCache {
    entries: HashMap<PathBuf, TerminalBackgroundImageState>,
}

impl Global for TerminalBackgroundImageCache {}

fn background_image_for_path(path: &Path, cx: &mut App) -> Option<Arc<RenderImage>> {
    match cx
        .try_global::<TerminalBackgroundImageCache>()
        .and_then(|cache| cache.entries.get(path))
    {
        Some(TerminalBackgroundImageState::Ready(image)) => return Some(image.clone()),
        Some(TerminalBackgroundImageState::Loading)
        | Some(TerminalBackgroundImageState::Failed) => return None,
        None => {}
    }

    cx.default_global::<TerminalBackgroundImageCache>()
        .entries
        .insert(path.to_path_buf(), TerminalBackgroundImageState::Loading);

    let owned_path = path.to_path_buf();
    let decode = cx.background_executor().spawn(async move {
        let decoded = decode_background_image(&owned_path);
        (owned_path, decoded)
    });
    cx.spawn(async move |cx| {
        let (path, decoded) = decode.await;
        cx.update(|cx| {
            let state = match decoded {
                Some(image) => TerminalBackgroundImageState::Ready(image),
                None => TerminalBackgroundImageState::Failed,
            };
            cx.default_global::<TerminalBackgroundImageCache>()
                .entries
                .insert(path, state);
            cx.refresh_windows();
        });
    })
    .detach();
    None
}

fn decode_background_image(path: &Path) -> Option<Arc<RenderImage>> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > TERMINAL_BACKGROUND_IMAGE_MAX_SOURCE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    // Reject oversized images from the header alone, before full decode.
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes.as_slice()))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if u64::from(width) * u64::from(height) > TERMINAL_BACKGROUND_IMAGE_MAX_PIXELS {
        return None;
    }
    let mut data = image::load_from_memory(&bytes).ok()?.into_rgba8();
    // gpui uploads sprite atlas tiles as BGRA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(data)])))
}

/// Where to draw the image inside `container`. Oversized results are expected for
/// `Cover`; the caller clips with a content mask.
fn background_image_bounds(
    container: Bounds<Pixels>,
    image_size: Size<DevicePixels>,
    fit: TerminalBackgroundImageFit,
    scale_factor: f32,
) -> Bounds<Pixels> {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let natural_width = image_size.width.0 as f32 / scale;
    let natural_height = image_size.height.0 as f32 / scale;
    if natural_width <= 0.0 || natural_height <= 0.0 {
        return container;
    }

    let container_width = f32::from(container.size.width);
    let container_height = f32::from(container.size.height);
    let (width, height) = match fit {
        TerminalBackgroundImageFit::Stretch => (container_width, container_height),
        TerminalBackgroundImageFit::Natural => (natural_width, natural_height),
        TerminalBackgroundImageFit::Contain | TerminalBackgroundImageFit::Cover => {
            let scale_x = container_width / natural_width;
            let scale_y = container_height / natural_height;
            let factor = if matches!(fit, TerminalBackgroundImageFit::Contain) {
                scale_x.min(scale_y)
            } else {
                scale_x.max(scale_y)
            };
            (natural_width * factor, natural_height * factor)
        }
    };

    Bounds::new(
        point(
            container.origin.x + px((container_width - width) / 2.0),
            container.origin.y + px((container_height - height) / 2.0),
        ),
        size(px(width), px(height)),
    )
}

/// Effective cell colors after defaults and attribute resolution: `inverse`
/// arrives un-applied from the snapshot and is swapped here; `faint` dims the
/// foreground toward the effective background. Returns `(fg, Option<bg>)`
/// where `None` bg means the element's default background quad shows through.
fn resolve_cell_colors(cell: &SnapshotCell, frame: &TerminalSnapshot) -> (Rgb, Option<Rgb>) {
    let mut fg = cell.fg.unwrap_or(frame.foreground);
    let mut bg = cell.bg;
    if cell.inverse {
        let inverted_bg = fg;
        fg = bg.unwrap_or(frame.background);
        bg = Some(inverted_bg);
    }
    if cell.faint {
        fg = blend_rgb(fg, bg.unwrap_or(frame.background), 0.5);
    }
    (fg, bg)
}

fn cell_text_run(cell: &SnapshotCell, fg: Rgb, len: usize, config: &TerminalFontConfig) -> TextRun {
    let underline = match cell.underline {
        CellUnderline::None => None,
        // gpui underlines are plain or wavy; Double/Dotted/Dashed render as
        // Single for now (P1e parity polish).
        CellUnderline::Curly => Some(GpuiUnderlineStyle {
            thickness: px(1.),
            color: Some(rgb_to_hsla(cell.underline_color.unwrap_or(fg))),
            wavy: true,
        }),
        CellUnderline::Single
        | CellUnderline::Double
        | CellUnderline::Dotted
        | CellUnderline::Dashed => Some(GpuiUnderlineStyle {
            thickness: px(1.),
            color: Some(rgb_to_hsla(cell.underline_color.unwrap_or(fg))),
            wavy: false,
        }),
    };
    let strikethrough = cell.strikethrough.then(|| StrikethroughStyle {
        thickness: px(1.),
        color: Some(rgb_to_hsla(fg)),
    });
    TextRun {
        len,
        font: base_font(config, cell.bold, cell.italic),
        color: rgb_to_hsla(fg),
        // Backgrounds paint as merged quads, not text decorations.
        background_color: None,
        underline,
        strikethrough,
    }
}

/// Whether a cell contributes no glyphs or text decorations. Its background
/// still paints via bg spans.
fn cell_is_blank(cell: &SnapshotCell) -> bool {
    cell.invisible
        || (cell.base == ' '
            && cell.combining.is_none()
            && cell.underline == CellUnderline::None
            && !cell.strikethrough)
}

fn push_span(spans: &mut Vec<CellSpan>, col: u16, color: Hsla) {
    if let Some(last) = spans.last_mut()
        && last.color == color
        && last.col + last.len == col
    {
        last.len += 1;
        return;
    }
    spans.push(CellSpan { col, len: 1, color });
}

/// Shape one row into cached layout: merged background/overline spans plus
/// batched same-style text runs with forced cell advance.
fn build_row_layout(
    row: &SnapshotRow,
    frame: &TerminalSnapshot,
    config: &TerminalFontConfig,
    metrics: CellMetrics,
    window: &mut Window,
) -> RowLayout {
    let mut bg_spans: Vec<CellSpan> = Vec::new();
    let mut overline_spans: Vec<CellSpan> = Vec::new();
    let mut runs: Vec<PositionedRun> = Vec::new();

    let mut batch_col: u16 = 0;
    let mut batch_cells: u16 = 0;
    let mut batch_text = String::new();
    let mut batch_run: Option<TextRun> = None;

    let flush = |batch_run: &mut Option<TextRun>,
                 batch_text: &mut String,
                 batch_col: u16,
                 force_width: Option<Pixels>,
                 runs: &mut Vec<PositionedRun>,
                 window: &mut Window| {
        let Some(mut run) = batch_run.take() else {
            return;
        };
        run.len = batch_text.len();
        let shaped = window.text_system().shape_line(
            SharedString::from(std::mem::take(batch_text)),
            config.size,
            std::slice::from_ref(&run),
            force_width,
        );
        runs.push(PositionedRun {
            col: batch_col,
            shaped,
        });
    };

    for (col, cell) in row.cells.iter().enumerate() {
        let col = col as u16;
        let (fg, bg) = resolve_cell_colors(cell, frame);

        // Spacer cells carry the wide char's style: include their background
        // so wide-cell backgrounds cover both columns, but never their text.
        if let Some(bg) = bg {
            push_span(&mut bg_spans, col, rgb_to_hsla(bg));
        }
        if cell.overline {
            push_span(&mut overline_spans, col, rgb_to_hsla(fg));
        }
        match cell.width {
            VtCellWide::SpacerTail | VtCellWide::SpacerHead => continue,
            VtCellWide::Narrow | VtCellWide::Wide => {}
        }
        if cell_is_blank(cell) {
            continue;
        }

        let mut cell_text = String::new();
        cell_text.push(cell.base);
        if let Some(combining) = &cell.combining {
            cell_text.push_str(combining);
        }
        let run = cell_text_run(cell, fg, cell_text.len(), config);

        if cell.width == VtCellWide::Wide {
            // Wide glyphs take two columns; forced single-cell advance would
            // misplace anything batched after them, so they run alone.
            flush(
                &mut batch_run,
                &mut batch_text,
                batch_col,
                Some(metrics.cell_width),
                &mut runs,
                window,
            );
            let shaped = window.text_system().shape_line(
                SharedString::from(cell_text),
                config.size,
                &[run],
                None,
            );
            runs.push(PositionedRun { col, shaped });
            continue;
        }

        let appendable = batch_run.as_ref().is_some_and(|batch| {
            batch.font == run.font
                && batch.color == run.color
                && batch.underline == run.underline
                && batch.strikethrough == run.strikethrough
        }) && batch_col + batch_cells == col;
        if !appendable {
            flush(
                &mut batch_run,
                &mut batch_text,
                batch_col,
                Some(metrics.cell_width),
                &mut runs,
                window,
            );
            batch_col = col;
            batch_cells = 0;
            batch_run = Some(run);
        }
        batch_text.push_str(&cell_text);
        batch_cells += 1;
    }
    flush(
        &mut batch_run,
        &mut batch_text,
        batch_col,
        Some(metrics.cell_width),
        &mut runs,
        window,
    );

    RowLayout {
        bg_spans,
        overline_spans,
        runs,
    }
}

/// gpui modifiers -> vt modifier bitmask (sides unavailable from gpui).
fn vt_mods(modifiers: &Modifiers) -> VtMods {
    let mut mods: VtMods = 0;
    if modifiers.shift {
        mods |= ffi::GHOSTTY_MODS_SHIFT;
    }
    if modifiers.control {
        mods |= ffi::GHOSTTY_MODS_CTRL;
    }
    if modifiers.alt {
        mods |= ffi::GHOSTTY_MODS_ALT;
    }
    if modifiers.platform {
        mods |= ffi::GHOSTTY_MODS_SUPER;
    }
    mods
}

fn vt_mouse_button(button: MouseButton) -> Option<VtMouseButton> {
    match button {
        MouseButton::Left => Some(VtMouseButton::Left),
        MouseButton::Right => Some(VtMouseButton::Right),
        MouseButton::Middle => Some(VtMouseButton::Middle),
        _ => None,
    }
}

/// gpui keystroke -> vt physical key + unshifted codepoint. gpui reports
/// layout-resolved key names (letters already unshifted; shifted symbols
/// arrive shifted, a known encoding approximation), so single characters
/// map by identity and named keys by table. Unknown keys stay Unidentified
/// and encode through their codepoint/text.
fn keystroke_vt_key(keystroke: &Keystroke) -> (VtKey, u32) {
    let key = keystroke.key.as_str();
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let vt = match c.to_ascii_lowercase() {
            'a' => VtKey::A,
            'b' => VtKey::B,
            'c' => VtKey::C,
            'd' => VtKey::D,
            'e' => VtKey::E,
            'f' => VtKey::F,
            'g' => VtKey::G,
            'h' => VtKey::H,
            'i' => VtKey::I,
            'j' => VtKey::J,
            'k' => VtKey::K,
            'l' => VtKey::L,
            'm' => VtKey::M,
            'n' => VtKey::N,
            'o' => VtKey::O,
            'p' => VtKey::P,
            'q' => VtKey::Q,
            'r' => VtKey::R,
            's' => VtKey::S,
            't' => VtKey::T,
            'u' => VtKey::U,
            'v' => VtKey::V,
            'w' => VtKey::W,
            'x' => VtKey::X,
            'y' => VtKey::Y,
            'z' => VtKey::Z,
            '0' => VtKey::Digit0,
            '1' => VtKey::Digit1,
            '2' => VtKey::Digit2,
            '3' => VtKey::Digit3,
            '4' => VtKey::Digit4,
            '5' => VtKey::Digit5,
            '6' => VtKey::Digit6,
            '7' => VtKey::Digit7,
            '8' => VtKey::Digit8,
            '9' => VtKey::Digit9,
            '`' => VtKey::Backquote,
            '\\' => VtKey::Backslash,
            '[' => VtKey::BracketLeft,
            ']' => VtKey::BracketRight,
            ',' => VtKey::Comma,
            '=' => VtKey::Equal,
            '-' => VtKey::Minus,
            '.' => VtKey::Period,
            '\'' => VtKey::Quote,
            ';' => VtKey::Semicolon,
            '/' => VtKey::Slash,
            _ => VtKey::Unidentified,
        };
        return (vt, c as u32);
    }
    let vt = match key {
        "space" => VtKey::Space,
        "tab" => VtKey::Tab,
        "enter" => VtKey::Enter,
        "backspace" => VtKey::Backspace,
        "escape" => VtKey::Escape,
        "up" => VtKey::ArrowUp,
        "down" => VtKey::ArrowDown,
        "left" => VtKey::ArrowLeft,
        "right" => VtKey::ArrowRight,
        "pageup" => VtKey::PageUp,
        "pagedown" => VtKey::PageDown,
        "home" => VtKey::Home,
        "end" => VtKey::End,
        "delete" => VtKey::Delete,
        "insert" => VtKey::Insert,
        "f1" => VtKey::F1,
        "f2" => VtKey::F2,
        "f3" => VtKey::F3,
        "f4" => VtKey::F4,
        "f5" => VtKey::F5,
        "f6" => VtKey::F6,
        "f7" => VtKey::F7,
        "f8" => VtKey::F8,
        "f9" => VtKey::F9,
        "f10" => VtKey::F10,
        "f11" => VtKey::F11,
        "f12" => VtKey::F12,
        "f13" => VtKey::F13,
        "f14" => VtKey::F14,
        "f15" => VtKey::F15,
        "f16" => VtKey::F16,
        "f17" => VtKey::F17,
        "f18" => VtKey::F18,
        "f19" => VtKey::F19,
        "f20" => VtKey::F20,
        "f21" => VtKey::F21,
        "f22" => VtKey::F22,
        "f23" => VtKey::F23,
        "f24" => VtKey::F24,
        "f25" => VtKey::F25,
        _ => VtKey::Unidentified,
    };
    let codepoint = if vt == VtKey::Space {
        u32::from(' ')
    } else {
        0
    };
    (vt, codepoint)
}

/// Text the key would insert, filtered per the encoder contract: no empty
/// strings (dead keys mid-composition), no C0 controls (enter/tab arrive as
/// "\n"/"\t"), and no macOS PUA function-key codes.
fn keystroke_text(keystroke: &Keystroke) -> Option<&str> {
    terminal_key_text(keystroke.key_char.as_deref()?)
}

/// With macos-option-as-alt enabled, GPUI's `key_char` is the Option-produced
/// symbol (Option+, -> ≤), while `key` is explicitly the character printed on
/// the physical layout key (","). Ghostty needs the latter plus an unconsumed
/// Alt modifier so its own encoder can produce the terminal escape sequence.
fn keystroke_option_as_alt_text(keystroke: &Keystroke) -> Option<&str> {
    let key = keystroke.key.as_str();
    (key.chars().count() == 1)
        .then(|| terminal_key_text(key))
        .flatten()
}

fn terminal_key_text(text: &str) -> Option<&str> {
    if text.is_empty()
        || text
            .chars()
            .any(|c| c.is_control() || ('\u{F700}'..='\u{F8FF}').contains(&c))
    {
        return None;
    }
    Some(text)
}

fn single_scalar_codepoint(text: Option<&str>) -> Option<u32> {
    let mut scalars = text?.chars();
    let scalar = scalars.next()?;
    scalars.next().is_none().then_some(scalar as u32)
}

/// Cell-granularity drag: the anchor cell always stays selected; the end
/// column is exclusive. Rectangular (alt+drag) selections normalize to
/// top-left/bottom-right here because their per-row column range is the
/// same on every row, unlike the linear walk order.
fn cell_selection(anchor: (u64, u16), cell: (u64, u16), rectangular: bool) -> TerminalSelection {
    if rectangular {
        return TerminalSelection {
            start_row: anchor.0.min(cell.0),
            start_col: anchor.1.min(cell.1),
            end_row: anchor.0.max(cell.0),
            end_col: anchor.1.max(cell.1) + 1,
            rectangular: true,
        };
    }
    if cell >= anchor {
        TerminalSelection {
            start_row: anchor.0,
            start_col: anchor.1,
            end_row: cell.0,
            end_col: cell.1 + 1,
            rectangular,
        }
    } else {
        TerminalSelection {
            start_row: cell.0,
            start_col: cell.1,
            end_row: anchor.0,
            end_col: anchor.1 + 1,
            rectangular,
        }
    }
}

/// Word-granularity drag: the union of the words under the anchor and the
/// current cell.
fn word_selection(
    frame: &TerminalSnapshot,
    viewport_offset: u64,
    anchor: (u64, u16),
    cell: (u64, u16),
    word_chars: &str,
) -> TerminalSelection {
    let anchor_viewport_row = anchor.0.saturating_sub(viewport_offset) as u16;
    let cell_viewport_row = cell.0.saturating_sub(viewport_offset) as u16;
    let (anchor_start, anchor_end) = word_bounds(frame, anchor_viewport_row, anchor.1, word_chars);
    let (cell_start, cell_end) = word_bounds(frame, cell_viewport_row, cell.1, word_chars);
    if (cell.0, cell_start) >= (anchor.0, anchor_start) {
        TerminalSelection {
            start_row: anchor.0,
            start_col: anchor_start,
            end_row: cell.0,
            end_col: cell_end,
            rectangular: false,
        }
    } else {
        TerminalSelection {
            start_row: cell.0,
            start_col: cell_start,
            end_row: anchor.0,
            end_col: anchor_end,
            rectangular: false,
        }
    }
}

/// Line-granularity drag: whole rows between the anchor and current row.
fn line_selection(cols: u16, row_a: u64, row_b: u64) -> TerminalSelection {
    TerminalSelection {
        start_row: row_a.min(row_b),
        start_col: 0,
        end_row: row_a.max(row_b),
        end_col: cols,
        rectangular: false,
    }
}

/// A row's text plus a per-byte column map: `columns[i]` is the grid column
/// of the cell that produced byte `i`. Spacer cells contribute no bytes.
fn row_text_with_columns(row: &SnapshotRow) -> (String, Vec<u16>) {
    let mut text = String::new();
    let mut columns: Vec<u16> = Vec::new();
    for (col, cell) in row.cells.iter().enumerate() {
        match cell.width {
            VtCellWide::SpacerTail | VtCellWide::SpacerHead => continue,
            VtCellWide::Narrow | VtCellWide::Wide => {}
        }
        let before = text.len();
        text.push(cell.base);
        if let Some(combining) = &cell.combining {
            text.push_str(combining);
        }
        columns.resize(text.len(), 0);
        for slot in &mut columns[before..] {
            *slot = col as u16;
        }
    }
    (text, columns)
}

fn terminal_text_row_with_columns(row: &TerminalTextRow) -> (String, Vec<u16>) {
    let mut text = String::new();
    let mut columns = Vec::new();
    for cell in &row.cells {
        text.push_str(&cell.text);
        columns.resize(text.len(), cell.column);
    }
    (text, columns)
}

/// URL schemes recognized by the plain-text row scan. Deliberately narrow:
/// this approximates ghostty's regex link detection for the common cases
/// without inheriting its full pattern set.
const ROW_SCAN_URL_SCHEMES: &[&str] = &["https://", "http://", "file://"];

/// Trailing characters that read as prose punctuation, not URL content.
const ROW_SCAN_URL_TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', '"', '\'', '>', ')'];

/// Find a URL in the row text whose cells cover `hover_col`. Returns the
/// URL string and the grid column range it occupies.
fn scan_row_url(text: &str, columns: &[u16], hover_col: u16) -> Option<(String, Range<u16>)> {
    for scheme in ROW_SCAN_URL_SCHEMES {
        let mut from = 0;
        while let Some(found) = text[from..].find(scheme) {
            let start = from + found;
            let tail = &text[start..];
            let mut len = tail
                .find(|c: char| c.is_whitespace() || c == '<' || c == '"' || c == '\'')
                .unwrap_or(tail.len());
            // Trim prose punctuation so "see https://x.dev." opens x.dev.
            while len > 0 {
                let last = tail[..len].chars().next_back().unwrap();
                if ROW_SCAN_URL_TRAILING.contains(&last)
                    && !(last == ')'
                        && tail[..len].matches('(').count() >= tail[..len].matches(')').count())
                {
                    len -= last.len_utf8();
                } else {
                    break;
                }
            }
            if len <= scheme.len() {
                from = start + scheme.len();
                continue;
            }
            let end = start + len;
            let start_col = columns.get(start).copied()?;
            let end_col = columns.get(end - 1).copied()? + 1;
            if (start_col..end_col).contains(&hover_col) {
                return Some((text[start..end].to_string(), start_col..end_col));
            }
            from = end;
        }
    }
    None
}

/// Characters that may appear inside a scanned file path. Approximates
/// ghostty's path character class minus prose punctuation (commas,
/// quotes, brackets) that usually delimits paths in terminal output.
fn is_row_scan_path_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '_' | '-' | '.' | '/' | '~' | ':' | '@' | '#' | '$' | '%' | '+' | '='
        )
}

/// Find a file path covering `hover_col` in the row text, approximating
/// ghostty's path regex for the common cases: absolute (`/`), dot-relative
/// (`./`, `../`), home (`~/`), `$VAR/`, and bare relative paths containing
/// a dot (`src/config/url.zig`). Returns the path text and the grid column
/// range it occupies.
fn scan_row_file_path(text: &str, columns: &[u16], hover_col: u16) -> Option<(String, Range<u16>)> {
    let (hover_byte, hover_char) = text
        .char_indices()
        .find(|(index, _)| columns.get(*index).copied() == Some(hover_col))?;
    if !is_row_scan_path_char(hover_char) {
        return None;
    }
    let start = text[..hover_byte]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_row_scan_path_char(*c))
        .last()
        .map(|(index, _)| index)
        .unwrap_or(hover_byte);
    let mut end = text[hover_byte..]
        .char_indices()
        .take_while(|(_, c)| is_row_scan_path_char(*c))
        .last()
        .map(|(index, c)| hover_byte + index + c.len_utf8())
        .unwrap_or(hover_byte);
    // Trim trailing prose punctuation ("edit src/main.rs.") the way the
    // URL scan does; a trailing colon is a suffix separator, not path.
    while end > start {
        let last = text[start..end].chars().next_back().unwrap();
        if matches!(last, '.' | ',' | ':' | ';' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    if end <= start {
        return None;
    }
    let token = &text[start..end];
    let looks_like_path = if let Some(rest) = token.strip_prefix('/') {
        // Absolute path; a second leading slash reads as a comment.
        !rest.is_empty() && !rest.starts_with('/')
    } else if let Some(rest) = token
        .strip_prefix("./")
        .or_else(|| token.strip_prefix("../"))
        .or_else(|| token.strip_prefix("~/"))
    {
        !rest.is_empty()
    } else {
        // Bare relative paths need an interior slash and a dot so prose
        // like "and/or" stays plain text (ghostty's dotted-path rule).
        token.contains('/') && token.contains('.')
    };
    if !looks_like_path {
        return None;
    }
    let start_col = columns.get(start).copied()?;
    let end_col = columns.get(end - 1).copied()? + 1;
    if !(start_col..end_col).contains(&hover_col) {
        return None;
    }
    Some((token.to_string(), start_col..end_col))
}

/// Resolve a relative file-path link against the terminal pwd before
/// opening, mirroring ghostty's resolvePathForOpening: URLs, absolute,
/// and `~` paths pass through, and the raw text is kept when the
/// resolved file does not exist.
fn resolve_relative_link_target(url: &str, pwd: Option<&str>) -> String {
    if url.contains("://") || url.starts_with('/') || url.starts_with('~') {
        return url.to_string();
    }
    if let Some(pwd) = pwd {
        let resolved = std::path::Path::new(pwd).join(url);
        if std::fs::metadata(&resolved).is_ok() {
            return resolved.to_string_lossy().into_owned();
        }
    }
    url.to_string()
}

/// Character classes for double-click word selection: whitespace, word
/// characters, and separator symbols each form their own runs. Path-ish
/// punctuation (./-_~) stays inside words so file paths select whole.
fn word_char_class(c: char, separators: &str) -> u8 {
    if c.is_whitespace() {
        return 0;
    }
    if separators.contains(c) { 2 } else { 1 }
}

/// Half-open column range of the same-class run around `col`. Wide-cell
/// spacers inherit the class of their head cell.
fn word_bounds(frame: &TerminalSnapshot, row: u16, col: u16, separators: &str) -> (u16, u16) {
    let Some(cells) = frame.rows.get(usize::from(row)).map(|row| &row.cells) else {
        return (col, col.saturating_add(1));
    };
    if cells.is_empty() {
        return (col, col.saturating_add(1));
    }
    let col = usize::from(col).min(cells.len() - 1);
    let class_at = |index: usize| -> u8 {
        let mut index = index;
        while index > 0
            && matches!(
                cells[index].width,
                VtCellWide::SpacerTail | VtCellWide::SpacerHead
            )
        {
            index -= 1;
        }
        word_char_class(cells[index].base, separators)
    };
    let class = class_at(col);
    let mut start = col;
    while start > 0 && class_at(start - 1) == class {
        start -= 1;
    }
    let mut end = col + 1;
    while end < cells.len() && class_at(end) == class {
        end += 1;
    }
    (start as u16, end as u16)
}

fn layout_selection(
    selection: Option<TerminalSelection>,
    frame: &TerminalSnapshot,
    configured_background: Option<TerminalConfiguredColor>,
) -> Vec<(u16, CellSpan)> {
    let Some(selection) = selection else {
        return Vec::new();
    };
    let viewport_rows = frame.rows.len() as u16;
    if viewport_rows == 0 || frame.cols == 0 {
        return Vec::new();
    }
    let (start, end) = {
        let start = (selection.start_row, selection.start_col);
        let end = (selection.end_row, selection.end_col);
        if start <= end {
            (start, end)
        } else {
            (end, start)
        }
    };
    // Selection tint: default foreground at low alpha adapts to any theme.
    let mut color = rgb_to_hsla(
        configured_background
            .map(|color| color.resolve(frame))
            .unwrap_or(frame.foreground),
    );
    if configured_background.is_none() {
        color.a = 0.25;
    }

    let mut spans = Vec::new();
    for viewport_row in 0..viewport_rows {
        let row = frame
            .scrollbar
            .offset
            .saturating_add(u64::from(viewport_row));
        if row < start.0 || row > end.0 {
            continue;
        }
        let cols = selection_row_cols(selection, start, end, row, frame.cols);
        let col_start = cols.start;
        let col_end = cols.end.min(frame.cols);
        if col_start >= col_end {
            continue;
        }
        spans.push((
            viewport_row,
            CellSpan {
                col: col_start,
                len: col_end - col_start,
                color,
            },
        ));
    }
    spans
}

/// Search highlight spans for the open find, if any: every viewport match
/// gets a translucent highlight, with the selected match emphasized.
fn layout_search(
    search: Option<&TerminalSearch>,
    frame: &TerminalSnapshot,
) -> Vec<(u16, CellSpan)> {
    let Some(search) = search else {
        return Vec::new();
    };
    search
        .matches
        .iter()
        .enumerate()
        .filter_map(|(index, (row, cols))| {
            let viewport_row = row.checked_sub(frame.scrollbar.offset)?;
            if viewport_row >= frame.scrollbar.len {
                return None;
            }
            let color = if index == search.selected {
                Rgba {
                    r: 1.,
                    g: 0.62,
                    b: 0.1,
                    a: 0.55,
                }
            } else {
                Rgba {
                    r: 1.,
                    g: 0.85,
                    b: 0.2,
                    a: 0.3,
                }
            };
            Some((
                viewport_row as u16,
                CellSpan {
                    col: cols.start,
                    len: cols.end.saturating_sub(cols.start),
                    color: color.into(),
                },
            ))
        })
        .collect()
}

fn layout_scrollbar(
    visible: bool,
    scrollbar: VtScrollbar,
    bounds: Bounds<Pixels>,
) -> Option<ScrollbarLayout> {
    if !visible || scrollbar.total <= scrollbar.len || bounds.size.height <= px(0.) {
        return None;
    }

    let thickness = px(TERMINAL_SCROLLBAR_THICKNESS);
    let slot_height = bounds.size.height;
    let slot = Bounds::new(
        point(bounds.right() - thickness, bounds.top()),
        size(thickness, slot_height),
    );
    let knob_height = (slot_height * (scrollbar.len as f32 / scrollbar.total as f32))
        .max(px(TERMINAL_SCROLLBAR_MIN_KNOB_HEIGHT))
        .min(slot_height);
    let max_offset = scrollbar.total.saturating_sub(scrollbar.len);
    let offset_fraction = if max_offset == 0 {
        0.
    } else {
        (scrollbar.offset.min(max_offset) as f32 / max_offset as f32).clamp(0., 1.)
    };
    let knob_y = slot.origin.y + (slot_height - knob_height) * offset_fraction;
    let knob = Bounds::new(point(slot.origin.x, knob_y), size(thickness, knob_height));

    Some(ScrollbarLayout {
        slot,
        knob,
        slot_color: Rgba {
            r: 0.08,
            g: 0.08,
            b: 0.08,
            a: 0.18,
        }
        .into(),
        knob_color: Rgba {
            r: 0.92,
            g: 0.92,
            b: 0.92,
            a: 0.48,
        }
        .into(),
    })
}

/// Column range a selection covers on one row: rectangular selections use
/// the same normalized column range on every row; linear selections walk
/// from the start position to the end position.
fn selection_row_cols(
    selection: TerminalSelection,
    start: (u64, u16),
    end: (u64, u16),
    row: u64,
    cols: u16,
) -> Range<u16> {
    if selection.rectangular {
        return selection.start_col.min(selection.end_col)
            ..selection.start_col.max(selection.end_col);
    }
    let col_start = if row == start.0 { start.1 } else { 0 };
    let col_end = if row == end.0 { end.1 } else { cols };
    col_start..col_end
}

fn layout_cursor(
    shape: TerminalCursorShape,
    opacity: f32,
    configured_text: Option<TerminalConfiguredColor>,
    focused: bool,
    frame: &TerminalSnapshot,
    config: &TerminalFontConfig,
    metrics: CellMetrics,
    window: &mut Window,
) -> Option<CursorLayout> {
    if !frame.cursor_visible {
        return None;
    }
    let (col, row) = frame.cursor?;
    let cell = frame
        .rows
        .get(usize::from(row))
        .and_then(|r| r.cells.get(usize::from(col)));
    let width_cells = match cell.map(|cell| cell.width) {
        Some(VtCellWide::Wide) => 2,
        _ => 1,
    };
    let mut color = rgb_to_hsla(frame.cursor_color.unwrap_or(frame.foreground));
    color.a = opacity.clamp(0.0, 1.0);

    // Unfocused terminals draw a hollow block regardless of the configured
    // shape (ghostty/Zed convention) and keep the covered glyph as-is.
    if !focused {
        return Some(CursorLayout {
            col,
            row,
            width_cells,
            shape: TerminalCursorShape::Block,
            color,
            hollow: true,
            overlay: None,
        });
    }

    // Block cursors invert the glyph they cover so it stays readable.
    let overlay = match (shape, cell) {
        (TerminalCursorShape::Block, Some(cell)) if !cell_is_blank(cell) => {
            let mut text = String::new();
            text.push(cell.base);
            if let Some(combining) = &cell.combining {
                text.push_str(combining);
            }
            let run = TextRun {
                len: text.len(),
                font: base_font(config, cell.bold, cell.italic),
                color: rgb_to_hsla(
                    configured_text
                        .map(|color| color.resolve(frame))
                        .unwrap_or(frame.background),
                ),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            Some(window.text_system().shape_line(
                SharedString::from(text),
                config.size,
                &[run],
                Some(metrics.cell_width),
            ))
        }
        _ => None,
    };

    Some(CursorLayout {
        col,
        row,
        width_cells,
        shape,
        color,
        hollow: false,
        overlay,
    })
}

fn layout_marked_text(
    marked_text: Option<&str>,
    frame: &TerminalSnapshot,
    config: &TerminalFontConfig,
    window: &mut Window,
) -> Option<MarkedTextLayout> {
    let text = marked_text?.trim_end_matches('\n');
    if text.is_empty() {
        return None;
    }
    let (col, row) = frame.cursor?;
    let fg = rgb_to_hsla(frame.foreground);
    let run = TextRun {
        len: text.len(),
        font: base_font(config, false, false),
        color: fg,
        background_color: None,
        underline: Some(GpuiUnderlineStyle {
            thickness: px(1.),
            color: Some(fg),
            wavy: false,
        }),
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        config.size,
        &[run],
        None,
    );
    Some(MarkedTextLayout {
        col,
        row,
        shaped,
        background: rgb_to_hsla(frame.background),
    })
}
