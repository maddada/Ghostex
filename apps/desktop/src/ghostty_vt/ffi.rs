#![allow(non_camel_case_types)]

use std::ffi::{c_int, c_void};

pub type GhosttyResult = c_int;
pub const GHOSTTY_SUCCESS: GhosttyResult = 0;
pub const GHOSTTY_OUT_OF_MEMORY: GhosttyResult = -1;
pub const GHOSTTY_INVALID_VALUE: GhosttyResult = -2;
pub const GHOSTTY_OUT_OF_SPACE: GhosttyResult = -3;
pub const GHOSTTY_NO_VALUE: GhosttyResult = -4;
pub const GHOSTTY_IO_ERROR: GhosttyResult = -5;
pub const GHOSTTY_LIMIT_EXCEEDED: GhosttyResult = -6;
pub const GHOSTTY_REJECTED: GhosttyResult = -7;

pub type GhosttyTerminal = *mut c_void;
pub type GhosttyRenderState = *mut c_void;
pub type GhosttyRenderStateRowIterator = *mut c_void;
pub type GhosttyRenderStateRowCells = *mut c_void;
pub type GhosttyKittyGraphics = *mut c_void;
pub type GhosttyKittyGraphicsImage = *const c_void;
pub type GhosttyKittyGraphicsPlacementIterator = *mut c_void;
pub const GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS: GhosttyTerminalData = 30;
pub const GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MAX_DIMENSION: GhosttyTerminalOption = 42;
pub const GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MAX_PIXELS: GhosttyTerminalOption = 43;
pub const GHOSTTY_SYS_OPT_DECODE_PNG: c_int = 1;
pub const GHOSTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR: c_int = 1;
pub const GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID: c_int = 1;
pub const GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL: c_int = 3;
pub const GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET: c_int = 4;
pub const GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET: c_int = 5;
pub const GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z: c_int = 12;
pub const GHOSTTY_KITTY_IMAGE_DATA_WIDTH: c_int = 3;
pub const GHOSTTY_KITTY_IMAGE_DATA_HEIGHT: c_int = 4;
pub const GHOSTTY_KITTY_IMAGE_DATA_FORMAT: c_int = 5;
pub const GHOSTTY_KITTY_IMAGE_DATA_DATA_PTR: c_int = 7;
pub const GHOSTTY_KITTY_IMAGE_DATA_DATA_LEN: c_int = 8;
pub const GHOSTTY_KITTY_IMAGE_DATA_GENERATION: c_int = 9;
pub const GHOSTTY_KITTY_IMAGE_FORMAT_RGB: c_int = 0;
pub const GHOSTTY_KITTY_IMAGE_FORMAT_RGBA: c_int = 1;
pub const GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA: c_int = 3;
pub const GHOSTTY_KITTY_IMAGE_FORMAT_GRAY: c_int = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyKittyGraphicsPlacementRenderInfo {
    pub size: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub grid_cols: u32,
    pub grid_rows: u32,
    pub viewport_col: i32,
    pub viewport_row: i32,
    pub viewport_visible: bool,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
}

impl Default for GhosttyKittyGraphicsPlacementRenderInfo {
    fn default() -> Self {
        // All-zero scalar fields are valid; size initializes the sized ABI.
        let mut value: Self = unsafe { std::mem::zeroed() };
        value.size = std::mem::size_of::<Self>();
        value
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyKittyGraphicsVirtualPlacement {
    pub size: usize,
    pub image_id: u32,
    pub viewport_col: u16,
    pub viewport_row: u16,
    pub offset_x: u32,
    pub offset_y: u32,
    pub dest_width: u32,
    pub dest_height: u32,
    pub source_x: f64,
    pub source_y: f64,
    pub source_width: f64,
    pub source_height: f64,
}

impl Default for GhosttyKittyGraphicsVirtualPlacement {
    fn default() -> Self {
        let mut value: Self = unsafe { std::mem::zeroed() };
        value.size = std::mem::size_of::<Self>();
        value
    }
}

#[repr(C)]
pub struct GhosttySysImage {
    pub width: u32,
    pub height: u32,
    pub data: *mut u8,
    pub data_len: usize,
}

/// Opaque cell value (`GhosttyCell` in screen.h).
pub type GhosttyCell = u64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GhosttyColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub type GhosttyRenderStateDirty = c_int;
pub const GHOSTTY_RENDER_STATE_DIRTY_FALSE: GhosttyRenderStateDirty = 0;
pub const GHOSTTY_RENDER_STATE_DIRTY_PARTIAL: GhosttyRenderStateDirty = 1;
pub const GHOSTTY_RENDER_STATE_DIRTY_FULL: GhosttyRenderStateDirty = 2;

pub type GhosttyRenderStateData = c_int;
pub const GHOSTTY_RENDER_STATE_DATA_COLS: GhosttyRenderStateData = 1;
pub const GHOSTTY_RENDER_STATE_DATA_ROWS: GhosttyRenderStateData = 2;
pub const GHOSTTY_RENDER_STATE_DATA_DIRTY: GhosttyRenderStateData = 3;
pub const GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR: GhosttyRenderStateData = 4;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE: GhosttyRenderStateData = 11;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE: GhosttyRenderStateData = 14;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X: GhosttyRenderStateData = 15;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y: GhosttyRenderStateData = 16;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR: GhosttyRenderStateData = 18;
pub const GHOSTTY_RENDER_STATE_DATA_COLORS: GhosttyRenderStateData = 19;

pub type GhosttyRenderStateOption = c_int;
pub const GHOSTTY_RENDER_STATE_OPTION_DIRTY: GhosttyRenderStateOption = 0;

pub type GhosttyRenderStateRowData = c_int;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY: GhosttyRenderStateRowData = 1;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_RAW: GhosttyRenderStateRowData = 2;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_CELLS: GhosttyRenderStateRowData = 3;
pub const GHOSTTY_RENDER_STATE_ROW_DATA_CELLS_RAW: GhosttyRenderStateRowData = 5;

pub type GhosttyRenderStateRowOption = c_int;
pub const GHOSTTY_RENDER_STATE_ROW_OPTION_DIRTY: GhosttyRenderStateRowOption = 0;

pub type GhosttyRenderStateRowCellsData = c_int;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW: GhosttyRenderStateRowCellsData = 1;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE: GhosttyRenderStateRowCellsData = 2;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN: GhosttyRenderStateRowCellsData = 3;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF: GhosttyRenderStateRowCellsData = 4;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR: GhosttyRenderStateRowCellsData = 5;
pub const GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR: GhosttyRenderStateRowCellsData = 6;

pub type GhosttyCellData = c_int;
pub const GHOSTTY_CELL_DATA_WIDE: GhosttyCellData = 3;
pub const GHOSTTY_CELL_DATA_HAS_HYPERLINK: GhosttyCellData = 7;
pub const GHOSTTY_CELL_DATA_SEMANTIC_CONTENT: GhosttyCellData = 9;

/// screen.h `GhosttyCellSemanticContent` (OSC 133 cell classification).
pub type GhosttyCellSemanticContent = c_int;
pub const GHOSTTY_CELL_SEMANTIC_OUTPUT: GhosttyCellSemanticContent = 0;
pub const GHOSTTY_CELL_SEMANTIC_INPUT: GhosttyCellSemanticContent = 1;
pub const GHOSTTY_CELL_SEMANTIC_PROMPT: GhosttyCellSemanticContent = 2;

/// Opaque row value (`GhosttyRow` in screen.h).
pub type GhosttyRow = u64;

pub type GhosttyRowData = c_int;
pub const GHOSTTY_ROW_DATA_WRAP: GhosttyRowData = 1;
pub const GHOSTTY_ROW_DATA_WRAP_CONTINUATION: GhosttyRowData = 2;
pub const GHOSTTY_ROW_DATA_SEMANTIC_PROMPT: GhosttyRowData = 6;

/// screen.h `GhosttyRowSemanticPrompt` (OSC 133 row classification).
pub type GhosttyRowSemanticPrompt = c_int;
pub const GHOSTTY_ROW_SEMANTIC_NONE: GhosttyRowSemanticPrompt = 0;

/// types.h `GhosttyString`: a borrowed byte string. Lifetime is bounded
/// by the producing API (terminal title/pwd stay valid until the next
/// `ghostty_terminal_vt_write`/`ghostty_terminal_reset`), so wrappers
/// must copy the bytes out before releasing terminal access.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyString {
    pub ptr: *const u8,
    pub len: usize,
}

/// terminal.h `GhosttyTerminalScrollbar`: viewport position within the
/// scrollable area, in rows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GhosttyTerminalScrollbar {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

/// point.h coordinate/tagged-point types for grid references.
pub type GhosttyPointTag = c_int;
pub const GHOSTTY_POINT_TAG_ACTIVE: GhosttyPointTag = 0;
pub const GHOSTTY_POINT_TAG_VIEWPORT: GhosttyPointTag = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyPointCoordinate {
    pub x: u16,
    pub y: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyPointValue {
    pub coordinate: GhosttyPointCoordinate,
    pub _padding: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyPoint {
    pub tag: GhosttyPointTag,
    pub value: GhosttyPointValue,
}

/// Sized struct (grid_ref.h). A resolved cell reference, valid only
/// until the next mutating terminal call.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyGridRef {
    pub size: usize,
    pub node: *mut c_void,
    pub x: u16,
    pub y: u16,
}

impl GhosttyGridRef {
    pub fn init_sized() -> Self {
        let mut grid_ref: Self = unsafe { std::mem::zeroed() };
        grid_ref.size = std::mem::size_of::<Self>();
        grid_ref
    }
}

pub type GhosttyCellWide = c_int;
pub const GHOSTTY_CELL_WIDE_NARROW: GhosttyCellWide = 0;
pub const GHOSTTY_CELL_WIDE_WIDE: GhosttyCellWide = 1;
pub const GHOSTTY_CELL_WIDE_SPACER_TAIL: GhosttyCellWide = 2;
pub const GHOSTTY_CELL_WIDE_SPACER_HEAD: GhosttyCellWide = 3;

/// sgr.h `GhosttySgrUnderline`: value of [`GhosttyStyle::underline`].
pub type GhosttySgrUnderline = c_int;
pub const GHOSTTY_SGR_UNDERLINE_NONE: GhosttySgrUnderline = 0;
pub const GHOSTTY_SGR_UNDERLINE_SINGLE: GhosttySgrUnderline = 1;
pub const GHOSTTY_SGR_UNDERLINE_DOUBLE: GhosttySgrUnderline = 2;
pub const GHOSTTY_SGR_UNDERLINE_CURLY: GhosttySgrUnderline = 3;
pub const GHOSTTY_SGR_UNDERLINE_DOTTED: GhosttySgrUnderline = 4;
pub const GHOSTTY_SGR_UNDERLINE_DASHED: GhosttySgrUnderline = 5;

pub type GhosttyTerminalOption = c_int;
pub const GHOSTTY_TERMINAL_OPT_USERDATA: GhosttyTerminalOption = 0;
pub const GHOSTTY_TERMINAL_OPT_WRITE_PTY: GhosttyTerminalOption = 1;
pub const GHOSTTY_TERMINAL_OPT_BELL: GhosttyTerminalOption = 2;
pub const GHOSTTY_TERMINAL_OPT_TITLE_CHANGED: GhosttyTerminalOption = 5;
pub const GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND: GhosttyTerminalOption = 11;
pub const GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND: GhosttyTerminalOption = 12;
pub const GHOSTTY_TERMINAL_OPT_COLOR_CURSOR: GhosttyTerminalOption = 13;
pub const GHOSTTY_TERMINAL_OPT_COLOR_PALETTE: GhosttyTerminalOption = 14;
pub const GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE: GhosttyTerminalOption = 26;
pub const GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES: GhosttyTerminalOption = 27;
pub const GHOSTTY_TERMINAL_OPT_CLIPBOARD_READ: GhosttyTerminalOption = 38;
pub const GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE_MAX_BYTES: GhosttyTerminalOption = 39;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyTerminalModeConfig {
    pub mode: GhosttyMode,
    pub value: bool,
}

pub type GhosttyTerminalData = c_int;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_X: GhosttyTerminalData = 3;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_Y: GhosttyTerminalData = 4;
pub const GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN: GhosttyTerminalData = 6;
pub const GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS: GhosttyTerminalData = 8;
pub const GHOSTTY_TERMINAL_DATA_SCROLLBAR: GhosttyTerminalData = 9;
pub const GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING: GhosttyTerminalData = 11;
pub const GHOSTTY_TERMINAL_DATA_TITLE: GhosttyTerminalData = 12;
pub const GHOSTTY_TERMINAL_DATA_PWD: GhosttyTerminalData = 13;
pub const GHOSTTY_TERMINAL_DATA_MODE: GhosttyTerminalData = 37;
pub const GHOSTTY_TERMINAL_DATA_CLIPBOARD_WRITE_MAX_BYTES: GhosttyTerminalData = 40;

pub type GhosttyTerminalScreen = c_int;
pub const GHOSTTY_TERMINAL_SCREEN_PRIMARY: GhosttyTerminalScreen = 0;
pub const GHOSTTY_TERMINAL_SCREEN_ALTERNATE: GhosttyTerminalScreen = 1;

/// modes.h `GhosttyMode`: packed 16-bit mode id, bits 0-14 the mode
/// value, bit 15 set for ANSI modes (clear for DEC private modes).
pub type GhosttyMode = u16;
pub const GHOSTTY_MODE_ALT_SCROLL: GhosttyMode = 1007;
pub const GHOSTTY_MODE_FOCUS_EVENT: GhosttyMode = 1004;
pub const GHOSTTY_MODE_BRACKETED_PASTE: GhosttyMode = 2004;
/// DECSET 2026, synchronized output: the producer asks the terminal to
/// defer painting until the matching reset, so a redraw that spans
/// several reads never shows its intermediate states.
pub const GHOSTTY_MODE_SYNCHRONIZED_OUTPUT: GhosttyMode = 2026;

pub type GhosttyTerminalScrollViewportTag = c_int;
pub const GHOSTTY_SCROLL_VIEWPORT_TOP: GhosttyTerminalScrollViewportTag = 0;
pub const GHOSTTY_SCROLL_VIEWPORT_BOTTOM: GhosttyTerminalScrollViewportTag = 1;
pub const GHOSTTY_SCROLL_VIEWPORT_DELTA: GhosttyTerminalScrollViewportTag = 2;
pub const GHOSTTY_SCROLL_VIEWPORT_ROW: GhosttyTerminalScrollViewportTag = 3;

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyTerminalScrollViewportValue {
    pub delta: isize,
    pub row: usize,
    pub _padding: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyTerminalScrollViewport {
    pub tag: GhosttyTerminalScrollViewportTag,
    pub value: GhosttyTerminalScrollViewportValue,
}

pub type GhosttyKeyEvent = *mut c_void;
pub type GhosttyKeyEncoder = *mut c_void;
pub type GhosttyMouseEvent = *mut c_void;
pub type GhosttyMouseEncoder = *mut c_void;

/// key/event.h `GhosttyMods` bitmask.
pub type GhosttyMods = u16;
pub const GHOSTTY_MODS_SHIFT: GhosttyMods = 1 << 0;
pub const GHOSTTY_MODS_CTRL: GhosttyMods = 1 << 1;
pub const GHOSTTY_MODS_ALT: GhosttyMods = 1 << 2;
pub const GHOSTTY_MODS_SUPER: GhosttyMods = 1 << 3;
pub const GHOSTTY_MODS_CAPS_LOCK: GhosttyMods = 1 << 4;
pub const GHOSTTY_MODS_NUM_LOCK: GhosttyMods = 1 << 5;

pub type GhosttyKeyAction = c_int;
pub const GHOSTTY_KEY_ACTION_RELEASE: GhosttyKeyAction = 0;
pub const GHOSTTY_KEY_ACTION_PRESS: GhosttyKeyAction = 1;
pub const GHOSTTY_KEY_ACTION_REPEAT: GhosttyKeyAction = 2;

/// key/event.h `GhosttyKey`: W3C physical key codes. Declared as a
/// repr(C-int) enum so the discriminants track the C enum ORDER exactly
/// (both sides assign sequentially from 0); do not reorder or skip
/// entries when syncing with a vendored header bump.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GhosttyKey {
    Unidentified = 0,
    // Writing System Keys (W3C § 3.1.1)
    Backquote,
    Backslash,
    BracketLeft,
    BracketRight,
    Comma,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Equal,
    IntlBackslash,
    IntlRo,
    IntlYen,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Minus,
    Period,
    Quote,
    Semicolon,
    Slash,
    // Functional Keys (W3C § 3.1.2)
    AltLeft,
    AltRight,
    Backspace,
    CapsLock,
    ContextMenu,
    ControlLeft,
    ControlRight,
    Enter,
    MetaLeft,
    MetaRight,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
    Convert,
    KanaMode,
    NonConvert,
    // Control Pad Section (W3C § 3.2)
    Delete,
    End,
    Help,
    Home,
    Insert,
    PageDown,
    PageUp,
    // Arrow Pad Section (W3C § 3.3)
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    // Numpad Section (W3C § 3.4)
    NumLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadBackspace,
    NumpadClear,
    NumpadClearEntry,
    NumpadComma,
    NumpadDecimal,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    NumpadMemoryAdd,
    NumpadMemoryClear,
    NumpadMemoryRecall,
    NumpadMemoryStore,
    NumpadMemorySubtract,
    NumpadMultiply,
    NumpadParenLeft,
    NumpadParenRight,
    NumpadSubtract,
    NumpadSeparator,
    NumpadUp,
    NumpadDown,
    NumpadRight,
    NumpadLeft,
    NumpadBegin,
    NumpadHome,
    NumpadEnd,
    NumpadInsert,
    NumpadDelete,
    NumpadPageUp,
    NumpadPageDown,
    // Function Section (W3C § 3.5)
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    Fn,
    FnLock,
    PrintScreen,
    ScrollLock,
    Pause,
    // Media Keys (W3C § 3.6)
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    Eject,
    LaunchApp1,
    LaunchApp2,
    LaunchMail,
    MediaPlayPause,
    MediaSelect,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrevious,
    Power,
    Sleep,
    AudioVolumeDown,
    AudioVolumeMute,
    AudioVolumeUp,
    WakeUp,
    // Legacy, Non-standard, and Special Keys (W3C § 3.7)
    Copy,
    Cut,
    Paste,
}

pub type GhosttyOptionAsAlt = c_int;
pub const GHOSTTY_OPTION_AS_ALT_FALSE: GhosttyOptionAsAlt = 0;
pub const GHOSTTY_OPTION_AS_ALT_TRUE: GhosttyOptionAsAlt = 1;
pub const GHOSTTY_OPTION_AS_ALT_LEFT: GhosttyOptionAsAlt = 2;
pub const GHOSTTY_OPTION_AS_ALT_RIGHT: GhosttyOptionAsAlt = 3;

pub type GhosttyKeyEncoderOption = c_int;
pub const GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT: GhosttyKeyEncoderOption = 6;

pub type GhosttyMouseAction = c_int;
pub const GHOSTTY_MOUSE_ACTION_PRESS: GhosttyMouseAction = 0;
pub const GHOSTTY_MOUSE_ACTION_RELEASE: GhosttyMouseAction = 1;
pub const GHOSTTY_MOUSE_ACTION_MOTION: GhosttyMouseAction = 2;

pub type GhosttyMouseButton = c_int;
pub const GHOSTTY_MOUSE_BUTTON_UNKNOWN: GhosttyMouseButton = 0;
pub const GHOSTTY_MOUSE_BUTTON_LEFT: GhosttyMouseButton = 1;
pub const GHOSTTY_MOUSE_BUTTON_RIGHT: GhosttyMouseButton = 2;
pub const GHOSTTY_MOUSE_BUTTON_MIDDLE: GhosttyMouseButton = 3;
/// Wheel up in xterm-style encodings (button 64).
pub const GHOSTTY_MOUSE_BUTTON_FOUR: GhosttyMouseButton = 4;
/// Wheel down in xterm-style encodings (button 65).
pub const GHOSTTY_MOUSE_BUTTON_FIVE: GhosttyMouseButton = 5;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyMousePosition {
    pub x: f32,
    pub y: f32,
}

/// Sized struct (mouse/encoder.h). Construct via
/// [`GhosttyMouseEncoderSize::init_sized`].
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GhosttyMouseEncoderSize {
    pub size: usize,
    pub screen_width: u32,
    pub screen_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub padding_top: u32,
    pub padding_bottom: u32,
    pub padding_right: u32,
    pub padding_left: u32,
}

impl GhosttyMouseEncoderSize {
    pub fn init_sized() -> Self {
        let mut size: Self = unsafe { std::mem::zeroed() };
        size.size = std::mem::size_of::<Self>();
        size
    }
}

pub type GhosttyMouseEncoderOption = c_int;
pub const GHOSTTY_MOUSE_ENCODER_OPT_SIZE: GhosttyMouseEncoderOption = 2;
pub const GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED: GhosttyMouseEncoderOption = 3;
pub const GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL: GhosttyMouseEncoderOption = 4;

pub type GhosttyFocusEvent = c_int;
pub const GHOSTTY_FOCUS_GAINED: GhosttyFocusEvent = 0;
pub const GHOSTTY_FOCUS_LOST: GhosttyFocusEvent = 1;

/// terminal.h `GhosttyTerminalWritePtyFn`: query auto-replies (DA1, DSR,
/// DECRQM, ...) that must be written back to the PTY. `data` is only
/// valid for the duration of the call.
pub type GhosttyTerminalWritePtyFn = unsafe extern "C" fn(
    terminal: GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
);
/// terminal.h `GhosttyTerminalBellFn`.
pub type GhosttyTerminalBellFn =
    unsafe extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void);
/// terminal.h `GhosttyTerminalTitleChangedFn`. The new title is queried
/// from the terminal after the callback returns.
pub type GhosttyTerminalTitleChangedFn =
    unsafe extern "C" fn(terminal: GhosttyTerminal, userdata: *mut c_void);

/// terminal.h `GhosttyClipboardLocation`: normalized clipboard
/// destination of a program-initiated clipboard write.
pub type GhosttyClipboardLocation = c_int;
pub const GHOSTTY_CLIPBOARD_LOCATION_STANDARD: GhosttyClipboardLocation = 0;
pub const GHOSTTY_CLIPBOARD_LOCATION_SELECTION: GhosttyClipboardLocation = 1;
pub const GHOSTTY_CLIPBOARD_LOCATION_PRIMARY: GhosttyClipboardLocation = 2;

/// terminal.h `GhosttyClipboardWriteResult`.
pub type GhosttyClipboardWriteResult = c_int;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS: GhosttyClipboardWriteResult = 0;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_DENIED: GhosttyClipboardWriteResult = 1;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED: GhosttyClipboardWriteResult = 2;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_BUSY: GhosttyClipboardWriteResult = 3;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_INVALID_DATA: GhosttyClipboardWriteResult = 4;
pub const GHOSTTY_CLIPBOARD_WRITE_RESULT_IO_ERROR: GhosttyClipboardWriteResult = 5;

/// terminal.h `GhosttyClipboardContent`: one MIME representation in a
/// clipboard write. Both strings are borrowed and only valid for the
/// duration of the callback; `data` is already protocol-decoded.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyClipboardContent {
    pub mime: GhosttyString,
    pub data: GhosttyString,
}

/// Sized reply for terminal.h `GhosttyClipboardWrite`.
#[repr(C)]
pub struct GhosttyClipboardWriteReply {
    pub size: usize,
    pub result: GhosttyClipboardWriteResult,
    pub remember: bool,
}

pub type GhosttyClipboardWriteReplyFn = Option<
    unsafe extern "C" fn(
        write: *const GhosttyClipboardWrite,
        reply: *const GhosttyClipboardWriteReply,
    ),
>;

/// Sized terminal.h `GhosttyClipboardWrite`: a semantic, atomic clipboard
/// write which must be answered through `reply` before the callback returns.
#[repr(C)]
pub struct GhosttyClipboardWrite {
    pub size: usize,
    pub location: GhosttyClipboardLocation,
    pub contents: *const GhosttyClipboardContent,
    pub contents_len: usize,
    pub name: GhosttyString,
    pub granted: bool,
    pub can_remember: bool,
    pub ctx: *const c_void,
    pub reply: GhosttyClipboardWriteReplyFn,
}

/// terminal.h `GhosttyTerminalClipboardWriteFn`: invoked synchronously and
/// answered through `GhosttyClipboardWrite::reply` before returning.
pub type GhosttyTerminalClipboardWriteFn = unsafe extern "C" fn(
    terminal: GhosttyTerminal,
    userdata: *mut c_void,
    write: *const GhosttyClipboardWrite,
);

pub type GhosttyStyleColorTag = c_int;
pub const GHOSTTY_STYLE_COLOR_NONE: GhosttyStyleColorTag = 0;
pub const GHOSTTY_STYLE_COLOR_PALETTE: GhosttyStyleColorTag = 1;
pub const GHOSTTY_STYLE_COLOR_RGB: GhosttyStyleColorTag = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyStyleColorValue {
    pub palette: u8,
    pub rgb: GhosttyColorRgb,
    pub _padding: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyStyleColor {
    pub tag: GhosttyStyleColorTag,
    pub value: GhosttyStyleColorValue,
}

/// Sized struct (style.h). Construct via [`GhosttyStyle::init_sized`] so
/// the library can detect which struct version the caller compiled with.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyStyle {
    pub size: usize,
    pub fg_color: GhosttyStyleColor,
    pub bg_color: GhosttyStyleColor,
    pub underline_color: GhosttyStyleColor,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: c_int,
}

impl GhosttyStyle {
    pub fn init_sized() -> Self {
        // GHOSTTY_INIT_SIZED equivalent: zeroed with the size field set.
        let mut style: Self = unsafe { std::mem::zeroed() };
        style.size = std::mem::size_of::<Self>();
        style
    }
}

/// Sized struct (render.h). Construct via
/// [`GhosttyRenderStateColors::init_sized`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyRenderStateColors {
    pub size: usize,
    pub background: GhosttyColorRgb,
    pub foreground: GhosttyColorRgb,
    pub cursor: GhosttyColorRgb,
    pub cursor_has_value: bool,
    pub palette: [GhosttyColorRgb; 256],
}

impl GhosttyRenderStateColors {
    pub fn init_sized() -> Self {
        let mut colors: Self = unsafe { std::mem::zeroed() };
        colors.size = std::mem::size_of::<Self>();
        colors
    }
}

unsafe extern "C" {
    pub fn ghostty_sys_set(option: c_int, value: *const c_void) -> GhosttyResult;
    pub fn ghostty_alloc(allocator: *const c_void, len: usize) -> *mut u8;
    pub fn ghostty_kitty_graphics_get(
        graphics: GhosttyKittyGraphics,
        data: c_int,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_kitty_graphics_image(
        graphics: GhosttyKittyGraphics,
        image_id: u32,
    ) -> GhosttyKittyGraphicsImage;
    pub fn ghostty_kitty_graphics_image_get(
        image: GhosttyKittyGraphicsImage,
        data: c_int,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_kitty_graphics_placement_iterator_new(
        allocator: *const c_void,
        out: *mut GhosttyKittyGraphicsPlacementIterator,
    ) -> GhosttyResult;
    pub fn ghostty_kitty_graphics_placement_iterator_free(
        iterator: GhosttyKittyGraphicsPlacementIterator,
    );
    pub fn ghostty_kitty_graphics_placement_next(
        iterator: GhosttyKittyGraphicsPlacementIterator,
    ) -> bool;
    pub fn ghostty_kitty_graphics_placement_get(
        iterator: GhosttyKittyGraphicsPlacementIterator,
        data: c_int,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_kitty_graphics_placement_render_info(
        iterator: GhosttyKittyGraphicsPlacementIterator,
        image: GhosttyKittyGraphicsImage,
        terminal: GhosttyTerminal,
        out: *mut GhosttyKittyGraphicsPlacementRenderInfo,
    ) -> GhosttyResult;
    pub fn ghostty_kitty_graphics_virtual_placements(
        terminal: GhosttyTerminal,
        out: *mut GhosttyKittyGraphicsVirtualPlacement,
        capacity: usize,
        out_len: *mut usize,
    ) -> GhosttyResult;
    pub fn ghostty_color_palette_default(out: *mut GhosttyColorRgb);
    pub fn ghostty_terminal_new(
        allocator: *const c_void,
        terminal: *mut GhosttyTerminal,
        cols: u16,
        rows: u16,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);
    pub fn ghostty_terminal_reset(terminal: GhosttyTerminal);
    pub fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);
    /// For pointer-typed options (userdata, callbacks) `value` IS the
    /// pointer/function pointer itself, not a pointer to it. NULL clears.
    pub fn ghostty_terminal_set(
        terminal: GhosttyTerminal,
        option: GhosttyTerminalOption,
        value: *const c_void,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_new(
        allocator: *const c_void,
        state: *mut GhosttyRenderState,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_free(state: GhosttyRenderState);
    pub fn ghostty_render_state_update(
        state: GhosttyRenderState,
        terminal: GhosttyTerminal,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_get(
        state: GhosttyRenderState,
        data: GhosttyRenderStateData,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_set(
        state: GhosttyRenderState,
        option: GhosttyRenderStateOption,
        value: *const c_void,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_row_iterator_new(
        allocator: *const c_void,
        out_iterator: *mut GhosttyRenderStateRowIterator,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_row_iterator_free(iterator: GhosttyRenderStateRowIterator);
    pub fn ghostty_render_state_row_iterator_next(iterator: GhosttyRenderStateRowIterator) -> bool;
    pub fn ghostty_render_state_row_get(
        iterator: GhosttyRenderStateRowIterator,
        data: GhosttyRenderStateRowData,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_row_set(
        iterator: GhosttyRenderStateRowIterator,
        option: GhosttyRenderStateRowOption,
        value: *const c_void,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_row_cells_new(
        allocator: *const c_void,
        out_cells: *mut GhosttyRenderStateRowCells,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_row_cells_free(cells: GhosttyRenderStateRowCells);
    pub fn ghostty_render_state_row_cells_next(cells: GhosttyRenderStateRowCells) -> bool;
    pub fn ghostty_render_state_row_cells_select(
        cells: GhosttyRenderStateRowCells,
        x: u16,
    ) -> GhosttyResult;
    pub fn ghostty_render_state_row_cells_get(
        cells: GhosttyRenderStateRowCells,
        data: GhosttyRenderStateRowCellsData,
        out: *mut c_void,
    ) -> GhosttyResult;

    pub fn ghostty_cell_get(
        cell: GhosttyCell,
        data: GhosttyCellData,
        out: *mut c_void,
    ) -> GhosttyResult;

    pub fn ghostty_terminal_get(
        terminal: GhosttyTerminal,
        data: GhosttyTerminalData,
        out: *mut c_void,
    ) -> GhosttyResult;
    pub fn ghostty_terminal_scroll_viewport(
        terminal: GhosttyTerminal,
        behavior: GhosttyTerminalScrollViewport,
    );
    pub fn ghostty_terminal_grid_ref(
        terminal: GhosttyTerminal,
        point: GhosttyPoint,
        out_ref: *mut GhosttyGridRef,
    ) -> GhosttyResult;

    pub fn ghostty_grid_ref_cell(
        grid_ref: *const GhosttyGridRef,
        out_cell: *mut GhosttyCell,
    ) -> GhosttyResult;
    pub fn ghostty_grid_ref_row(
        grid_ref: *const GhosttyGridRef,
        out_row: *mut GhosttyRow,
    ) -> GhosttyResult;
    pub fn ghostty_grid_ref_hyperlink_uri(
        grid_ref: *const GhosttyGridRef,
        buf: *mut u8,
        buf_len: usize,
        out_len: *mut usize,
    ) -> GhosttyResult;

    pub fn ghostty_row_get(
        row: GhosttyRow,
        data: GhosttyRowData,
        out: *mut c_void,
    ) -> GhosttyResult;

    pub fn ghostty_key_event_new(
        allocator: *const c_void,
        event: *mut GhosttyKeyEvent,
    ) -> GhosttyResult;
    pub fn ghostty_key_event_free(event: GhosttyKeyEvent);
    pub fn ghostty_key_event_set_action(event: GhosttyKeyEvent, action: GhosttyKeyAction);
    pub fn ghostty_key_event_set_key(event: GhosttyKeyEvent, key: GhosttyKey);
    pub fn ghostty_key_event_set_mods(event: GhosttyKeyEvent, mods: GhosttyMods);
    pub fn ghostty_key_event_set_consumed_mods(event: GhosttyKeyEvent, mods: GhosttyMods);
    pub fn ghostty_key_event_set_composing(event: GhosttyKeyEvent, composing: bool);
    /// The event does NOT take ownership of `utf8`; the pointer must stay
    /// valid until the event is encoded or the utf8 is replaced.
    pub fn ghostty_key_event_set_utf8(event: GhosttyKeyEvent, utf8: *const u8, len: usize);
    pub fn ghostty_key_event_set_unshifted_codepoint(event: GhosttyKeyEvent, codepoint: u32);

    pub fn ghostty_key_encoder_new(
        allocator: *const c_void,
        encoder: *mut GhosttyKeyEncoder,
    ) -> GhosttyResult;
    pub fn ghostty_key_encoder_free(encoder: GhosttyKeyEncoder);
    pub fn ghostty_key_encoder_setopt(
        encoder: GhosttyKeyEncoder,
        option: GhosttyKeyEncoderOption,
        value: *const c_void,
    );
    pub fn ghostty_key_encoder_setopt_from_terminal(
        encoder: GhosttyKeyEncoder,
        terminal: GhosttyTerminal,
    );
    pub fn ghostty_key_encoder_encode(
        encoder: GhosttyKeyEncoder,
        event: GhosttyKeyEvent,
        out_buf: *mut u8,
        out_buf_size: usize,
        out_len: *mut usize,
    ) -> GhosttyResult;

    pub fn ghostty_mouse_event_new(
        allocator: *const c_void,
        event: *mut GhosttyMouseEvent,
    ) -> GhosttyResult;
    pub fn ghostty_mouse_event_free(event: GhosttyMouseEvent);
    pub fn ghostty_mouse_event_set_action(event: GhosttyMouseEvent, action: GhosttyMouseAction);
    pub fn ghostty_mouse_event_set_button(event: GhosttyMouseEvent, button: GhosttyMouseButton);
    pub fn ghostty_mouse_event_clear_button(event: GhosttyMouseEvent);
    pub fn ghostty_mouse_event_set_mods(event: GhosttyMouseEvent, mods: GhosttyMods);
    pub fn ghostty_mouse_event_set_position(
        event: GhosttyMouseEvent,
        position: GhosttyMousePosition,
    );

    pub fn ghostty_mouse_encoder_new(
        allocator: *const c_void,
        encoder: *mut GhosttyMouseEncoder,
    ) -> GhosttyResult;
    pub fn ghostty_mouse_encoder_free(encoder: GhosttyMouseEncoder);
    pub fn ghostty_mouse_encoder_setopt(
        encoder: GhosttyMouseEncoder,
        option: GhosttyMouseEncoderOption,
        value: *const c_void,
    );
    pub fn ghostty_mouse_encoder_setopt_from_terminal(
        encoder: GhosttyMouseEncoder,
        terminal: GhosttyTerminal,
    );
    pub fn ghostty_mouse_encoder_reset(encoder: GhosttyMouseEncoder);
    pub fn ghostty_mouse_encoder_encode(
        encoder: GhosttyMouseEncoder,
        event: GhosttyMouseEvent,
        out_buf: *mut u8,
        out_buf_size: usize,
        out_len: *mut usize,
    ) -> GhosttyResult;

    /// `data` is modified in place (unsafe byte stripping) during
    /// encoding; both calls of the query-then-encode pattern are
    /// idempotent over the same buffer.
    pub fn ghostty_paste_encode(
        data: *mut u8,
        data_len: usize,
        bracketed: bool,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;

    pub fn ghostty_focus_encode(
        event: GhosttyFocusEvent,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;
}
